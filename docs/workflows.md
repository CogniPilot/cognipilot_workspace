# Development workflows

This page starts with a developer goal and gives the profile and exact Devenv
entry points for it. Put the profile you use most often in the ignored
`devenv.local.yaml`; commands below retain `--profile` so they also work from a
fresh checkout.

Run `devenv --profile <name> tasks list` to see the selected task DAG. Do not
shorten a task to its namespace: `devenv tasks run cubs2` runs every `cubs2:*`
task, while `devenv tasks run cubs2:simulation:sil:test` runs that test and its
dependencies.

## CUBS2

Use the `cubs2` profile for firmware, simulation, and deployment. Run the real
pure Modelica, 64-bit firmware SIL, or hardware-binary BIL workflow:

```sh
devenv --profile cubs2 tasks run cubs2:simulation:modelica:test
devenv --profile cubs2 tasks run cubs2:simulation:sil:test
devenv --profile cubs2 tasks run cubs2:simulation:bil:test
devenv --profile cubs2 tasks run cubs2:simulation:compare
```

Build aircraft firmware without touching hardware, or configure it with the
project-owned menuconfig application:

```sh
devenv --profile cubs2 tasks run cubs2:firmware:build
devenv --profile cubs2 tasks run cubs2:firmware:configure
```

Flash only after inspecting the image and explicitly selecting the connected
hardware:

```sh
CUBS2_FLASH_RUNNER=pyocd devenv --profile cubs2 tasks run \
  cubs2:firmware:flash --input confirm=true
```

Omitting `--input confirm=true` fails before the flash command executes.

Start the deployed operator stack with the PPM encoder attached:

```sh
PPM_SERIAL_DEVICE=/dev/ttyACM0 devenv --profile cubs2 up
```

Exercise the ground-station UI and PPM arbitration without firmware or serial
hardware:

```sh
PPM_NO_SERIAL=true devenv --profile cubs2 up \
  electrode-ground-station electrode-ppm-bridge electrode-fake-vehicle
```

These are deliberately different simulations: `simulation:modelica:test` has
no Zephyr executable, `simulation:sil:test` executes host Zephyr firmware,
`simulation:bil:test` executes the ARM firmware through FastDyn/QEMU, and
`electrode-fake-vehicle` is only an operator-UI stand-in.
The complete workflow, artifacts, 32-bit target, West updates, Qualisys setup,
and process arguments are documented in [CUBS2 developer workflow](cubs2.md).
The comparison leaf runs all three fidelity levels, overlays their canonical
logs, and enforces the cross-fidelity error budget.

## RDD2

Use the same task shape as CUBS2:

```sh
devenv --profile rdd2 tasks run rdd2:simulation:modelica:test
devenv --profile rdd2 tasks run rdd2:simulation:sil:test
devenv --profile rdd2 tasks run rdd2:simulation:bil:test
devenv --profile rdd2 tasks run rdd2:simulation:compare
devenv --profile rdd2 tasks run rdd2:firmware:build
devenv --profile rdd2 tasks run rdd2:firmware:configure
devenv --profile rdd2 tasks run rdd2:firmware:flash --input confirm=true
```

CUBS2 and RDD2 never share a West workspace. Their first build initializes the
workspace selected by the project flake. Update its manifest revisions only
when requested:

```sh
devenv --profile rdd2 tasks run rdd2:workspace:update
```

The RDD2 Modelica, native-firmware, and rehosted-binary regressions use the same
Rumoca-generated plant. The precise SIL/BIL and Rumoca/FastDyn roles are documented in
[Vehicle firmware and simulation](vehicle-development.md) and
[RDD2 developer workflow](rdd2.md).

## Electrode ground station

Use the focused profile for build and test work:

```sh
devenv --profile electrode shell
devenv --profile electrode tasks run electrode-web:build
devenv --profile electrode tasks run electrode-web:test
```

Use the CUBS2 profile for the deployed ground-station stack:

```sh
devenv --profile cubs2 up
```

The locked npm install is skipped when the installed tree matches the current
lock file. Local generated Synapse and Rumoca packages are rebound before the
web build.

## Rumoca and Modelica

Use the project-owned Rumoca flake for native compiler, WASM, Python, playground,
or VS Code extension changes:

```sh
cd src/rumoca
nix develop
cargo xtask verify quick
cargo xtask playground test
cargo xtask vscode edit
```

That shell consumes Rumoca's pinned nightly toolchain instead of the stable
cross-project Rust toolchain. Use the root profile for integration tasks:

```sh
devenv --profile modelica tasks run rumoca:compiler
devenv --profile modelica tasks run rumoca:test
devenv --profile modelica tasks run modelica-models:test
devenv --profile modelica tasks run modelica-models:cubs2:export-controller
devenv --profile modelica tasks run modelica-models:cubs2:export-plant
devenv --profile modelica tasks run modelica-models:rdd2:export-controller
devenv --profile modelica tasks run modelica-models:rdd2:export-estimator
devenv --profile modelica tasks run modelica-models:rdd2:export-plant
```

## Synapse schemas

```sh
devenv --profile synapse tasks run synapse-fbs:build
devenv --profile synapse tasks run synapse-fbs:test
```

Consumer profiles import these tasks automatically when their dependency graph
uses generated local packages.

## ZROS, CSyn, and Cerebri modules

```sh
devenv --profile zros tasks run zros:test
devenv --profile zros tasks run cerebri-modules:test
devenv --profile zros tasks run csyn:qualification
```

These tests use the isolated CUBS2 West workspace as their Zephyr SDK while
passing the editable module paths explicitly.

## Qualisys and PPM bridges

```sh
devenv --profile qualisys tasks run qualisys-sdk:test
devenv --profile qualisys tasks run qualisys-bridge:e2e
devenv --profile qualisys up synapse-qualisys-bridge

devenv --profile ppm tasks run ppm:test
devenv --profile ppm up synapse-ppm-bridge
```

For a detached bridge, append `-d`, then use `processes logs`, `processes list`,
and `down` with the same profile.

## ROS 2 bridge

```sh
devenv --profile ros2 tasks run ros2:test
```

The task delegates the colcon and ROS environment to the bridge's project-owned
flake application.

## FastDyn

```sh
devenv --profile fastdyn tasks run fastdyn:test
```

The project-owned virtual environment is recreated when `requirements.txt` or
`setup.sh` changes and otherwise reused.

## Release qualification

Run the bounded integration tests without publication:

```sh
devenv --profile release tasks run release:qualify
```

Include package dry runs and hardware firmware builds:

```sh
devenv --profile release tasks run release:all
```

Project publication remains in each repository's tag workflow. The root release
profile never publishes packages.

## Workspace diagnosis

```sh
devenv tasks run sources:status
devenv test
devenv --profile cubs2 info
devenv changelogs
```

If a selected task is not listed, select the profile that owns that work area.
If a process command says no manager is running, either use foreground `up` or
start detached processes with `up -d` before querying `processes list`.
