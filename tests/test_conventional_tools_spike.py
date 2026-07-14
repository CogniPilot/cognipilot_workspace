from __future__ import annotations

import json
import pathlib
import re
import shutil
import subprocess
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
SPIKE = ROOT / "nix" / "spikes" / "conventional-tools"
REPORT = ROOT / "dev" / "conventional-tools-compatibility-spike.md"
EXPECTED_PINS = {
    "flake-parts": "17c9d6cdfc60c64f4ee8d306f9bc0b4ccb51481e",
    "std": "4177882c378184b795fa97594b5effd062213891",
    "west2nix": "f84670d66f881d9340b7d7626fbfe499438c134b",
    "zephyr-nix": "6966fb1cbf2fdb494bea3062c5e8e7d44dd8ac9c",
}
EXPECTED_MODULE_PATHS = [
    "modules/hal/cmsis",
    "modules/hal/cmsis_6",
    "modules/hal/nxp",
    "modules/lib/zenoh-pico",
    "modules/lib/cerebri_lockstep",
    "modules/lib/zros",
    "modules/lib/csyn",
    "modules/lib/zephyr_boards",
]


def parse_cubs2_west_manifest(path: pathlib.Path) -> list[dict[str, str]]:
    """Parse the deliberately simple project/remotes subset used by CUBS2."""
    lines = path.read_text().splitlines()
    remotes: dict[str, str] = {}
    projects: list[dict[str, str]] = []
    section = ""
    current: dict[str, str] | None = None

    def value(raw: str) -> str:
        return raw.split(" #", 1)[0].strip()

    for line in lines:
        if line == "  remotes:":
            section = "remotes"
            current = None
            continue
        if line == "  projects:":
            section = "projects"
            current = None
            continue
        name_match = re.fullmatch(r"    - name: (.+)", line)
        if name_match:
            current = {"name": value(name_match.group(1))}
            if section == "projects":
                projects.append(current)
            continue
        field_match = re.fullmatch(r"      ([a-z-]+): (.+)", line)
        if current is None or field_match is None:
            continue
        key, raw = field_match.groups()
        current[key] = value(raw)
        if section == "remotes" and key == "url-base":
            remotes[current["name"]] = current[key]

    resolved = []
    for project in projects:
        name = project["name"]
        url = project.get("url")
        if url is None:
            url = f"{remotes[project['remote']]}/{project.get('repo-path', name)}"
        resolved.append(
            {
                "name": name,
                "path": project.get("path", name),
                "revision": project["revision"],
                "url": url,
            }
        )
    return resolved


class ConventionalToolsStaticTests(unittest.TestCase):
    def test_spike_has_an_independent_exact_lock(self) -> None:
        lock = json.loads((SPIKE / "flake.lock").read_text())
        root_inputs = lock["nodes"]["root"]["inputs"]
        for name, revision in EXPECTED_PINS.items():
            node_name = root_inputs[name]
            self.assertEqual(revision, lock["nodes"][node_name]["locked"]["rev"])

        std_node = lock["nodes"][root_inputs["std"]]
        self.assertNotEqual(root_inputs["nixpkgs"], std_node["inputs"]["nixpkgs"])
        self.assertNotEqual(
            (ROOT / "flake.lock").resolve(), (SPIKE / "flake.lock").resolve()
        )

    def test_report_records_explicit_non_legacy_decisions(self) -> None:
        report = REPORT.read_text()
        self.assertIn("Decision: REJECT Standard Cells as workspace discovery", report)
        self.assertIn("Decision: CONDITIONAL ADOPTION for pure releases", report)
        self.assertIn("adopt as the Zephyr tool provider", report)
        self.assertIn("Do not add a CogniPilot-specific manifest resolver", report)

    def test_evaluation_only_west_snapshot_cannot_be_promoted_by_accident(self) -> None:
        manifest = (SPIKE / "cubs2-frozen-manifest.nix").read_text()
        readme = (SPIKE / "README.md").read_text()
        self.assertIn("sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=", manifest)
        self.assertIn("Do not build `cubs2WestHook`", readme)
        self.assertIn("Generate real hashes", readme)

    def test_actual_cubs2_manifest_modules_and_native_boards_match_the_audit(
        self,
    ) -> None:
        cubs2 = ROOT / "src" / "cerebri_cubs2"
        if not cubs2.exists():
            self.skipTest("the independently synchronized CUBS2 source is absent")

        manifest_text = (cubs2 / "west.yml").read_text()
        self.assertNotRegex(manifest_text, r"(?m)^\s+import:")

        cmake = (cubs2 / "CMakeLists.txt").read_text()
        module_block = re.search(r"set\(ZEPHYR_MODULES(.*?)\n\)", cmake, re.DOTALL)
        self.assertIsNotNone(module_block)
        module_paths = re.findall(
            r"\$\{WORKSPACE_ROOT\}/([^\s)]+)", module_block.group(1)
        )
        self.assertEqual(EXPECTED_MODULE_PATHS, module_paths)

        flake = (cubs2 / "flake.nix").read_text()
        self.assertIn('defaultNativeSimBoard = "native_sim";', flake)
        self.assertIn('defaultNativeSim64Board = "native_sim/native/64";', flake)

        west_topdir = ROOT.parent
        if all((west_topdir / module).is_dir() for module in EXPECTED_MODULE_PATHS):
            for module in EXPECTED_MODULE_PATHS:
                self.assertTrue((west_topdir / module / "zephyr" / "module.yml").is_file())


@unittest.skipUnless(shutil.which("nix"), "Nix is required for spike evaluation")
class ConventionalToolsEvaluationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        result = subprocess.run(
            [
                "nix",
                "eval",
                "--offline",
                "--json",
                f"path:{SPIKE}#conventionalToolSpike",
            ],
            check=False,
            capture_output=True,
            text=True,
            timeout=45,
        )
        if result.returncode:
            if "not available in offline mode" in result.stderr:
                raise unittest.SkipTest("the pinned spike inputs are not cached")
            raise AssertionError(result.stderr)
        cls.evaluated = json.loads(result.stdout)

    def test_same_typed_fixture_is_discovered_by_both_frameworks(self) -> None:
        comparison = self.evaluated["comparison"]
        self.assertTrue(comparison["sameProjects"])
        self.assertEqual(
            comparison["flakeParts"]["projects"], comparison["std"]["projects"]
        )
        self.assertEqual(
            {"consumer", "producer"}, set(comparison["std"]["projects"])
        )

    def test_evaluated_interfaces_and_pins_are_exact(self) -> None:
        self.assertEqual(EXPECTED_PINS, self.evaluated["pins"])
        self.assertEqual(["pkgs"], self.evaluated["west2nix"]["libraryFunctionArguments"])
        self.assertTrue(
            {"mkWest2nixHook", "west2nix"}.issubset(
                self.evaluated["west2nix"]["apiNames"]
            )
        )
        self.assertTrue(
            {"hosttools", "hosttools-nix", "pythonEnv", "sdk", "sdkFull"}.issubset(
                self.evaluated["zephyrNix"]["packageNames"]
            )
        )

    def test_west2nix_hook_accepts_the_actual_flattened_cubs2_closure(self) -> None:
        cubs2 = self.evaluated["west2nix"]["cubs2"]
        self.assertEqual("west2nix-project-hook.sh", cubs2["hookName"])
        self.assertEqual("cerebri_cubs2", cubs2["selfPath"])
        self.assertEqual(10, len(cubs2["projects"]))

        manifest = ROOT / "src" / "cerebri_cubs2" / "west.yml"
        if manifest.exists():
            self.assertEqual(parse_cubs2_west_manifest(manifest), cubs2["projects"])


if __name__ == "__main__":
    unittest.main()
