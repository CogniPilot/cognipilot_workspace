from __future__ import annotations

from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class WorkspaceWrapperTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        shutil.copy2(ROOT / "ws", self.root / "ws")
        (self.root / "ws").chmod(0o755)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def run_ws(self, *arguments: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(self.root / "ws"), *arguments],
            text=True,
            capture_output=True,
            check=False,
        )

    def test_wrapper_executes_only_the_cached_nixspace_client(self) -> None:
        client = self.root / ".nixspace" / "ws" / "bin" / "ws"
        client.parent.mkdir(parents=True)
        client.write_text("#!/bin/sh\nprintf '%s\\n' \"$*\"\n", encoding="utf-8")
        client.chmod(0o755)

        result = self.run_ws("package", "list")

        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual(
            f"--workspace-root {self.root} package list\n", result.stdout
        )

    def test_wrapper_has_one_actionable_bootstrap_failure(self) -> None:
        result = self.run_ws("doctor")

        self.assertEqual(1, result.returncode)
        self.assertIn("run ./setup", result.stderr)


if __name__ == "__main__":
    unittest.main()
