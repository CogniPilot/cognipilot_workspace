# Workspace development performance summary

Current as of 2026-07-13 on `xps15`, measured from workspace revision
`95dd34a66284f487c2e400fe59ad0f9ffecc5dbb` plus the uncommitted development
changes described in `dev/devenv-performance-roadmap.md`.

This is a local working document. The entire `/dev/` directory is deliberately
ignored by the workspace repository.

Repository-state note: at the time this summary was written, the root worktree
contained an unresolved Git conflict in `nix/tasks.nix` (`Updated upstream`
versus `Stashed changes`). The measurements below were captured from the
successfully evaluated state before that conflict appeared. Resolve it before
rerunning workspace commands or treating a new result as comparable.

## What the measurements mean

- **Cold**: build in fresh, isolated workspace `build/`, `devel/`, and task
  cache state. Shared Nix, Cargo, npm, pip, Git/download, and system caches are
  retained. This represents a new workspace build on an already provisioned
  developer machine, not a first-ever build on a clean host.
- **Warm/hot**: immediately repeat the same build with unchanged sources and
  the cold build's outputs present.
- The benchmark never deletes or modifies the developer's normal artifacts.
- Measurements are sequential wall-clock times, not CPU time.
- Cold times vary substantially with host load, network state, and shared cache
  contents. Warm times have checked regression budgets.

Run the same measurement with:

```sh
ws benchmark                          # every declared local build target
ws benchmark cerebri_cubs2            # selected target(s)
ws benchmark --check                  # enforce established warm budgets
```

Results and phase logs are retained under `dev/benchmarks/`. The complete
non-FastDyn run is `dev/benchmarks/20260713T204902Z.json`; FastDyn's repaired
run is `dev/benchmarks/20260713T210125Z.json`.

## Current build times

| Target | Source | Developer profile(s) | Cold | Warm/hot | Warm budget |
| --- | --- | --- | ---: | ---: | ---: |
| `synapse_fbs` | `src/synapse_fbs` | default, cubs2, rdd2 | 2.265s | 0.397s | 1.0s |
| `rumoca` | `src/rumoca` | default, cubs2, rdd2, modelica | 9.430s | 1.045s | 2.0s |
| `modelica_models` | `src/modelica_models` | default, cubs2, rdd2, modelica | 1.535s | 0.473s | 2.0s |
| `csyn` | `src/csyn` | default, cubs2, rdd2 | 35.525s | 0.539s | 2.0s |
| `synapse_ppm_bridge` | `src/synapse_ppm_bridge` | default, cubs2, ppm | 9.148s | 0.382s | 1.0s |
| `cerebri_modules` | `src/cerebri_modules` | default, cubs2, rdd2 | 36.101s | 0.724s | 2.0s |
| `electrode_web` | `src/electrode_web` | default, cubs2, rdd2 | 138.886s | 2.070s | 4.0s |
| `cerebri_cubs2` | `src/cerebri_cubs2` | default, cubs2 | 51.457s | 3.881s | 6.0s |
| `FastDyn` | `src/FastDyn` | fastdyn | 389.255s | 4.762s | 8.0s |

The sequential total is 673.602 seconds cold and 14.273 seconds warm. FastDyn's
one-time QEMU compile dominates the cold total. Excluding FastDyn, the cold
total is 284.347 seconds and the warm total is 9.511 seconds.

The most important hot-path improvements relative to the first full baseline
are:

| Target | Previous warm | Current warm | Result |
| --- | ---: | ---: | ---: |
| `electrode_web` | 62.309s | 2.070s | 96.7% lower |
| `cerebri_modules` | 22.681s | 0.724s | 96.8% lower |
| All previously working targets | 94.037s | 9.511s | 89.9% lower |

The task cache validates required outputs and fingerprints the component plus
its complete transitive local-source dependency closure. A missing artifact,
source change, dependency change, or build-recipe change therefore causes a
real rebuild. Firmware keeps its native CMake/Ninja incremental path rather
than treating an output stamp as a substitute for dependency tracking.

## Declared targets not currently installed

These are part of the workspace registry and are reported as `unavailable`,
not silently omitted:

| Target | Source | Profile | How to enable |
| --- | --- | --- | --- |
| `cerebri_rdd2` | `src/cerebri_rdd2` | rdd2 | `ws profile rdd2 && ws sync all` |
| `csyn_ros2_bridge` | `src/csyn_ros2_bridge` | ros2 | `ws profile ros2 && ws sync all` |
| `qualisys_rust_sdk` | `src/qualisys_rust_sdk` | qualisys | `ws profile qualisys && ws sync all` |
| `synapse_qualisys_bridge` | `src/synapse_qualisys_bridge` | qualisys | `ws profile qualisys && ws sync all` |
| `zros` | `src/zros` | zros | `ws profile zros && ws sync all` |
| `zros_drivers` | `src/zros_drivers` | zros | `ws profile zros && ws sync all` |

When one of these becomes available, `ws benchmark --check` intentionally
fails until a measured warm budget is added to
`nix/workspace-benchmark-thresholds.json`.

## Normal developer workflows

No ROS-style setup script needs to be sourced. Enter the workspace once with
`devenv shell`, or use `direnv allow`, and the local `devel/` overlay is
activated automatically.

Default CUBS2 development:

```sh
devenv shell
ws profile default
ws sync all
ws mode local
ws build                         # complete active-profile graph
ws build cerebri_cubs2           # one target plus artifact dependencies
ws test cerebri_cubs2
```

Focused profiles:

```sh
ws profile modelica              # rumoca + modelica_models
ws profile ppm                   # synapse_ppm_bridge only
ws profile fastdyn               # FastDyn only
ws profile default qualisys      # default stack plus mocap repositories
ws profile default               # return to the normal selection
```

Profiles only select repositories; selection never clones, fetches, deletes,
or resets source. `ws sync all` materializes missing repositories. `ws build`
does not sync, fetch, test, lint, package, or execute CI work.

Use `ws shell TARGET` only when an interactive component-owned toolchain shell
is needed. Ordinary builds activate the required component profile
automatically.

Release verification is deliberately separate from the developer overlay:

```sh
ws mode release
ws build TARGET
ws test TARGET
ws mode local
```

Release tasks use checked-in locks and pins and do not consume `devel/`.

## Developer launch profiles

Launch profiles supervise already-built local artifacts. Launch never builds
implicitly; a missing executable or frontend produces the exact `ws build`
command needed to create it.

| Launch profile | Processes | Required build | Current status |
| --- | --- | --- | --- |
| `simulation` | `simulation` | `ws build electrode_web` | Available |
| `ground-station` | `ground-station` | `ws build electrode_web` | Available |
| `mocap` | `mocap` | `ws build synapse_qualisys_bridge` | Source currently unavailable |
| `simulation-stack` | `simulation`, `ground-station`, `mocap` | Build Electrode and Qualisys bridge | Blocked until qualisys profile is synced |

Common launch commands:

```sh
ws launch list

ws build electrode_web
ws launch simulation
ws launch ground-station

ws profile default qualisys
ws sync all
ws build electrode_web
ws build synapse_qualisys_bridge
ws launch simulation-stack
```

With no action, `ws launch PROFILE` runs the selected profile in the
foreground. Operational commands use the same profile selection:

```sh
ws launch simulation-stack status
ws launch simulation-stack logs ground-station
ws launch simulation-stack restart simulation
ws launch simulation-stack stop mocap
ws launch simulation-stack start mocap
ws launch simulation-stack down
```

The ground station reports readiness through `http://127.0.0.1:8790/gcs/health`.
The mocap bridge reports readiness on port 8787. Process Compose restarts each
failed process up to three times. In the combined stack, the ground station is
the shared local router and simulation/mocap start in dependency order.

Launch startup/readiness times are **not yet benchmarked**. They depend on
long-running services, frontend/backend readiness, local networking, and—in the
mocap case—hardware and currently absent optional repositories. Do not compare
the build warm times above with service readiness latency.

## Current interpretation and next measurements

- Unchanged builds are now comfortably inside their warm budgets.
- CUBS2 remains intentionally dependent on CMake/Ninja's incremental graph and
  is the largest normal-profile warm build at 3.881 seconds.
- FastDyn's remaining cold cost is the real QEMU compilation; its former full
  Git-history download has been replaced by a depth-1, blob-filtered fetch.
- The next useful hot-path measurement is a small source edit per target. In
  particular, Electrode still performs npm dependency installation and a full
  frontend build after any cache-invalidating source change.
- Launch readiness should be measured separately once the Qualisys repositories
  and representative runtime/hardware are available.
