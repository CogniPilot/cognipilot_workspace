from __future__ import annotations

import json
from pathlib import Path
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests" / "fixtures" / "devenv-workspace" / "tests.nix"


class CognipilotDevenvWorkspaceTests(unittest.TestCase):
    def test_root_uses_the_pinned_devenv_evaluator_without_implicit_packages(self) -> None:
        flake = (ROOT / "flake.nix").read_text(encoding="utf-8")
        integration = (
            ROOT / "nix" / "cognipilot" / "devenv-flake-module.nix"
        ).read_text(encoding="utf-8")
        lock = json.loads((ROOT / "flake.lock").read_text(encoding="utf-8"))
        expected_revision = "407080febcc800abfd0fd688a0d513884aad620c"

        self.assertEqual(lock["nodes"]["devenv"]["locked"]["rev"], expected_revision)
        self.assertIn(
            f'url = "github:cachix/devenv/{expected_revision}";',
            flake,
        )
        self.assertNotIn("inputs.devenv.flakeModules.default", flake)
        self.assertIn("./nix/cognipilot/devenv-flake-module.nix", flake)
        self.assertIn("devenv.lib.mkEval", integration)
        self.assertNotIn("config.packages", integration)
        self.assertIn(
            "devenvLaunches = ./nix/cognipilot/devenv-launch-module.nix;",
            flake,
        )
        self.assertIn(
            "devenvWorkspace = ./nix/cognipilot/devenv-workspace-module.nix;",
            flake,
        )
        self.assertIn(
            "nixspace = ./nix/nixspace/index-module.nix;",
            flake,
        )
        self.assertIn(
            "nixspaceTool = ./nix/nixspace/tool-module.nix;",
            flake,
        )
        self.assertIn(
            "nixspaceCogniPilot = ./nix/cognipilot/nixspace-module.nix;",
            flake,
        )
        self.assertNotIn("wsTool", flake)

    def test_root_packages_exclude_deprecated_and_implicit_container_outputs(self) -> None:
        result = subprocess.run(
            [
                "nix",
                "eval",
                "--accept-flake-config",
                "--offline",
                "--json",
                ".#packages.x86_64-linux",
                "--apply",
                "builtins.attrNames",
            ],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        names = json.loads(result.stdout)
        self.assertFalse(
            any(
                name.endswith(("container-processes", "container-shell"))
                or name.endswith(("devenv-test", "devenv-up"))
                for name in names
            ),
            names,
        )

    def test_workspace_shells_evaluate_against_pinned_devenv(self) -> None:
        result = subprocess.run(
            [
                "nix",
                "eval",
                "--offline",
                "--impure",
                "--json",
                "--file",
                str(FIXTURE),
            ],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        checks = json.loads(result.stdout)
        self.assertTrue(checks)
        self.assertTrue(all(checks.values()), checks)


if __name__ == "__main__":
    unittest.main()
