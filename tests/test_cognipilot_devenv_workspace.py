from __future__ import annotations

import json
from pathlib import Path
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests" / "fixtures" / "devenv-workspace" / "tests.nix"


class CognipilotDevenvWorkspaceTests(unittest.TestCase):
    def test_root_imports_the_exact_pinned_official_module(self) -> None:
        flake = (ROOT / "flake.nix").read_text(encoding="utf-8")
        lock = json.loads((ROOT / "flake.lock").read_text(encoding="utf-8"))
        expected_revision = "407080febcc800abfd0fd688a0d513884aad620c"

        self.assertEqual(lock["nodes"]["devenv"]["locked"]["rev"], expected_revision)
        self.assertIn(
            f'url = "github:cachix/devenv/{expected_revision}";',
            flake,
        )
        self.assertIn("inputs.devenv.flakeModules.default", flake)
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
