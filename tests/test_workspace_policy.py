from __future__ import annotations

import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
MODULE = ROOT / "nix/cognipilot/workspace-policy-module.nix"


@unittest.skipUnless(shutil.which("nix"), "Nix is required")
class WorkspacePolicyTests(unittest.TestCase):
    def evaluate(self, source: Path) -> subprocess.CompletedProcess[str]:
        expression = f"""
          let
            pkgs = import <nixpkgs> {{ }};
            lib = pkgs.lib;
            evaluated = lib.evalModules {{
              modules = [
                ({{ lib, ... }}: {{
                  options.perSystem = lib.mkOption {{
                    type = lib.types.deferredModule;
                    default = {{ }};
                  }};
                }})
                (import (builtins.toPath {json.dumps(str(MODULE))}))
                {{
                  cognipilot.workspacePolicy.source =
                    builtins.path {{ path = builtins.toPath {json.dumps(str(source))}; }};
                }}
              ];
            }};
          in builtins.deepSeq evaluated.config.cognipilot.workspacePolicy.report
            evaluated.config.cognipilot.workspacePolicy.report
        """
        return subprocess.run(
            ["nix", "eval", "--impure", "--json", "--expr", expression],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )

    def test_only_exact_bootstrap_shell_files_and_test_python_are_allowed(self) -> None:
        with tempfile.TemporaryDirectory(prefix="nixspace-policy-") as temporary:
            source = Path(temporary)
            (source / "tests").mkdir()
            (source / "setup").write_text("#!/bin/sh\nexec true\n", encoding="utf-8")
            (source / "ws").write_text("#!/bin/sh\nexec true\n", encoding="utf-8")
            (source / ".envrc").write_text(
                "#!/usr/bin/env bash\nuse flake\n", encoding="utf-8"
            )
            (source / "tests/helper.py").write_text("pass\n", encoding="utf-8")
            (source / "README.md").write_text("fixture\n", encoding="utf-8")

            allowed = self.evaluate(source)
            self.assertEqual(allowed.returncode, 0, allowed.stderr)
            report = json.loads(allowed.stdout)
            self.assertTrue(report["compliant"])
            self.assertEqual(
                report["policy"]["allowedShellFiles"], [".envrc", "setup", "ws"]
            )
            self.assertEqual(report["policy"]["shell"], "exact-bootstrap-allowlist")

            violations = {
                "rogue.bash": "printf hidden-shell",
                "windows-control.ps1": "Write-Output hidden-shell\n",
                "extensionless-tool": "#!/usr/bin/env -S ash -e\ntrue\n",
                "control.py": "raise SystemExit\n",
                "python-tool": "#!/usr/bin/env -S python3 -I\nraise SystemExit\n",
                "windows-python.pyw": "raise SystemExit\n",
            }
            for relative, contents in violations.items():
                path = source / relative
                path.write_text(contents, encoding="utf-8")
                rejected = self.evaluate(source)
                self.assertNotEqual(rejected.returncode, 0, relative)
                self.assertIn(relative, rejected.stderr)
                path.unlink()


if __name__ == "__main__":
    unittest.main()
