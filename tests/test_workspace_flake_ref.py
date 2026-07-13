import os
import pathlib
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
FLAKE_REF = ROOT / "scripts" / "workspace-flake-ref"


def git(path: pathlib.Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(path), *args],
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


class WorkspaceFlakeRefTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.tempdir.name)
        self.submodule = self.root / "submodule"
        self.component = self.root / "component"
        for path in (self.submodule, self.component):
            path.mkdir()
            git(path, "init", "--initial-branch=main")
            git(path, "config", "user.email", "test@example.com")
            git(path, "config", "user.name", "Workspace Test")

        (self.submodule / "README.md").write_text("submodule\n")
        git(self.submodule, "add", "README.md")
        git(self.submodule, "commit", "-m", "Initialize submodule")

        (self.component / "README.md").write_text("component\n")
        git(self.component, "add", "README.md")
        git(self.component, "commit", "-m", "Initialize component")
        git(
            self.component,
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            str(self.submodule),
            "third_party/example",
        )
        git(self.component, "commit", "-m", "Add submodule")

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def flake_ref(self) -> str:
        result = subprocess.run(
            [str(FLAKE_REF), "--mode", "local", str(self.component)],
            check=True,
            capture_output=True,
            text=True,
            env={**os.environ, "GIT_ALLOW_PROTOCOL": "file"},
        )
        return result.stdout.strip()

    def test_initialized_submodule_is_included(self) -> None:
        self.assertTrue(self.flake_ref().endswith("?submodules=1"))

    def test_uninitialized_submodule_is_not_fetched(self) -> None:
        git(self.component, "submodule", "deinit", "-f", "--all")
        self.assertNotIn("submodules=1", self.flake_ref())


if __name__ == "__main__":
    unittest.main()
