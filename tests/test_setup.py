from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class SetupBootstrapTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        shutil.copy2(ROOT / "setup", self.root / "setup")
        (self.root / "setup").chmod(0o755)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.log = self.root / "nix.log"
        nix = self.bin / "nix"
        nix.write_text(
            """#!/bin/sh
set -eu
printf '%s\\n' "$*" >> "$TEST_NIX_LOG"
case " $* " in
  *" build "*)
    mkdir -p "$TEST_ROOT/.nixspace/ws/bin"
    cat > "$TEST_ROOT/.nixspace/ws/bin/ws" <<'EOF'
#!/bin/sh
printf 'client:%s\\n' "$*"
EOF
    chmod +x "$TEST_ROOT/.nixspace/ws/bin/ws"
    ;;
esac
""",
            encoding="utf-8",
        )
        nix.chmod(0o755)
        self.environment = os.environ.copy()
        self.environment.update(
            {
                "PATH": f"{self.bin}{os.pathsep}{self.environment['PATH']}",
                "TEST_NIX_LOG": str(self.log),
                "TEST_ROOT": str(self.root),
            }
        )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def run_setup(self, *arguments: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(self.root / "setup"), *arguments],
            env=self.environment,
            text=True,
            capture_output=True,
            check=False,
        )

    def install_client(self) -> None:
        client = self.root / ".nixspace" / "ws" / "bin" / "ws"
        client.parent.mkdir(parents=True)
        client.write_text("#!/bin/sh\nprintf 'client:%s\\n' \"$*\"\n", encoding="utf-8")
        client.chmod(0o755)

    def test_setup_delegates_host_policy_then_realizes_cached_client(self) -> None:
        result = self.run_setup()

        self.assertEqual(0, result.returncode, result.stderr)
        calls = self.log.read_text(encoding="utf-8").splitlines()
        self.assertEqual(2, len(calls))
        self.assertIn("run --accept-flake-config", calls[0])
        self.assertIn("#nixspace-host", calls[0])
        self.assertIn("--workspace-root", calls[0])
        self.assertIn("build --accept-flake-config --out-link", calls[1])
        self.assertIn("#ws", calls[1])
        self.assertIn("client:--workspace-root", result.stdout)
        self.assertTrue(result.stdout.rstrip().endswith("doctor"))

    def test_check_uses_cached_client_without_nix_evaluation(self) -> None:
        self.install_client()

        result = self.run_setup("--check")

        self.assertEqual(0, result.returncode, result.stderr)
        self.assertFalse(self.log.exists())
        self.assertEqual(
            f"client:--workspace-root {self.root} setup --check\n", result.stdout
        )

    def test_check_explains_missing_cached_client(self) -> None:
        result = self.run_setup("--check")

        self.assertEqual(1, result.returncode)
        self.assertIn("cached workspace client is missing", result.stderr)
        self.assertFalse(self.log.exists())


if __name__ == "__main__":
    unittest.main()
