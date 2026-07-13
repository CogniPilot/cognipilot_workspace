#!/usr/bin/env python3
"""Archive every frozen component commit into a standalone Git bundle."""

from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile


REVISION = re.compile(r"^[0-9a-fA-F]{40}$")


def run(*command: str, cwd: Path) -> None:
    subprocess.run(command, cwd=cwd, check=True)


def archive(name: str, component: dict[str, str], output: Path, work: Path) -> str:
    revision = component["revision"]
    github = component["github"]
    if not REVISION.fullmatch(revision):
        raise RuntimeError(f"{name} has invalid revision {revision!r}")
    if not re.fullmatch(r"[A-Za-z0-9_.-]+", name):
        raise RuntimeError(f"unsafe component name {name!r}")

    repository = work / name
    repository.mkdir()
    run("git", "init", "-q", cwd=repository)
    run(
        "git",
        "remote",
        "add",
        "origin",
        f"https://github.com/{github}.git",
        cwd=repository,
    )
    run("git", "fetch", "--depth=1", "--no-tags", "origin", revision, cwd=repository)
    resolved = subprocess.check_output(
        ["git", "rev-parse", "FETCH_HEAD"], cwd=repository, text=True
    ).strip()
    if resolved != revision:
        raise RuntimeError(f"{name} resolved to {resolved}, expected {revision}")
    run(
        "git",
        "update-ref",
        "refs/heads/workspace-snapshot",
        revision,
        cwd=repository,
    )

    temporary = output / f".{name}.bundle.tmp"
    bundle = output / f"{name}.bundle"
    run(
        "git",
        "bundle",
        "create",
        str(temporary),
        "refs/heads/workspace-snapshot",
        cwd=repository,
    )
    run("git", "bundle", "verify", str(temporary), cwd=repository)
    temporary.replace(bundle)
    return name


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("lock", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--jobs", type=int, default=4)
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error("--jobs must be positive")

    lock = json.loads(args.lock.read_text(encoding="utf-8"))
    components = lock.get("components")
    if lock.get("schema") != 1 or not isinstance(components, dict) or not components:
        raise SystemExit("snapshot must use schema 1 and contain components")

    args.output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="workspace-sources-") as temporary:
        work = Path(temporary)
        with ThreadPoolExecutor(max_workers=args.jobs) as executor:
            futures = {
                executor.submit(archive, name, data, args.output, work): name
                for name, data in components.items()
            }
            for future in as_completed(futures):
                name = future.result()
                print(f"archived {name}")

    shutil.copy2(args.lock, args.output / "workspace.lock.json")


if __name__ == "__main__":
    main()
