# Vehicle firmware and simulation

CUBS2 and RDD2 each have one developer profile. Select the vehicle, not the
activity:

```yaml
# devenv.local.yaml
profile: cubs2 # or rdd2
```

Both profiles provide the same top-level workflow vocabulary:

```text
<vehicle>:workspace:{ready,update}
<vehicle>:firmware:{build,configure,flash}
<vehicle>:simulation:modelica:test
<vehicle>:simulation:sil:{build,build-32,test}
<vehicle>:simulation:bil:{build,test}
<vehicle>:simulation:compare
```

Both vehicles implement this complete matrix. Their Modelica controller and
plant sources are common; the execution adapters differ only at the native
Zephyr and FastDyn boundaries.

## SIL, BIL, FastDyn, and Rumoca

The four modes identify where control executes. Rumoca supplies physics for
all three simulation modes and also executes control in the pure Modelica
mode. FastDyn is the ARM binary rehosting layer in BIL; it is not an alternative
to Rumoca physics.

| Mode | Control program | Physics | Execution |
| --- | --- | --- | --- |
| Pure Modelica | Modelica | Modelica | Rumoca |
| SIL | Zephyr `native_sim` host binary | Modelica | Native Zephyr process + Rumoca |
| BIL | Zephyr ARM `mr_vmu_tropic` binary | Modelica | FastDyn/QEMU + Rumoca |
| Hardware | Zephyr ARM `mr_vmu_tropic` binary | Physical vehicle | Controller hardware |

The RDD2 native and FastDyn runners dynamically load the FMI 3 plant generated
from `Vehicles.Rdd2.AvionicsPlant`; variable references and the instantiation
token are read from `modelDescription.xml`. There is no handwritten fallback
plant. Landing contact remains an eventful Modelica spring-damper interaction,
and Rumoca handles the root event internally during Co-Simulation stepping.

## Model-first development cycle

An aerospace engineer can remain in `src/modelica_models` for vehicle
templates, named parameterizations, control, estimation, physics, missions,
and qualification. The vehicle profile exposes the library's explicit export
steps when an interface artifact is needed:

```sh
devenv --profile cubs2 tasks run modelica-models:cubs2:qualify
devenv --profile cubs2 tasks run modelica-models:cubs2:export-controller
devenv --profile cubs2 tasks run modelica-models:cubs2:export-plant

devenv --profile rdd2 tasks run modelica-models:rdd2:qualify
devenv --profile rdd2 tasks run modelica-models:rdd2:export-controller
devenv --profile rdd2 tasks run modelica-models:rdd2:export-estimator
devenv --profile rdd2 tasks run modelica-models:rdd2:export-plant
```

Controller and estimator exports are eFMI Production Code. Plant exports are
FMI 3 Co-Simulation units. Firmware builds regenerate their eFMI C into the
vehicle build directory from the same source; simulation runners dynamically
load the named FMI plant. Generated artifacts never become a second editable
model source.

After model qualification and export, use `simulation:sil:test` to exercise a
host Zephyr build, `simulation:bil:test` to exercise the ARM build, and
`simulation:compare` to enforce that all three missions remain within the
vehicle's cross-fidelity budget. Hardware build and confirmed flash are the
final steps. Nothing inside `modelica_models` names those execution stages.

## Symmetric West development

Each vehicle application owns its `west.yml` and an isolated managed West
workspace:

```text
.devenv/state/west/cubs2/
.devenv/state/west/rdd2/
```

Build, simulation, configuration, and flash tasks all delegate to that
vehicle's Nix flake applications. Devenv supplies editable cross-repository
paths and task ordering; it does not create a root manifest or a shared Zephyr
module tree.

`workspace:ready` is an internal dependency that initializes a missing West
workspace and then becomes a no-op. It never updates an existing manifest.
Run the explicit update only when you want the revisions selected by the
vehicle's current `west.yml`:

```sh
devenv --profile cubs2 tasks run cubs2:workspace:update
devenv --profile rdd2 tasks run rdd2:workspace:update
```

This separation lets a developer update or temporarily modify one vehicle's
Zephyr dependencies without changing the other vehicle.

## Common use cases

Build the normal hardware image without touching a device:

```sh
devenv --profile cubs2 tasks run cubs2:firmware:build
devenv --profile rdd2 tasks run rdd2:firmware:build
```

Build the host-native SIL image:

```sh
devenv --profile cubs2 tasks run cubs2:simulation:sil:build
devenv --profile rdd2 tasks run rdd2:simulation:sil:build
```

Run the available finite regression for each vehicle:

```sh
devenv --profile cubs2 tasks run cubs2:simulation:modelica:test
devenv --profile cubs2 tasks run cubs2:simulation:sil:test
devenv --profile cubs2 tasks run cubs2:simulation:bil:test
devenv --profile rdd2 tasks run rdd2:simulation:modelica:test
devenv --profile rdd2 tasks run rdd2:simulation:sil:test
devenv --profile rdd2 tasks run rdd2:simulation:bil:test
```

The BIL tasks share FastDyn build artifacts but build different vehicle
firmware, bridge binaries, work directories, and mission reports. They are
finite Devenv tasks rather than supervised processes: each builds what is
stale, runs a bounded scenario, evaluates it, and exits.

## One mission, comparable trajectory logs

Run the complete fidelity comparison for either vehicle with one leaf task:

```sh
devenv --profile cubs2 tasks run cubs2:simulation:compare
devenv --profile rdd2 tasks run rdd2:simulation:compare
```

The task depends on that vehicle's pure Modelica, SIL, and BIL missions. Each
path flies the same time-indexed mission and writes the same canonical CSV
contract:

```text
time_s,x_m,y_m,z_m,roll_rad,pitch_rad,yaw_rad
```

The pure Modelica log is the default gold-standard log. Candidates are
interpolated at its timestamps after each log's start time is normalized. The
test gates duration delta, three-dimensional position RMSE and p95 error,
altitude RMSE, and attitude p95 error. It exits nonzero when any configured
limit is exceeded.

| Vehicle | Gold log | SIL log | BIL log |
| --- | --- | --- | --- |
| CUBS2 | `src/modelica_models/artifacts/vehicles/cubs2/mission-trajectory.csv` | `src/cerebri_cubs2/artifacts/native-sim-64-sil/mission-trajectory.csv` | `src/cerebri_cubs2/artifacts/bil/work/cerebri_cubs2_fmi3/mission-trajectory.csv` |
| RDD2 | `src/modelica_models/artifacts/vehicles/rdd2/mission-trajectory.csv` | `src/cerebri_rdd2/artifacts/sil/mission-trajectory.csv` | `src/cerebri_rdd2/artifacts/bil/work/mission-trajectory.csv` |

Each firmware repository writes its comparison bundle under
`artifacts/trajectory-comparison/`. It contains an HTML report, Markdown and
JSON metrics, and four full-resolution PNGs: combined top-down/3-D/altitude/
attitude/error views, position components, attitude components, and error
histories. SIL and BIL remain individually qualified as well; the comparison
adds the cross-fidelity regression.

The reference is a log-file interface, so a decoded hardware flight can become
the gold standard later without changing the comparator. Convert the flight
log to the seven canonical fields, then select it explicitly:

```sh
CUBS2_TRAJECTORY_REFERENCE_LABEL=flight-2026-07-16 \
CUBS2_TRAJECTORY_REFERENCE=/absolute/path/to/flight.csv \
  devenv --profile cubs2 tasks run cubs2:simulation:compare
```

RDD2 uses the corresponding `RDD2_TRAJECTORY_REFERENCE_LABEL` and
`RDD2_TRAJECTORY_REFERENCE` settings. Candidate paths, output location, and
all five limits also have vehicle-prefixed `TRAJECTORY_*` overrides. A new
flight-log format therefore needs a converter in the repository that owns that
log format, not flight-log knowledge in the common Modelica library.

## Rehosting ownership

FastDyn owns the generic QEMU runtime, peripheral-model machinery, external
configuration contract, and installation-root resolution. It does not own the
CUBS2 or RDD2 configuration. Each firmware repository owns its FastDyn TOML,
Zephyr fragments, Rust lockstep adapter, mission checks, documentation, and CI.

The workspace builds FastDyn through its `setup.sh`, Makefile, and CMake entry
points, then invokes the selected vehicle repository's configuration and
mission. An explicit `FASTDYN_ROOT` makes the same workflow usable when the
repositories are not siblings. The temporary FastDyn branch pin carries the
generic external-configuration support while that change is upstreamed.

## Arguments and lower-level experiments

Stable scenario settings use the environment variables owned by the vehicle or
FastDyn project. Examples include `CUBS2_FASTDYN_T_END`,
`RDD2_FASTDYN_MISSION_DURATION_S`, and `FASTDYN_*_TIMEOUT_SEC`. Build selection
uses project variables such as `CUBS2_BOARD`, `RDD2_BOARD`, and the matching
build-directory variables.

For options that are not part of the stable workspace workflow, enter the
vehicle profile and invoke the project-owned app directly:

```sh
devenv --profile cubs2 shell
cd src/cerebri_cubs2
nix run .#native-sim-64-sil-run -- --help
```

This is the Devenv equivalent of dropping below a ROS launch preset: the named
task is the repeatable workspace use case, while the underlying project command
retains its complete argument interface.

See [CUBS2 developer workflow](cubs2.md) and
[RDD2 developer workflow](rdd2.md) for vehicle-specific artifacts, runtime
settings, and deployment steps.
