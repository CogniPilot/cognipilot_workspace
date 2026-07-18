# CUBS2 developer workflow

A CUBS2 developer uses one profile for firmware, simulation, and deployment:

```yaml
# devenv.local.yaml
profile: cubs2
```

Enter it with `devenv shell`, or prefix a one-off command with
`devenv --profile cubs2`. The profile contains the host and ARM Zephyr
toolchains, the isolated CUBS2 West workspace, local Synapse and Rumoca
generation, CSyn and ZROS integration, and the Electrode ground-station
runtime. Changing activities should not require changing profiles.

## Choose the workflow

| Goal | Entry point | What it operates |
| --- | --- | --- |
| Build aircraft firmware | `devenv tasks run cubs2:firmware:build` | `mr_vmu_tropic` Zephyr image |
| Configure firmware | `devenv tasks run cubs2:firmware:configure` | Existing hardware build through project-owned `menuconfig` |
| Test control and physics in Modelica | `devenv tasks run cubs2:simulation:modelica:test` | Pure Modelica scenarios executed by Rumoca; no Zephyr binary |
| Test the host firmware in SIL | `devenv tasks run cubs2:simulation:sil:test` | 64-bit `native_sim`, Rumoca-generated FMI3 plant, and checks |
| Test the hardware binary in BIL | `devenv tasks run cubs2:simulation:bil:test` | Cortex-M7 image rehosted by FastDyn/QEMU against the Rumoca-generated plant |
| Compare every simulation fidelity | `devenv tasks run cubs2:simulation:compare` | Gold log, error gates, HTML report, and overlaid plots |
| Check 32-bit compatibility | `devenv tasks run cubs2:simulation:sil:build-32` | 32-bit `native_sim` SIL image |
| Exercise only the ground-station UI without hardware | See [UI simulation](#ground-station-ui-simulation) | Electrode ground station, no-serial PPM, and fake vehicle |
| Flash an aircraft controller | `devenv tasks run cubs2:firmware:flash --input confirm=true` | Previously selected hardware through the project flake |
| Run the deployed ground station | `devenv up` | Electrode ground station and PPM bridge |

Tasks are finite operations that succeed and exit. Processes are the
long-running operator-side programs used during a session. The CUBS2 project
flake continues to own West, Zephyr, SIL, menuconfig, and flash behavior;
Devenv supplies their editable cross-repository dependencies and ordering.

## Firmware development

Build the default aircraft firmware:

```sh
devenv --profile cubs2 tasks run cubs2:firmware:build
```

The first build initializes the isolated workspace under
`.devenv/state/west/cubs2/`. Later builds reuse the West checkout, Zephyr build
directory, generated sources, ccache, and sccache. The build consumes the
editable repositories under `src/`; it does not copy them into the Nix store.

The principal outputs are:

```text
src/cerebri_cubs2/build-mr_vmu_tropic/zephyr/zephyr.elf
src/cerebri_cubs2/build-mr_vmu_tropic/zephyr/zephyr.bin
```

Use the ELF for debugging and the BIN as the raw image. This board
configuration does not emit a HEX file.

Open the hardware build's Kconfig editor and then rebuild:

```sh
devenv --profile cubs2 tasks run cubs2:firmware:configure
devenv --profile cubs2 tasks run cubs2:firmware:build
```

The project selects `mr_vmu_tropic` by default. Project-supported environment
settings such as `CUBS2_BOARD` and `CUBS2_BUILD_DIR` can select another board
and a separate build directory without changing the workspace task graph.

The build initializes a missing West workspace but does not update an existing
one. Pull revisions from the CUBS2-owned `west.yml` only when intended:

```sh
devenv --profile cubs2 tasks run cubs2:workspace:update
```

## Pure Modelica simulation

Run the staged flight scenarios with both the controller and physics expressed
in Modelica and executed by Rumoca:

```sh
devenv --profile cubs2 tasks run cubs2:simulation:modelica:test
```

This delegates directly to the common package's `cubs2-qualification` flake
application. It exercises no Zephyr executable. The workspace supplies the
editable `modelica_models` checkout and local Rumoca Python package. Reports
and plots are written under:

```text
src/modelica_models/artifacts/vehicles/cubs2/
```

## Software-in-the-loop simulation

The SIL workflow builds and executes the actual 64-bit CUBS2 Zephyr
`native_sim` controller against Rumoca physics:

```sh
devenv --profile cubs2 tasks run cubs2:simulation:sil:test
```

The bounded test uses isolated UDP port 7448 so a ground station on 7447 cannot
inject pilot traffic. If an external test router already owns port 7448, reuse
it instead of starting a second router:

```sh
devenv --profile cubs2 tasks run cubs2:simulation:sil:test \
  --input reuse_router=true
```

Its dependency graph generates local Synapse and Rumoca packages, builds the
64-bit lockstep firmware, runs it with the project-owned Rumoca plant, and
writes the SIL checks and artifacts. This is the firmware regression path, not
an Electrode mock.

Build without running the scenario, or build the compatibility target:

```sh
devenv --profile cubs2 tasks run cubs2:simulation:sil:build
devenv --profile cubs2 tasks run cubs2:simulation:sil:build-32
```

The default build directories are:

```text
src/cerebri_cubs2/build-native_sim_native_64_sil/
src/cerebri_cubs2/build-native_sim_sil/
```

The SIL runner is a task because it executes a finite scenario, evaluates the
result, and exits. The project also exposes lower-level runner options through
its flake. Use those directly from the selected shell for unusual one-off
experiments rather than adding arbitrary argument forwarding to the workspace:

```sh
devenv --profile cubs2 shell
cd src/cerebri_cubs2
nix run .#native-sim-64-sil-run -- --help
```

## Binary-in-the-loop simulation

Run the Cortex-M7 hardware image under FastDyn/QEMU instead of rebuilding the
application for the host:

```sh
devenv --profile cubs2 tasks run cubs2:simulation:bil:test
```

The dependency graph builds the FastDyn runtime and patched QEMU, builds the
`mr_vmu_tropic` image with CUBS2's FastDyn configuration, builds CUBS2's Rust
shared-memory bridge, and runs CUBS2's mission smoke test. The plant is still
the CUBS2 FMI3 plant generated by Rumoca. SIL and BIL therefore describe which
Zephyr binary executes; both use Rumoca physics.

The first BIL run is substantially longer because FastDyn builds QEMU, device
models, Rumoca support, and its Python environment. Those outputs remain in
the editable FastDyn checkout and are reused by CUBS2 and RDD2:

```text
.devenv/state/fastdyn/qemu/build/qemu-system-arm
src/FastDyn/build/libfastdyn.so
src/cerebri_cubs2/build-mr_vmu_tropic-fastdyn/zephyr/zephyr.elf
src/cerebri_cubs2/artifacts/bil/work/
```

Build the BIL stack without running the mission with
`cubs2:simulation:bil:build`. Project-owned environment variables adjust the
mission without changing its Devenv dependency graph:

```sh
CUBS2_FASTDYN_T_END=20 \
CUBS2_FASTDYN_SIM_SPEED=100 \
devenv --profile cubs2 tasks run cubs2:simulation:bil:test
```

Useful settings also include `CUBS2_FASTDYN_MIN_SPEEDUP`,
`CUBS2_FASTDYN_STARTUP_TIMEOUT_S`, `CUBS2_FASTDYN_RESPONSE_TIMEOUT_S`, and
`FASTDYN_CUBS2_TIMEOUT_SEC`. The default direct lockstep path needs no host
network. `FASTDYN_CUBS2_NETWORK_SETUP=true` opts into FastDyn's privileged TAP
setup for the separate communications profile.

`cognipilot.repos` records FastDyn revision
`c235932a60a7bb839e59cac111920fb3b5cbf1aa`, which contains the generic external
vehicle-configuration support used by CUBS2. To modify FastDyn, create a branch
from the recorded commit before editing:

```sh
git -C src/FastDyn switch -c improve/cerebri-bil \
  c235932a60a7bb839e59cac111920fb3b5cbf1aa
```

After pushing the candidate commit, run `devenv tasks run sources:lock` at the
root and review the `cognipilot.repos` diff before integration testing.

## Cross-fidelity mission comparison

Run the same 40-second route in pure Modelica, SIL, and BIL, then compare all
three canonical logs:

```sh
devenv --profile cubs2 tasks run cubs2:simulation:compare
```

This is a leaf qualification task with all three mission tasks as dependencies;
it is intentionally more expensive than running one fidelity level. Its
default gold log is
`src/modelica_models/artifacts/vehicles/cubs2/mission-trajectory.csv`. The SIL
and BIL candidates are respectively:

```text
src/cerebri_cubs2/artifacts/native-sim-64-sil/mission-trajectory.csv
src/cerebri_cubs2/artifacts/bil/work/cerebri_cubs2_fmi3/mission-trajectory.csv
```

The report bundle is
`src/cerebri_cubs2/artifacts/trajectory-comparison/`. Open
`trajectory-comparison.html` for the metrics plus all figures, or use the PNGs
directly in a design review. `trajectory-comparison.json` is the machine-
readable test result.

The vehicle flake owns the acceptance limits and exposes explicit overrides:
`CUBS2_TRAJECTORY_DURATION_DELTA_MAX_S`,
`CUBS2_TRAJECTORY_POSITION_RMSE_MAX_M`,
`CUBS2_TRAJECTORY_POSITION_P95_MAX_M`,
`CUBS2_TRAJECTORY_ALTITUDE_RMSE_MAX_M`, and
`CUBS2_TRAJECTORY_ATTITUDE_P95_MAX_DEG`. Path overrides are
`CUBS2_TRAJECTORY_REFERENCE`, `CUBS2_TRAJECTORY_SIL`,
`CUBS2_TRAJECTORY_BIL`, and `CUBS2_TRAJECTORY_OUTPUT`.

To make a decoded live flight the gold standard, produce the canonical columns
`time_s,x_m,y_m,z_m,roll_rad,pitch_rad,yaw_rad`, then set both
`CUBS2_TRAJECTORY_REFERENCE` and a descriptive
`CUBS2_TRAJECTORY_REFERENCE_LABEL`. The comparison engine requires no change.

## Ground-station UI simulation

Use the Electrode fake vehicle when the goal is to exercise the operator UI,
Zenoh wiring, and PPM arbitration without running CUBS2 firmware or opening a
serial device:

```sh
PPM_NO_SERIAL=true devenv --profile cubs2 up \
  electrode-ground-station \
  electrode-ppm-bridge \
  electrode-fake-vehicle
```

Devenv builds Electrode, starts the ground station, waits for `/gcs/health`,
starts the no-serial PPM bridge, and then starts the fake vehicle. The fake
vehicle publishes both mocap and autopilot telemetry. This workflow is useful
for UI development, but it is not evidence that the CUBS2 firmware passes SIL.

The ground station prefers <http://127.0.0.1:8790/>. Devenv selects the next
available port after a conflict; use `--strict-ports` when scripts require the
preferred ports exactly.

## Deploy to an aircraft

Build and flash with an explicit hardware confirmation:

```sh
devenv --profile cubs2 tasks run \
  cubs2:firmware:flash --input confirm=true
```

The flash task depends on the firmware build. It refuses to invoke the runner
unless `confirm` is true. pyOCD is the project default; select another
board-supported runner explicitly:

```sh
CUBS2_FLASH_RUNNER=jlink devenv --profile cubs2 tasks run \
  cubs2:firmware:flash --input confirm=true
```

An empty `CUBS2_FLASH_RUNNER` lets Zephyr select its default. Flashing never
runs as a side effect of a namespace, build, test, shell activation, or process
startup.

Start the deployed ground-station stack:

```sh
PPM_SERIAL_DEVICE=/dev/ttyACM0 devenv --profile cubs2 up
```

Bare `up` starts these enabled processes:

1. `electrode-ground-station`, which serves the built Electrode web UI and owns
   the local Zenoh router.
2. `electrode-ppm-bridge`, which waits for the ground station and connects to
   its allocated Zenoh endpoint.

The bridge uses the CUBS2 AETRM channel map `1,2,0,3,4`. Override a deployment
setting with the executable's environment variables or a typed Devenv option:

```sh
devenv --profile cubs2 \
  --option processes.electrode-ppm-bridge.env.PPM_CHANNEL_MAP:string 0,1,2,3,4 \
  up
```

In this workspace session Devenv supervises the PPM bridge. Use `devenv
processes` to start, stop, restart, and inspect it rather than starting a second
copy through the ground-station UI.

Add motion capture when the deployment uses Qualisys:

```sh
QUALISYS_HOST=192.168.1.10 devenv --profile cubs2 up \
  electrode-ground-station \
  electrode-ppm-bridge \
  synapse-qualisys-bridge
```

The Qualisys bridge is available but stopped by default, so deployments without
motion capture do not enter a failed restart loop.

## Operate a process session

Foreground `up` provides a combined status and log TUI and stops the process
group on exit. For a detached deployment session:

```sh
devenv --profile cubs2 up -d
devenv --profile cubs2 processes wait --timeout 300
devenv --profile cubs2 processes list
devenv --profile cubs2 processes logs electrode-ground-station
devenv --profile cubs2 processes logs electrode-ppm-bridge
devenv --profile cubs2 processes down
```

Use the same profile for every management command. Rust and Cargo manifest
changes under `src/electrode_web` restart the affected development processes;
Cargo retains its incremental target directory.

## Task-name safety

Always run an exact task name from `devenv --profile cubs2 tasks list`. A bare
namespace runs every matching descendant. For example,
`devenv tasks run cubs2:firmware` selects configure, build, and flash tasks; the
flash confirmation still prevents a hardware write, but the namespace is not a
shortcut for the normal build.
