from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
RELEASE_ENV = ROOT / "scripts" / "workspace-release-environment"


class ReleaseEnvironmentTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.repository = Path(self.temporary.name) / "component"
        self.repository.mkdir()
        subprocess.run(["git", "init", "-q", str(self.repository)], check=True)
        subprocess.run(
            ["git", "-C", str(self.repository), "config", "user.name", "Test"],
            check=True,
        )
        subprocess.run(
            [
                "git",
                "-C",
                str(self.repository),
                "config",
                "user.email",
                "test@example.com",
            ],
            check=True,
        )
        (self.repository / "source.txt").write_text("clean\n", encoding="utf-8")
        subprocess.run(
            ["git", "-C", str(self.repository), "add", "source.txt"], check=True
        )
        subprocess.run(
            ["git", "-C", str(self.repository), "commit", "-q", "-m", "initial"],
            check=True,
        )
        self.devel = Path(self.temporary.name) / "devel"
        self.environment = os.environ.copy()
        self.environment.update(
            {
                "COGNIPILOT_DEVEL_ROOT": str(self.devel),
                "PATH": f"{self.devel / 'bin'}:{self.environment['PATH']}",
                "PYTHONPATH": f"{self.devel / 'python'}:/fixed/python",
            }
        )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def run_release(self, command: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(RELEASE_ENV), "bash", "-c", command],
            cwd=self.repository,
            env=self.environment,
            text=True,
            capture_output=True,
            check=False,
        )

    def test_clean_release_strips_devel_and_records_revision(self) -> None:
        result = self.run_release(
            'printf "%s\\n%s\\n%s\\n%s\\n" '
            '"${COGNIPILOT_DEVEL_ROOT-unset}" "$PATH" "$PYTHONPATH" '
            '"$COGNIPILOT_COMPONENT_REVISION"'
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        devel, path, pythonpath, revision = result.stdout.splitlines()
        self.assertEqual(devel, "unset")
        self.assertNotIn(str(self.devel), path)
        self.assertEqual(pythonpath, "/fixed/python")
        self.assertEqual(len(revision), 40)

    def test_dirty_release_is_rejected(self) -> None:
        (self.repository / "source.txt").write_text("dirty\n", encoding="utf-8")

        result = self.run_release("true")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("clean component worktree", result.stderr)


if __name__ == "__main__":
    unittest.main()
