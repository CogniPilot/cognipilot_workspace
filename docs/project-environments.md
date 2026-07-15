# Project environments

The root is a normal Devenv project. It intentionally has no workspace schema
or client: Devenv evaluates the environment, schedules tasks, and supervises
processes directly.

Each editable repository lives at `src/<repository>` and retains its native
build authority. The shared West manifest pins and updates those checkouts.
Its local Zephyr submanifest adds the common SDK dependencies once, and a
Devenv task exposes the editable ZROS, CSyn, Cerebri modules, and Modelica
checkouts at the conventional vehicle-workspace paths with symlinks rather than
additional clones.
Project `flake.nix` files remain useful for project-owned toolchains, immutable
packages, and release applications; repositories without a flake use packages
from the selected root Devenv profile.

Cross-repository development does not require a registry release. Devenv task
edges first generate local Synapse and Rumoca artifacts, then pass their exact
editable paths to downstream Cargo, npm, Modelica, CUBS2, and RDD2 commands.
The native build tools keep their normal incremental directories.

Profiles are product/persona composition, not a second package graph. They use
single-parent chains plus reusable Nix modules so Devenv does not repeatedly
expand diamond-shaped profile inheritance. The `cubs2` profile contains the
vehicle's simulation, ground-station, and Qualisys processes. The base shell
remains small.

Use a project flake when the repository owns a reproducible immutable package
or a complex project-specific command. Use a root Devenv task for the editable
developer entry point. If an upstream repository cannot carry its own setup,
the corresponding ordinary module belongs under `devenv/`; it must not create
a new discovery or orchestration protocol.
