from __future__ import annotations

import json
import pathlib
import shutil
import subprocess
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests" / "fixtures" / "nixspace-generic" / "tests.nix"


@unittest.skipUnless(shutil.which("nix"), "Nix is required for module evaluation")
class NixspaceGenericModuleTests(unittest.TestCase):
    def test_generic_provider_has_no_cognipilot_dependency(self) -> None:
        result = subprocess.run(
            ["nix", "eval", "--impure", "--json", "--file", str(FIXTURE)],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        )
        self.assertEqual(
            {
                "success": True,
                "package": "nixspace",
                "interface": "nixspace/v1",
            },
            json.loads(result.stdout),
        )


if __name__ == "__main__":
    unittest.main()
