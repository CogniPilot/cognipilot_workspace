from __future__ import annotations

import json
import pathlib
import shutil
import subprocess
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests" / "fixtures" / "nixspace-generic" / "tests.nix"
HOST_SECRET_FIXTURE = (
    ROOT / "tests" / "fixtures" / "nixspace-host-secrets" / "tests.nix"
)


@unittest.skipUnless(shutil.which("nix"), "Nix is required for module evaluation")
class NixspaceGenericModuleTests(unittest.TestCase):
    def evaluate_fixture(self, fixture: pathlib.Path) -> dict[str, object]:
        result = subprocess.run(
            ["nix", "eval", "--impure", "--json", "--file", str(fixture)],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        )
        return json.loads(result.stdout)

    def test_generic_provider_has_no_cognipilot_dependency(self) -> None:
        self.assertEqual(
            {
                "success": True,
                "package": "nixspace",
                "interface": "nixspace/v1",
            },
            self.evaluate_fixture(FIXTURE),
        )

    def test_host_plan_rejects_secret_settings_before_store_serialization(self) -> None:
        self.assertEqual(
            {"success": True, "rejectedCount": 8},
            self.evaluate_fixture(HOST_SECRET_FIXTURE),
        )

    def test_reusable_provider_and_cargo_package_have_no_product_names(self) -> None:
        roots = (ROOT / "nix" / "nixspace", ROOT / "tools" / "nixspace")
        checked_suffixes = {".md", ".nix", ".rs", ".toml"}
        for root in roots:
            for path in root.rglob("*"):
                if (
                    not path.is_file()
                    or "target" in path.parts
                    or path.suffix not in checked_suffixes
                ):
                    continue
                self.assertNotIn(
                    "cognipilot",
                    path.read_text(encoding="utf-8").lower(),
                    path.relative_to(ROOT),
                )

        generic_nix = "\n".join(
            path.read_text(encoding="utf-8")
            for path in (ROOT / "nix" / "nixspace").rglob("*.nix")
        )
        self.assertNotIn(".devenv/", generic_nix)


if __name__ == "__main__":
    unittest.main()
