from __future__ import annotations

import json
import pathlib
import shutil
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
COMMAND_TIMEOUT_SECONDS = 30
PROOF_LAYOUT = (
    ("nix/cognipilot/action-presets.nix", "nix/cognipilot/action-presets.nix"),
    (
        "nix/cognipilot/action-tool-profiles.nix",
        "nix/cognipilot/action-tool-profiles.nix",
    ),
    ("nix/cognipilot/flake-module.nix", "nix/cognipilot/flake-module.nix"),
    (
        "nix/cognipilot/resolution-template.nix",
        "nix/cognipilot/resolution-template.nix",
    ),
    (
        "nix/cognipilot/project-flake-module.nix",
        "nix/cognipilot/project-flake-module.nix",
    ),
    ("nix/nixspace/index-module.nix", "nix/nixspace/index-module.nix"),
    ("nix/nixspace/tool-module.nix", "nix/nixspace/tool-module.nix"),
    (
        "tests/fixtures/project-output/definition/flake.nix",
        "definition/flake.nix",
    ),
    (
        "tests/fixtures/project-output/definition/module.nix",
        "definition/module.nix",
    ),
    ("tests/fixtures/project-output/flake.lock", "flake.lock"),
    ("tests/fixtures/project-output/flake.nix", "flake.nix"),
    ("tests/fixtures/project-output/source/README.md", "source/README.md"),
    ("tools/nixspace/Cargo.lock", "tools/nixspace/Cargo.lock"),
    ("tools/nixspace/Cargo.toml", "tools/nixspace/Cargo.toml"),
    ("tools/nixspace/LICENSE", "tools/nixspace/LICENSE"),
    ("tools/nixspace/README.md", "tools/nixspace/README.md"),
    ("tools/nixspace/src/action.rs", "tools/nixspace/src/action.rs"),
    ("tools/nixspace/src/benchmark.rs", "tools/nixspace/src/benchmark.rs"),
    (
        "tools/nixspace/src/closure_materialization.rs",
        "tools/nixspace/src/closure_materialization.rs",
    ),
    ("tools/nixspace/src/index.rs", "tools/nixspace/src/index.rs"),
    ("tools/nixspace/src/launch.rs", "tools/nixspace/src/launch.rs"),
    ("tools/nixspace/src/main.rs", "tools/nixspace/src/main.rs"),
    ("tools/nixspace/src/model.rs", "tools/nixspace/src/model.rs"),
    ("tools/nixspace/src/resolution.rs", "tools/nixspace/src/resolution.rs"),
    ("tools/nixspace/src/west.rs", "tools/nixspace/src/west.rs"),
    ("tools/nixspace/tests/action.rs", "tools/nixspace/tests/action.rs"),
    ("tools/nixspace/tests/benchmark.rs", "tools/nixspace/tests/benchmark.rs"),
    (
        "tools/nixspace/tests/closure_materialization.rs",
        "tools/nixspace/tests/closure_materialization.rs",
    ),
    ("tools/nixspace/tests/index.rs", "tools/nixspace/tests/index.rs"),
    ("tools/nixspace/tests/package.rs", "tools/nixspace/tests/package.rs"),
    ("tools/nixspace/tests/query.rs", "tools/nixspace/tests/query.rs"),
    ("tools/nixspace/tests/resolution.rs", "tools/nixspace/tests/resolution.rs"),
    ("tools/nixspace/tests/west.rs", "tools/nixspace/tests/west.rs"),
)
PROOF_FILES = tuple(destination for _, destination in PROOF_LAYOUT)
STANDALONE_FLAKE = "path:."


@unittest.skipUnless(shutil.which("nix"), "Nix is required for flake evaluation")
class CognipilotProjectOutputTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.temporary = tempfile.TemporaryDirectory()
        cls.proof_root = pathlib.Path(cls.temporary.name)

        for source_relative, destination_relative in PROOF_LAYOUT:
            source = ROOT / source_relative
            destination = cls.proof_root / destination_relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)

        cls.run_command("git", "init", "-q")
        cls.run_command("git", "add", ".")
        cls.run_command(
            "git",
            "-c",
            "user.name=CogniPilot Test",
            "-c",
            "user.email=test@invalid",
            "commit",
            "-qm",
            "isolated standalone project proof",
        )

    @classmethod
    def tearDownClass(cls) -> None:
        cls.temporary.cleanup()

    @classmethod
    def run_command(
        cls, *command: str, check: bool = True
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            command,
            cwd=cls.proof_root,
            check=check,
            capture_output=True,
            text=True,
            timeout=COMMAND_TIMEOUT_SECONDS,
        )

    @classmethod
    def nix_eval(cls, attribute: str, *extra: str) -> object:
        result = cls.run_command(
            "nix",
            "eval",
            "--offline",
            "--json",
            f"{STANDALONE_FLAKE}#{attribute}",
            *extra,
        )
        return json.loads(result.stdout)

    def test_proof_is_tiny_tracked_and_source_remains_non_flake(self) -> None:
        tracked = self.run_command("git", "ls-files").stdout.splitlines()
        self.assertEqual(sorted(PROOF_FILES), tracked)
        self.assertFalse((self.proof_root / "source" / "flake.nix").exists())

        lock = json.loads((self.proof_root / "flake.lock").read_text())
        self.assertFalse(lock["nodes"]["fake_source"]["flake"])
        self.assertEqual(7, lock["version"])

        metadata = self.run_command(
            "nix",
            "flake",
            "metadata",
            "--offline",
            "--json",
            STANDALONE_FLAKE,
        )
        self.assertEqual(7, json.loads(metadata.stdout)["locks"]["version"])

    def test_external_definition_exports_a_default_project_module(self) -> None:
        definition = "path:./definition"
        result = self.run_command(
            "nix",
            "eval",
            "--offline",
            "--json",
            f"{definition}#flakeModules.default.cognipilot.projects",
            "--apply",
            "builtins.attrNames",
        )
        self.assertEqual(["fake-app"], json.loads(result.stdout))
        metadata = self.run_command(
            "nix",
            "eval",
            "--offline",
            "--json",
            f"{definition}#flakeModules.default.cognipilot.projects.fake-app.definition",
        )
        self.assertEqual(
            {"input": "fake_definition", "origin": "external"},
            json.loads(metadata.stdout),
        )

    def test_standalone_and_direct_contract_normalize_identically(self) -> None:
        standalone = self.nix_eval("cognipilotIndex")
        self.assertEqual("nixspace/v1", standalone["apiVersion"])
        self.assertEqual("Workspace", standalone["kind"])
        definition = self.proof_root / "definition"
        contract = self.proof_root / "nix" / "cognipilot" / "flake-module.nix"
        expression = f"""
          let
            pkgs = import <nixpkgs> {{ }};
            definition = (builtins.getFlake "path:{definition}").flakeModules.default;
            evaluated = pkgs.lib.evalModules {{
              modules = [ {contract} definition ];
            }};
          in evaluated.config.cognipilot.validatedIndex
        """
        direct = self.run_command(
            "nix",
            "eval",
            "--impure",
            "--json",
            "--expr",
            expression,
        )

        self.assertEqual(standalone, json.loads(direct.stdout))
        project = standalone["projects"]["fake-app"]
        self.assertEqual("fake_source", project["source"]["input"])

    def test_conventional_outputs_are_eval_only_and_have_no_default(self) -> None:
        self.assertEqual(
            ["nixspace", "nixspace-completions", "nixspace-index"],
            self.nix_eval("packages.x86_64-linux", "--apply", "builtins.attrNames"),
        )
        self.assertEqual(
            ["nixspace-interface", "nixspace-standalone"],
            self.nix_eval("checks.x86_64-linux", "--apply", "builtins.attrNames"),
        )
        self.assertEqual(
            "nixspace-interface",
            self.nix_eval("checks.x86_64-linux.nixspace-interface.name"),
        )
        self.assertEqual(
            ["default"],
            self.nix_eval("devShells.x86_64-linux", "--apply", "builtins.attrNames"),
        )
        self.assertEqual(
            ["nixspace", "show-index"],
            self.nix_eval("apps.x86_64-linux", "--apply", "builtins.attrNames"),
        )
        self.assertEqual("app", self.nix_eval("apps.x86_64-linux.show-index.type"))
        self.assertEqual(
            "derivation",
            self.nix_eval("formatter.x86_64-linux.type"),
        )
        self.assertNotIn(
            "default",
            self.nix_eval("packages.x86_64-linux", "--apply", "builtins.attrNames"),
        )

    def evaluate_project_count(self, projects: str) -> subprocess.CompletedProcess[str]:
        contract = self.proof_root / "nix" / "cognipilot" / "flake-module.nix"
        output = self.proof_root / "nix" / "cognipilot" / "project-flake-module.nix"
        expression = f"""
          let
            pkgs = import <nixpkgs> {{ }};
            evaluated = pkgs.lib.evalModules {{
              modules = [
                {{
                  options = {{
                    flake.nixspaceIndex = pkgs.lib.mkOption {{
                      type = pkgs.lib.types.attrs;
                    }};
                    flake.cognipilotIndex = pkgs.lib.mkOption {{
                      type = pkgs.lib.types.attrs;
                    }};
                    perSystem = pkgs.lib.mkOption {{
                      type = pkgs.lib.types.raw;
                    }};
                  }};
                }}
                {contract}
                {output}
                {{ cognipilot.projects = {projects}; }}
              ];
            }};
          in evaluated.config.flake.nixspaceIndex
        """
        return self.run_command(
            "nix",
            "eval",
            "--impure",
            "--json",
            "--expr",
            expression,
            check=False,
        )

    def test_standalone_mode_requires_exactly_one_selected_project(self) -> None:
        project = (
            '{ repositoryId = "repo"; source.input = "source"; preset = "cargo-v1"; }'
        )
        cases = {
            "{ }": "found 0: <none>",
            f"{{ one = {project}; two = {project}; }}": "found 2: one, two",
        }
        for projects, diagnostic in cases.items():
            with self.subTest(diagnostic=diagnostic):
                result = self.evaluate_project_count(projects)
                self.assertNotEqual(0, result.returncode)
                self.assertIn(
                    "standalone project output requires exactly one selected project",
                    result.stderr,
                )
                self.assertIn(diagnostic, result.stderr)


if __name__ == "__main__":
    unittest.main()
