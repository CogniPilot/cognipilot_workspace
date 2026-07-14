from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT / "scripts" / "workspace-task-cache"
CACHED_TASK = ROOT / "scripts" / "workspace-cached-task"


class WorkspaceTaskCacheTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.repository = self.root / "repository"
        self.repository.mkdir()
        subprocess.run(["git", "init", "-q", str(self.repository)], check=True)
        (self.repository / ".gitignore").write_text("generated/\n", encoding="utf-8")
        (self.repository / "source.txt").write_text("one\n", encoding="utf-8")
        subprocess.run(
            ["git", "-C", str(self.repository), "add", ".gitignore", "source.txt"],
            check=True,
        )
        self.build = self.root / "build"
        self.devel = self.root / "devel"
        self.release_results = self.root / "release-results"
        (self.devel / "demo").mkdir(parents=True)
        (self.devel / "demo" / "artifact.txt").write_text(
            "artifact\n", encoding="utf-8"
        )
        self.environment = os.environ.copy()
        self.environment.update(
            {
                "COGNIPILOT_WORKSPACE_ROOT": str(ROOT),
                "COGNIPILOT_BUILD_ROOT": str(self.build),
                "COGNIPILOT_DEVEL_ROOT": str(self.devel),
                "COGNIPILOT_RELEASE_RESULTS": str(self.release_results),
            }
        )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def cache(
        self, action: str, *outputs: str
    ) -> subprocess.CompletedProcess[str]:
        if not outputs:
            outputs = ("devel:demo/artifact.txt",)
        return subprocess.run(
            [
                str(CACHE),
                action,
                "local:build:demo",
                str(self.repository),
                *outputs,
            ],
            env=self.environment,
            text=True,
            capture_output=True,
            check=False,
        )

    def test_cache_requires_outputs_and_matching_sources(self) -> None:
        self.assertNotEqual(self.cache("check").returncode, 0)
        self.assertEqual(self.cache("record").returncode, 0)
        self.assertEqual(self.cache("check").returncode, 0)

        (self.repository / "source.txt").write_text("two\n", encoding="utf-8")
        self.assertNotEqual(self.cache("check").returncode, 0)

        self.assertEqual(self.cache("record").returncode, 0)
        (self.devel / "demo" / "artifact.txt").unlink()
        self.assertNotEqual(self.cache("check").returncode, 0)

    def test_ignored_build_outputs_do_not_invalidate_cache(self) -> None:
        self.assertEqual(self.cache("record").returncode, 0)
        generated = self.repository / "generated"
        generated.mkdir()
        (generated / "output.txt").write_text("changed\n", encoding="utf-8")

        self.assertEqual(self.cache("check").returncode, 0)

    def test_symlinked_directories_are_fingerprinted_as_links(self) -> None:
        target = self.root / "generated-docs"
        target.mkdir()
        (self.repository / "docs-link").symlink_to(target, target_is_directory=True)

        self.assertEqual(self.cache("record").returncode, 0)
        self.assertEqual(self.cache("check").returncode, 0)

        (self.repository / "docs-link").unlink()
        (self.repository / "docs-link").symlink_to(
            self.root / "other-docs", target_is_directory=True
        )
        self.assertNotEqual(self.cache("check").returncode, 0)

    def test_dependency_repository_changes_invalidate_consumer(self) -> None:
        dependency = self.root / "dependency"
        dependency.mkdir()
        subprocess.run(["git", "init", "-q", str(dependency)], check=True)
        (dependency / "schema.txt").write_text("one\n", encoding="utf-8")
        subprocess.run(["git", "-C", str(dependency), "add", "schema.txt"], check=True)
        self.environment["COGNIPILOT_TASK_CACHE_INPUT_REPOSITORIES"] = str(dependency)

        self.assertEqual(self.cache("record").returncode, 0)
        self.assertEqual(self.cache("check").returncode, 0)

        (dependency / "schema.txt").write_text("two\n", encoding="utf-8")
        self.assertNotEqual(self.cache("check").returncode, 0)

    def test_output_content_change_invalidates_cache(self) -> None:
        artifact = self.devel / "demo" / "artifact.txt"

        self.assertEqual(self.cache("record").returncode, 0)
        artifact.write_text("tampered\n", encoding="utf-8")

        self.assertNotEqual(self.cache("check").returncode, 0)

    def test_same_size_output_change_with_restored_mtime_invalidates_cache(
        self,
    ) -> None:
        artifact = self.devel / "demo" / "artifact.txt"
        original = artifact.stat()

        self.assertEqual(self.cache("record").returncode, 0)
        artifact.write_text("changed!\n", encoding="utf-8")
        os.utime(
            artifact,
            ns=(original.st_atime_ns, original.st_mtime_ns),
        )

        self.assertNotEqual(self.cache("check").returncode, 0)

    def test_stamp_records_metadata_and_content_proofs(self) -> None:
        self.assertEqual(self.cache("record").returncode, 0)

        stamp = self.build / "task-cache" / "local_build_demo.sha256"
        records = stamp.read_text(encoding="utf-8").splitlines()

        self.assertEqual(records[0], "workspace-task-cache-v4")
        self.assertEqual(len(records), 5)
        for proof in records[1:]:
            self.assertEqual(len(proof), 64)

    def test_output_mode_change_invalidates_cache(self) -> None:
        artifact = self.devel / "demo" / "artifact.txt"

        artifact.chmod(0o755)
        self.assertEqual(self.cache("record").returncode, 0)
        artifact.chmod(0o644)

        self.assertNotEqual(self.cache("check").returncode, 0)

    def test_output_tree_change_invalidates_cache(self) -> None:
        tree = self.devel / "demo" / "generated"
        tree.mkdir()
        generated = tree / "binding.rs"
        generated.write_text("one\n", encoding="utf-8")

        self.assertEqual(
            self.cache("record", "devel:demo/generated").returncode, 0
        )
        generated.write_text("two\n", encoding="utf-8")

        self.assertNotEqual(
            self.cache("check", "devel:demo/generated").returncode, 0
        )

    def test_output_symlink_target_change_invalidates_cache(self) -> None:
        first = self.root / "first-artifact"
        second = self.root / "second-artifact"
        first.write_text("same\n", encoding="utf-8")
        second.write_text("same\n", encoding="utf-8")
        artifact = self.devel / "demo" / "artifact-link"
        artifact.symlink_to(first)

        output = "devel:demo/artifact-link"
        self.assertEqual(self.cache("record", output).returncode, 0)
        artifact.unlink()
        artifact.symlink_to(second)

        self.assertNotEqual(self.cache("check", output).returncode, 0)

    def test_concurrent_cached_tasks_build_coordinate_once(self) -> None:
        artifact = self.devel / "demo" / "artifact.txt"
        artifact.unlink()
        count = self.root / "build-count"
        command = (
            'sleep 0.2; '
            f'count={str(count)!r}; '
            'value=0; [ ! -f "$count" ] || value="$(cat "$count")"; '
            'printf "%s\\n" "$((value + 1))" >"$count"; '
            f'printf "built\\n" >{str(artifact)!r}'
        )
        arguments = [
            str(CACHED_TASK),
            "local:build:demo",
            str(self.repository),
            "devel:demo/artifact.txt",
            "--",
            "bash",
            "-euo",
            "pipefail",
            "-c",
            command,
        ]

        first = subprocess.Popen(arguments, env=self.environment)
        second = subprocess.Popen(arguments, env=self.environment)

        self.assertEqual(first.wait(), 0)
        self.assertEqual(second.wait(), 0)
        self.assertEqual(count.read_text(encoding="utf-8"), "1\n")
        self.assertEqual(self.cache("check").returncode, 0)

    def test_failed_cached_task_does_not_publish_stamp(self) -> None:
        artifact = self.devel / "demo" / "artifact.txt"
        artifact.unlink()

        result = subprocess.run(
            [
                str(CACHED_TASK),
                "local:build:demo",
                str(self.repository),
                "devel:demo/artifact.txt",
                "--",
                "bash",
                "-c",
                "exit 17",
            ],
            env=self.environment,
            check=False,
        )

        self.assertEqual(result.returncode, 17)
        self.assertNotEqual(self.cache("check").returncode, 0)


if __name__ == "__main__":
    unittest.main()
