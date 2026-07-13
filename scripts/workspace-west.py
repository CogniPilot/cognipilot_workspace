#!/usr/bin/env python3
"""Build one deterministic west manifest and one local override view."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from typing import Any

import yaml


def colors_enabled(stream: Any) -> bool:
    if "NO_COLOR" in os.environ:
        return False
    forced = os.environ.get("CLICOLOR_FORCE", "0") != "0"
    return forced or (stream.isatty() and os.environ.get("TERM", "dumb") != "dumb")


if colors_enabled(sys.stdout):
    RESET = "\033[0m"
    BOLD = "\033[1m"
    DIM = "\033[2m"
    RED = "\033[31m"
    GREEN = "\033[32m"
    YELLOW = "\033[33m"
    BLUE = "\033[34m"
    CYAN = "\033[36m"
else:
    RESET = BOLD = DIM = RED = GREEN = YELLOW = BLUE = CYAN = ""


class WorkspaceError(RuntimeError):
    pass


def workspace_root() -> Path:
    configured = os.environ.get("COGNIPILOT_WORKSPACE_ROOT")
    if configured:
        return Path(configured).expanduser().resolve()
    current = Path.cwd().resolve()
    for candidate in (current, *current.parents):
        if (
            (candidate / "devenv.nix").is_file()
            and (candidate / "nix/components.nix").is_file()
        ):
            return candidate
    raise WorkspaceError(
        "could not locate cognipilot_workspace; enter its devenv shell"
    )


def cache_root(root: Path) -> Path:
    configured = os.environ.get("COGNIPILOT_WEST_CACHE")
    if configured:
        return Path(configured).expanduser().resolve()
    xdg_cache = Path(os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache"))
    workspace_id = hashlib.sha256(str(root).encode()).hexdigest()[:12]
    return xdg_cache / "cognipilot_workspace" / workspace_id / "west"


def active_profiles(root: Path) -> set[str]:
    configured = os.environ.get("COGNIPILOT_WORKSPACE_PROFILES")
    if configured:
        return {profile for profile in configured.split(",") if profile}
    profile_file = root / ".devenv/state/workspace-profile"
    if profile_file.is_file():
        profiles = {
            line.strip()
            for line in profile_file.read_text(encoding="utf-8").splitlines()
            if line.strip()
        }
        if profiles:
            return profiles
    return {"default"}


def source_manifests(root: Path) -> list[Path]:
    candidates = [
        root / "src/cerebri_cubs2/west.yml",
        root / "src/cerebri_rdd2/west.yml",
    ]
    manifests = [path for path in candidates if path.is_file()]
    if not manifests:
        raise WorkspaceError(
            "no firmware manifest is available; sync cerebri_cubs2 or cerebri_rdd2"
        )
    return manifests


def load_yaml(path: Path) -> dict[str, Any]:
    if not path.is_file():
        raise WorkspaceError(f"missing west manifest: {path}")
    with path.open(encoding="utf-8") as stream:
        loaded = yaml.safe_load(stream)
    if not isinstance(loaded, dict) or not isinstance(loaded.get("manifest"), dict):
        raise WorkspaceError(f"invalid west manifest: {path}")
    return loaded["manifest"]


def canonical_project(
    raw: dict[str, Any], remotes: dict[str, str], manifest_path: Path
) -> dict[str, Any]:
    project = dict(raw)
    try:
        name = str(project["name"])
    except KeyError as error:
        raise WorkspaceError(f"unnamed project in {manifest_path}") from error

    if "url" not in project:
        remote_name = project.get("remote")
        if not remote_name or remote_name not in remotes:
            raise WorkspaceError(
                f"project {name!r} in {manifest_path} has no resolvable URL"
            )
        repo_path = str(project.get("repo-path", name))
        project["url"] = f"{remotes[str(remote_name)].rstrip('/')}/{repo_path}"

    project.pop("remote", None)
    project.pop("repo-path", None)
    project.setdefault("path", name)
    project.setdefault("revision", "master")
    return project


def merged_manifest(root: Path) -> tuple[dict[str, Any], dict[str, list[str]]]:
    projects: dict[str, dict[str, Any]] = {}
    origins: dict[str, list[str]] = {}
    order: list[str] = []

    for path in source_manifests(root):
        manifest = load_yaml(path)
        remotes = {
            str(remote["name"]): str(remote["url-base"])
            for remote in manifest.get("remotes", [])
        }
        for raw in manifest.get("projects", []):
            project = canonical_project(raw, remotes, path)
            name = str(project["name"])
            origin = str(path.relative_to(root))
            if name in projects and projects[name] != project:
                old = yaml.safe_dump(projects[name], sort_keys=True).strip()
                new = yaml.safe_dump(project, sort_keys=True).strip()
                raise WorkspaceError(
                    f"west project {name!r} conflicts between manifests\n"
                    f"first definition:\n{old}\nsecond definition ({origin}):\n{new}\n"
                    "Use isolated workspaces or align the pins; no revision was selected."
                )
            if name not in projects:
                projects[name] = project
                order.append(name)
            origins.setdefault(name, []).append(origin)

    result = {
        "manifest": {
            "projects": [projects[name] for name in order],
            "self": {"path": "manifest"},
        }
    }
    return result, origins


def manifest_text(root: Path) -> tuple[str, dict[str, list[str]]]:
    manifest, origins = merged_manifest(root)
    text = (
        "# Generated by cognipilot_workspace from the application west manifests.\n"
        "# Do not edit this cached copy; edit the owning application manifest.\n"
        + yaml.safe_dump(manifest, sort_keys=False)
    )
    return text, origins


def manifest_digest(text: str) -> str:
    return hashlib.sha256(text.encode()).hexdigest()


def run(command: list[str], *, cwd: Path) -> None:
    try:
        subprocess.run(command, cwd=cwd, check=True)
    except FileNotFoundError as error:
        raise WorkspaceError(
            f"required command {command[0]!r} is unavailable; enter `devenv shell`"
        ) from error
    except subprocess.CalledProcessError as error:
        raise WorkspaceError(
            f"command failed with status {error.returncode}: {' '.join(command)}"
        ) from error


def write_manifest_repo(shared: Path, text: str) -> None:
    manifest_dir = shared / "manifest"
    manifest_dir.mkdir(parents=True, exist_ok=True)
    manifest_file = manifest_dir / "west.yml"
    if not manifest_file.exists() or manifest_file.read_text(encoding="utf-8") != text:
        manifest_file.write_text(text, encoding="utf-8")

    if not (manifest_dir / ".git").exists():
        run(["git", "init", "-q", "-b", "workspace"], cwd=manifest_dir)
    run(["git", "add", "west.yml"], cwd=manifest_dir)
    changed = subprocess.run(
        ["git", "diff", "--cached", "--quiet"], cwd=manifest_dir, check=False
    ).returncode
    if changed:
        run(
            [
                "git",
                "-c",
                "user.name=cognipilot_workspace",
                "-c",
                "user.email=workspace@example.invalid",
                "commit",
                "-q",
                "-m",
                "Update merged west manifest",
            ],
            cwd=manifest_dir,
        )


def write_west_config(workspace: Path) -> None:
    """Anchor west to this workspace without searching parent directories."""
    west_config = workspace / ".west/config"
    west_config.parent.mkdir(parents=True, exist_ok=True)
    west_config.write_text(
        "[manifest]\npath = manifest\nfile = west.yml\n\n[zephyr]\nbase = zephyr\n",
        encoding="utf-8",
    )


def project_map(manifest: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {
        str(project["name"]): project
        for project in manifest["manifest"].get("projects", [])
    }


def checkout_ready(shared: Path, manifest: dict[str, Any], digest: str) -> bool:
    marker = shared / ".cognipilot_workspace.json"
    if not marker.is_file() or not (shared / ".west/config").is_file():
        return False
    try:
        state = json.loads(marker.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return False
    if state.get("manifest_sha256") != digest:
        return False

    for project in manifest["manifest"].get("projects", []):
        checkout = shared / str(project["path"])
        if not checkout.is_dir():
            return False
        revision = str(project["revision"])
        completed = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=checkout,
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
        if completed.returncode != 0:
            return False
        # All current app pins are full Git object IDs. For a future immutable
        # tag, west's manifest-rev remains authoritative and sync will refresh.
        if len(revision) == 40 and completed.stdout.strip() != revision:
            return False
    return True


def create_local_view(root: Path, cache: Path, manifest: dict[str, Any]) -> None:
    shared = cache / "shared"
    local = cache / "local"
    temporary = cache / ".local.tmp"
    shutil.rmtree(temporary, ignore_errors=True)
    temporary.mkdir(parents=True)

    overrides = {
        "cerebri_modules": root / "src/cerebri_modules",
        "csyn": root / "src/csyn",
        "modelica_models": root / "src/modelica_models",
    }
    if "zros" in active_profiles(root):
        overrides["zros"] = root / "src/zros"
    for project in manifest["manifest"].get("projects", []):
        name = str(project["name"])
        relative = Path(str(project["path"]))
        source = overrides.get(name, shared / relative)
        if not source.exists():
            raise WorkspaceError(f"west project source is missing: {source}")
        destination = temporary / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.symlink_to(source.resolve(), target_is_directory=True)

    (temporary / "manifest").symlink_to(
        (shared / "manifest").resolve(), target_is_directory=True
    )
    write_west_config(temporary)

    shutil.rmtree(local, ignore_errors=True)
    temporary.rename(local)


def validate(root: Path) -> tuple[dict[str, Any], str, dict[str, list[str]]]:
    text, origins = manifest_text(root)
    manifest = yaml.safe_load(text)
    shared = sorted(name for name, sources in origins.items() if len(sources) > 1)
    revisions = [
        str(project["revision"])
        for project in manifest["manifest"].get("projects", [])
    ]
    floating = [revision for revision in revisions if len(revision) != 40]
    if floating:
        raise WorkspaceError(
            "shared west reproducibility requires full commit pins; found: "
            + ", ".join(sorted(set(floating)))
        )
    print(
        f"{GREEN}✓{RESET} west union valid: {len(origins)} projects, "
        f"{len(shared)} shared definitions, all revisions pinned"
    )
    return manifest, text, origins


def sync(root: Path) -> None:
    manifest, text, _ = validate(root)
    cache = cache_root(root)
    shared = cache / "shared"
    shared.mkdir(parents=True, exist_ok=True)
    write_manifest_repo(shared, text)
    # `west init` searches parent directories and aborts if the user happens to
    # have an unrelated workspace (for example ~/.west).  Writing the minimal
    # local configuration directly makes this cache an explicit west root.
    write_west_config(shared)
    # Every merged revision is a full commit ID. Narrow updates ask GitHub only
    # for those pinned objects instead of fetching every branch and tag, which
    # keeps a first-time firmware setup substantially smaller.
    run(["west", "update", "--narrow"], cwd=shared)
    marker = {
        "manifest_sha256": manifest_digest(text),
        "sources": [str(path.relative_to(root)) for path in source_manifests(root)],
    }
    (shared / ".cognipilot_workspace.json").write_text(
        json.dumps(marker, indent=2) + "\n", encoding="utf-8"
    )
    create_local_view(root, cache, manifest)
    print(f"{GREEN}✓{RESET} shared west workspace ready: {CYAN}{shared}{RESET}")


def ensure(root: Path) -> None:
    manifest, text, _ = validate(root)
    cache = cache_root(root)
    shared = cache / "shared"
    digest = manifest_digest(text)
    if not checkout_ready(shared, manifest, digest):
        sync(root)
        return
    create_local_view(root, cache, manifest)
    print(
        f"{GREEN}✓{RESET} shared west workspace already matches "
        f"{DIM}{digest[:12]}{RESET}"
    )


def status(root: Path) -> None:
    text, _ = manifest_text(root)
    manifest = yaml.safe_load(text)
    cache = cache_root(root)
    shared = cache / "shared"
    local = cache / "local"
    digest = manifest_digest(text)
    ready = checkout_ready(shared, manifest, digest)
    ready_color = GREEN if ready else RED
    ready_label = "yes" if ready else "no"
    print(f"{BOLD}{BLUE}west cache:{RESET}   {CYAN}{cache}{RESET}")
    print(f"{BOLD}{BLUE}release view:{RESET} {CYAN}{shared}{RESET}")
    print(f"{BOLD}{BLUE}local view:{RESET}   {CYAN}{local}{RESET}")
    print(f"{BOLD}{BLUE}manifest:{RESET}     {DIM}{digest}{RESET}")
    print(f"{BOLD}{BLUE}ready:{RESET}        {ready_color}{ready_label}{RESET}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("validate")
    subparsers.add_parser("ensure")
    subparsers.add_parser("sync")
    subparsers.add_parser("status")
    path_parser = subparsers.add_parser("path")
    path_parser.add_argument("--mode", choices=("local", "release"), required=True)
    args = parser.parse_args()

    root = workspace_root()
    if args.command == "validate":
        validate(root)
    elif args.command == "ensure":
        ensure(root)
    elif args.command == "sync":
        sync(root)
    elif args.command == "status":
        status(root)
    elif args.command == "path":
        view = "local" if args.mode == "local" else "shared"
        print(cache_root(root) / view)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        print(
            "workspace-west: interrupted; rerun the command to resume the cache update",
            file=sys.stderr,
        )
        raise SystemExit(130)
    except WorkspaceError as error:
        prefix = "\033[31merror:\033[0m" if colors_enabled(sys.stderr) else "error:"
        print(f"workspace-west {prefix} {error}", file=sys.stderr)
        raise SystemExit(1)
