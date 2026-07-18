# Project environments

The root is a normal Devenv project. It intentionally has no workspace schema
or client: Devenv evaluates the environment, schedules tasks, and supervises
processes directly.

Each editable repository lives at `src/<repository>` and retains its native
build authority. `cognipilot.repos` is the sole top-level source manifest and
pins exact commits that `vcs2l` checks out with ordinary Git. The root has no
West manifest or West workspace.

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
The exact top-level manifest defines the coordinated integration snapshot,
while each vehicle's `west.yml` defines its independently reproducible firmware
snapshot. Those are separate integration contexts rather than competing source
managers.

Cross-repository development does not require a registry release. Devenv task
edges first generate local Synapse and Rumoca artifacts, then pass their exact
editable paths to downstream Cargo, npm, Modelica, CUBS2, and RDD2 commands.
Vehicle task edges initialize and reuse the corresponding project-owned West
workspace; only the explicit `cubs2:workspace:update` or
`rdd2:workspace:update` task advances that vehicle's manifest revisions. The
native build tools keep their normal incremental directories.

Profiles are work-area composition, not a second package graph. Reusable Rust,
Web, Zephyr, diagnostics, and task-set modules remain internal; developers
select profiles such as `cubs2`, `modelica`, or `electrode`. Each profile
imports only the tasks supported by its tools, so the base shell exposes source
and workspace maintenance rather than every project command. The `cubs2`
profile contains the `electrode-ground-station` and `electrode-ppm-bridge`
deployment processes plus the opt-in `electrode-fake-vehicle` and
`synapse-qualisys-bridge` processes. The base shell remains small.

The root tool modules provide common cross-repository tools. When a project
owns a more specific environment, native repository work uses that environment.
For example, Rumoca's `nix develop` consumes its pinned nightly Rust toolchain,
WASM target, Node release, and native libraries; root Modelica tasks invoke the
project flake for generated immutable artifacts.

Use a project flake when the repository owns a reproducible immutable package
or a complex project-specific command. Use a root Devenv task for the editable
developer entry point. If an upstream repository cannot carry its own setup,
the corresponding ordinary module belongs under `devenv/`; it must not create
a new discovery or orchestration protocol.
