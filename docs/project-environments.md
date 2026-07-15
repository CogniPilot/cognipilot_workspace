# Project environments

The root is a normal Devenv project. It intentionally has no workspace schema
or client: Devenv evaluates the environment, schedules tasks, and supervises
processes directly.

Each editable repository lives at `src/<repository>` and retains its native
build authority. Generated Devenv tasks clone or fetch those repositories with
ordinary Git; the root has no West manifest or West workspace.
Project `flake.nix` files remain useful for project-owned toolchains, immutable
packages, and release applications; repositories without a flake use packages
from the selected root Devenv profile.

CUBS2 and RDD2 each own their complete West manifest and use independent
workspaces below `.devenv/state/west/`. This permits different Zephyr and
module revisions without path conflicts and keeps `.west/`, `zephyr/`,
`modules/`, and `models/` out of the root. Their project flake applications
initialize and update these workspaces. Root tasks pass editable ZROS, CSyn,
Cerebri modules, Modelica, and generated Synapse paths explicitly at the native
CMake/process boundaries, so the isolated West checkouts only need the
vehicle-specific SDK dependencies instead of duplicating editable repositories.

Cross-repository development does not require a registry release. Devenv task
edges first generate local Synapse and Rumoca artifacts, then pass their exact
editable paths to downstream Cargo, npm, Modelica, CUBS2, and RDD2 commands.
Vehicle task edges also update the corresponding project-owned West workspace.
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
