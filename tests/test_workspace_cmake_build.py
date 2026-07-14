from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
CMAKE_BUILD = ROOT / "scripts" / "workspace-cmake-build"


class WorkspaceCmakeBuildTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.build = self.root / "build"
        self.config = self.root / "flake.lock"
        self.config.write_text("one\n", encoding="utf-8")
        self.bin = self.root / "bin"
        self.bin.mkdir()
        fake_nix = self.bin / "nix"
        fake_nix.write_text(
            '#!/bin/sh\nprintf \'nix:%s\\n\' "$*"\ntest "${FAKE_NIX_FAIL:-0}" != 1\n',
            encoding="utf-8",
        )
        fake_nix.chmod(0o755)
        self.environment = os.environ.copy()
        self.environment["PATH"] = f"{self.bin}:{self.environment['PATH']}"

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def run_build(self) -> subprocess.CompletedProcess[str]:
        fallback = (
            f"mkdir -p {self.build}; touch {self.build / 'build.ninja'}; echo fallback"
        )
        return subprocess.run(
            [
                str(CMAKE_BUILD),
                "--build-dir",
                str(self.build),
                "--flake-ref",
                "git+file:///component",
                "--config-file",
                str(self.config),
                "--config-value",
                "board=native_sim",
                "--",
                "bash",
                "-c",
                fallback,
            ],
            env=self.environment,
            text=True,
            capture_output=True,
            check=False,
        )

    def test_unchanged_configuration_uses_direct_cmake_build(self) -> None:
        first = self.run_build()
        second = self.run_build()

        self.assertEqual(first.returncode, 0, first.stderr)
        self.assertIn("fallback", first.stdout)
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertIn("incremental CMake build", second.stdout)
        self.assertIn("cmake --build", second.stdout)
        self.assertNotIn("fallback", second.stdout)

    def test_configuration_change_returns_to_fallback(self) -> None:
        self.assertEqual(self.run_build().returncode, 0)
        self.config.write_text("two\n", encoding="utf-8")

        changed = self.run_build()

        self.assertEqual(changed.returncode, 0, changed.stderr)
        self.assertIn("fallback", changed.stdout)

    def test_failed_incremental_build_reconfigures_with_fallback(self) -> None:
        self.assertEqual(self.run_build().returncode, 0)
        self.environment["FAKE_NIX_FAIL"] = "1"

        retried = self.run_build()

        self.assertEqual(retried.returncode, 0, retried.stderr)
        self.assertIn("retrying through west", retried.stderr)
        self.assertIn("fallback", retried.stdout)


if __name__ == "__main__":
    unittest.main()
