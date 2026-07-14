from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
WS = ROOT / "scripts" / "ws"


def component(*, optional: bool = False) -> dict[str, object]:
    return {
        "path": "src/demo",
        "dependencies": [],
        "buildDependencies": [],
        "devenvProfile": None,
        "optional": optional,
        "repo": {
            "github": "CogniPilot/demo",
            "branch": "main",
            "private": False,
            "allowDeployed": False,
        },
        "tasks": {
            "local": {"build": False, "test": False},
            "release": {"build": False, "test": False},
        },
    }


class WorkspaceCliTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.state = self.root / "state"
        self.state.mkdir()
        self.manifest = self.root / "manifest.json"
        self.write_manifest(component())
        self.launch_manifest = self.root / "launch-manifest.json"
        self.launch_manifest.write_text(
            json.dumps(
                {
                    "schema": 1,
                    "profiles": {
                        "ground-station": {
                            "description": "Ground station.",
                            "processes": ["ground-station"],
                        },
                        "simulation": {
                            "description": "Simulator.",
                            "processes": ["simulation"],
                        },
                        "simulation-stack": {
                            "description": "Complete simulation.",
                            "processes": ["simulation", "ground-station"],
                        },
                    },
                }
            ),
            encoding="utf-8",
        )
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.devenv_log = self.root / "devenv.log"
        fake_devenv = self.bin / "devenv"
        fake_devenv.write_text(
            '#!/bin/sh\nprintf \'%s\\n\' "$@" >"$DEVENV_LOG"\n',
            encoding="utf-8",
        )
        fake_devenv.chmod(0o755)
        self.environment = os.environ.copy()
        self.environment.update(
            {
                "COGNIPILOT_LAUNCH_MANIFEST": str(self.launch_manifest),
                "COGNIPILOT_WORKSPACE_ROOT": str(self.root),
                "COGNIPILOT_REPO_MANIFEST": str(self.manifest),
                "DEVENV_STATE": str(self.state),
                "DEVENV_LOG": str(self.devenv_log),
                "NO_COLOR": "1",
                "PATH": f"{self.bin}{os.pathsep}{self.environment['PATH']}",
            }
        )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def write_manifest(self, demo: dict[str, object]) -> None:
        self.manifest.write_text(
            json.dumps(
                {
                    "schema": 1,
                    "profiles": {"default": ["demo"]},
                    "profileDefinitions": {
                        "default": {"includes": [], "components": ["demo"]}
                    },
                    "components": {"demo": demo},
                }
            ),
            encoding="utf-8",
        )

    def run_ws(
        self, *arguments: str, environment: dict[str, str] | None = None
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(WS), *arguments],
            env=environment or self.environment,
            text=True,
            capture_output=True,
            check=False,
        )

    def test_freeze_without_manifest_preserves_existing_lock(self) -> None:
        lock = self.root / "workspace.lock.json"
        lock.write_text("keep me\n", encoding="utf-8")
        environment = self.environment.copy()
        environment.pop("COGNIPILOT_REPO_MANIFEST")

        result = self.run_ws("freeze", environment=environment)

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("manifest is unavailable", result.stderr)
        self.assertEqual(lock.read_text(encoding="utf-8"), "keep me\n")

    def test_empty_freeze_preserves_existing_lock(self) -> None:
        self.write_manifest(component(optional=True))
        lock = self.root / "workspace.lock.json"
        lock.write_text("keep me\n", encoding="utf-8")

        result = self.run_ws("freeze")

        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(lock.read_text(encoding="utf-8"), "keep me\n")

    def test_freeze_writes_snapshot_object(self) -> None:
        repository = self.root / "src" / "demo"
        repository.mkdir(parents=True)
        subprocess.run(["git", "init", "-q", str(repository)], check=True)
        subprocess.run(
            ["git", "-C", str(repository), "config", "user.name", "Workspace Test"],
            check=True,
        )
        subprocess.run(
            [
                "git",
                "-C",
                str(repository),
                "config",
                "user.email",
                "test@example.com",
            ],
            check=True,
        )
        (repository / "payload.txt").write_text("snapshot\n", encoding="utf-8")
        subprocess.run(["git", "-C", str(repository), "add", "payload.txt"], check=True)
        subprocess.run(
            ["git", "-C", str(repository), "commit", "-q", "-m", "snapshot"],
            check=True,
        )
        revision = subprocess.check_output(
            ["git", "-C", str(repository), "rev-parse", "HEAD"], text=True
        ).strip()

        result = self.run_ws("freeze", "--allow-unpushed")

        self.assertEqual(result.returncode, 0, result.stderr)
        snapshot = json.loads((self.root / "workspace.lock.json").read_text())
        self.assertEqual(snapshot["schema"], 1)
        self.assertEqual(snapshot["components"]["demo"]["revision"], revision)
        self.assertEqual(snapshot["components"]["demo"]["github"], "CogniPilot/demo")

    def test_corrupt_mode_is_rejected_and_can_be_repaired(self) -> None:
        mode = self.state / "workspace-mode"
        mode.write_text("surprise\n", encoding="utf-8")

        rejected = self.run_ws("mode")
        extra = self.run_ws("mode", "local", "extra")
        repaired = self.run_ws("mode", "local")

        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("invalid workspace mode", rejected.stderr)
        self.assertEqual(extra.returncode, 2)
        self.assertEqual(repaired.returncode, 0)
        self.assertEqual(mode.read_text(encoding="utf-8"), "local\n")

    def test_remotes_adds_origin_before_setting_ssh_push_url(self) -> None:
        repository = self.root / "src" / "demo"
        repository.mkdir(parents=True)
        subprocess.run(["git", "init", "-q", str(repository)], check=True)

        result = self.run_ws("remotes", "ssh")

        self.assertEqual(result.returncode, 0, result.stderr)
        fetch_url = subprocess.check_output(
            ["git", "-C", str(repository), "remote", "get-url", "origin"],
            text=True,
        ).strip()
        push_url = subprocess.check_output(
            ["git", "-C", str(repository), "remote", "get-url", "--push", "origin"],
            text=True,
        ).strip()
        self.assertEqual(fetch_url, "https://github.com/CogniPilot/demo.git")
        self.assertEqual(push_url, "git@github.com:CogniPilot/demo.git")

    def test_restore_materializes_missing_repository_from_bundle(self) -> None:
        source = self.root / "source"
        source.mkdir()
        subprocess.run(["git", "init", "-q", str(source)], check=True)
        subprocess.run(
            ["git", "-C", str(source), "config", "user.name", "Workspace Test"],
            check=True,
        )
        subprocess.run(
            ["git", "-C", str(source), "config", "user.email", "test@example.com"],
            check=True,
        )
        (source / "payload.txt").write_text("snapshot\n", encoding="utf-8")
        subprocess.run(["git", "-C", str(source), "add", "payload.txt"], check=True)
        subprocess.run(
            ["git", "-C", str(source), "commit", "-q", "-m", "snapshot"],
            check=True,
        )
        revision = subprocess.check_output(
            ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
        ).strip()
        subprocess.run(
            [
                "git",
                "-C",
                str(source),
                "update-ref",
                "refs/heads/workspace-snapshot",
                revision,
            ],
            check=True,
        )
        bundles = self.root / "bundles"
        bundles.mkdir()
        subprocess.run(
            [
                "git",
                "-C",
                str(source),
                "bundle",
                "create",
                str(bundles / "demo.bundle"),
                "refs/heads/workspace-snapshot",
            ],
            check=True,
        )
        lock = self.root / "workspace.lock.json"
        lock.write_text(
            json.dumps(
                {
                    "schema": 1,
                    "components": {
                        "demo": {
                            "github": "CogniPilot/demo",
                            "revision": revision,
                        }
                    },
                }
            ),
            encoding="utf-8",
        )

        result = self.run_ws("restore", "--bundle-dir", str(bundles), str(lock))

        restored = self.root / "src" / "demo"
        self.assertEqual(result.returncode, 0, result.stderr)
        restored_revision = subprocess.check_output(
            ["git", "-C", str(restored), "rev-parse", "HEAD"], text=True
        ).strip()
        self.assertEqual(restored_revision, revision)
        self.assertEqual(
            (restored / "payload.txt").read_text(encoding="utf-8"), "snapshot\n"
        )

    def test_build_and_freeze_reject_extra_positionals(self) -> None:
        build = self.run_ws("build", "demo", "extra")
        freeze = self.run_ws("freeze", "one.json", "two.json")

        self.assertEqual(build.returncode, 2)
        self.assertEqual(freeze.returncode, 2)

    def test_build_activates_only_artifact_dependency_profiles(self) -> None:
        app = component()
        app.update(
            {
                "path": "src/app",
                "dependencies": ["source", "artifact"],
                "buildDependencies": ["artifact"],
                "tasks": {
                    "local": {"build": True, "test": False},
                    "release": {"build": True, "test": False},
                },
            }
        )
        source = component()
        source.update({"path": "src/source", "devenvProfile": "source-tools"})
        artifact = component()
        artifact.update({"path": "src/artifact", "devenvProfile": "artifact-tools"})
        for entry in (app, source, artifact):
            entry["repo"]["allowDeployed"] = True
        for name in ("app", "source", "artifact"):
            (self.root / "src" / name).mkdir(parents=True)
        self.manifest.write_text(
            json.dumps(
                {
                    "schema": 1,
                    "profiles": {"default": ["app"]},
                    "profileDefinitions": {
                        "default": {"includes": [], "components": ["app"]}
                    },
                    "components": {
                        "app": app,
                        "source": source,
                        "artifact": artifact,
                    },
                }
            ),
            encoding="utf-8",
        )

        result = self.run_ws("build", "app")

        self.assertEqual(result.returncode, 0, result.stderr)
        arguments = self.devenv_log.read_text(encoding="utf-8").splitlines()
        self.assertIn("artifact-tools", arguments)
        self.assertNotIn("source-tools", arguments)
        self.assertEqual(arguments[-3:], ["tasks", "run", "local:build:app"])

    def test_launch_lists_central_profiles(self) -> None:
        result = self.run_ws("launch", "list")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("simulation-stack", result.stdout)
        self.assertIn("simulation, ground-station", result.stdout)

    def test_launch_composes_profiles_and_dispatches_process_commands(self) -> None:
        result = self.run_ws("launch", "simulation", "ground-station", "status")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            self.devenv_log.read_text(encoding="utf-8").splitlines(),
            [
                "--profile",
                "simulation",
                "--profile",
                "ground-station",
                "processes",
                "list",
            ],
        )

    def test_launch_rejects_process_outside_selected_profiles(self) -> None:
        result = self.run_ws("launch", "simulation", "logs", "ground-station")

        self.assertEqual(result.returncode, 2)
        self.assertIn("not provided", result.stderr)
        self.assertFalse(self.devenv_log.exists())

    def test_launch_completion_lists_profiles_actions_and_processes(self) -> None:
        profiles = self.run_ws("_complete", "launch", "")
        actions = self.run_ws("_complete", "launch", "simulation", "")
        processes = self.run_ws("_complete", "launch", "simulation-stack", "logs", "")

        self.assertEqual(profiles.returncode, 0, profiles.stderr)
        self.assertIn("simulation-stack", profiles.stdout.splitlines())
        self.assertIn("status", actions.stdout.splitlines())
        self.assertEqual(
            processes.stdout.splitlines(), ["simulation", "ground-station"]
        )


if __name__ == "__main__":
    unittest.main()
