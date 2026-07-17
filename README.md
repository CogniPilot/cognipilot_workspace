# CogniPilot development workspace

This is the canonical [Devenv](https://devenv.sh/) workspace for editable
CogniPilot development. Devenv selects tool environments, schedules the task
DAG, supervises processes, installs workspace hooks, and integrates the public
Cachix caches. Each repository continues to own its Cargo, npm, CMake, West,
colcon, Meson, and Nix behavior.

## Five-minute start

Install Git and curl, clone this repository, and run:

```sh
./setup
devenv tasks run sources:sync
```

`setup` installs Nix when necessary, installs the workspace's pinned Devenv
release, and enters the small base shell. `sources:sync` clones the editable
repositories into `src/` or fetches each repository's configured branch for an
existing checkout. It never changes an existing checkout's active branch.
Most repositories use `main`; FastDyn temporarily starts at a verified revision
of its external-configuration branch while the generic installation-root
support is being upstreamed. Source sync fetches newer branch work without
moving an existing checkout.

Choose the work area you use most often in the ignored local configuration:

```yaml
# devenv.local.yaml
profile: cubs2
```

Then either enter it explicitly:

```sh
devenv shell
```

or enable Devenv's native automatic activation. Add the appropriate hook to
your shell configuration, restart the shell, and trust this checkout once:

```sh
# ~/.bashrc
eval "$(devenv hook bash)"

# ~/.zshrc uses: eval "$(devenv hook zsh)"
devenv allow
```

After that, entering the workspace activates the selected local profile and
leaving it deactivates the environment. No direnv installation is required.

If Nix reports that cache settings are restricted, trust the public caches once
at the host level. These commands prompt for elevation when required:

```sh
nix run nixpkgs#cachix -- use cognipilot
nix run nixpkgs#cachix -- use devenv
nix run nixpkgs#cachix -- use ros
```

## Pick a profile

Profiles are developer work areas. Internal Rust, Web, Zephyr, and diagnostics
tool modules are composed into these profiles instead of being exposed as more
choices.

| Profile | Use it for | Good first command |
| --- | --- | --- |
| `synapse` | Synapse schema and generated packages | `tasks run synapse-fbs:test` |
| `modelica` | Rumoca and Modelica integration | `tasks run modelica-models:test` |
| `electrode` | Electrode ground-station code | `tasks run electrode-web:test` |
| `qualisys` | Qualisys SDK and bridge | `up synapse-qualisys-bridge` |
| `ppm` | Synapse-to-PPM bridge | `up synapse-ppm-bridge` |
| `zros` | ZROS, CSyn, and Cerebri Zephyr modules | `tasks run zros:test` |
| `ros2` | CSyn ROS 2 bridge | `tasks run ros2:test` |
| `cubs2` | CUBS2 firmware, SIL/BIL simulation, and aircraft deployment | `tasks run cubs2:simulation:sil:test` |
| `rdd2` | RDD2 model, firmware, SIL/BIL, and deployment development | `tasks run rdd2:simulation:sil:test` |
| `fastdyn` | FastDyn C/C++, Rust, and Python work | `tasks run fastdyn:test` |
| `release` | Cross-project qualification without publishing | `tasks run release:all` |
| `workspace` | Every tool and the published OCI shell | `container build shell` |
| `ci` | Minimal workspace configuration validation | `test` |

The example column is the portion after `devenv --profile <profile>`. With a
default in `devenv.local.yaml`, omit `--profile <profile>`:

```sh
devenv tasks list
devenv tasks run cubs2:simulation:sil:test
devenv up
```

For a one-off different environment, override the local default:

```sh
devenv --profile modelica shell
devenv --profile rdd2 tasks run rdd2:simulation:bil:test
```

Task discovery is profile-scoped. The base environment shows only source and
workspace maintenance; each work profile adds the development tasks supported
by its tools. Publication and cross-project qualification tasks appear only in
the `release` and `workspace` profiles.

### Exact tasks and namespaces

Always use the complete task name shown by `devenv tasks list`. In Devenv, a
bare namespace such as `devenv tasks run cubs2` means "run every task whose
name begins with `cubs2:`"; it is not an alias for the usual build.

Flashing additionally requires explicit confirmation, so namespace execution
cannot write to connected hardware:

```sh
devenv --profile cubs2 tasks run cubs2:firmware:flash --input confirm=true
devenv --profile rdd2 tasks run rdd2:firmware:flash --input confirm=true
```

## Common work

Use one `cubs2` profile for the complete vehicle workflow:

```sh
devenv --profile cubs2 tasks run cubs2:simulation:modelica:test
devenv --profile cubs2 tasks run cubs2:simulation:sil:test
devenv --profile cubs2 tasks run cubs2:simulation:bil:test
devenv --profile cubs2 tasks run cubs2:simulation:compare
devenv --profile cubs2 tasks run cubs2:firmware:build
devenv --profile cubs2 tasks run \
  cubs2:firmware:flash --input confirm=true
devenv --profile cubs2 up
```

The first command runs the pure Modelica controller and physics with Rumoca.
The second runs the CUBS2 64-bit Zephyr `native_sim` controller against Rumoca
physics. The third rehosts the Cortex-M7 binary under FastDyn/QEMU and retains
Rumoca physics. The fourth runs all three and gates their overlaid canonical
trajectory logs. The fifth builds the aircraft image without touching hardware.
The confirmed flash command deploys that firmware, and `up` runs the
operator-side Electrode ground station plus its PPM bridge. See the
[CUBS2 developer workflow](docs/cubs2.md) for firmware configuration, 32-bit
compatibility, UI-only simulation, Qualisys, process arguments, artifacts, and
deployment operations.

Use the same workflow shape for RDD2:

```sh
devenv --profile rdd2 tasks run rdd2:simulation:modelica:test
devenv --profile rdd2 tasks run rdd2:simulation:sil:test
devenv --profile rdd2 tasks run rdd2:simulation:bil:test
devenv --profile rdd2 tasks run rdd2:simulation:compare
devenv --profile rdd2 tasks run rdd2:firmware:build
```

SIL runs a host-built Zephyr binary with Rumoca physics; BIL rehosts the
Cortex-M7 hardware binary with FastDyn/QEMU and also uses Rumoca physics. Pure
Modelica runs both control and physics in Rumoca without Zephyr. See
[Vehicle firmware and simulation](docs/vehicle-development.md) and the
[RDD2 developer workflow](docs/rdd2.md) for the exact current capability
boundary.

The first vehicle build initializes only that vehicle's isolated West
workspace. Later builds reuse it. Fetch the current revisions from the
project-owned manifest explicitly when desired:

```sh
devenv --profile cubs2 tasks run cubs2:workspace:update
devenv --profile rdd2 tasks run rdd2:workspace:update
```

After `cubs2:firmware:build`, the principal firmware files are:

```text
src/cerebri_cubs2/build-mr_vmu_tropic/zephyr/zephyr.elf
src/cerebri_cubs2/build-mr_vmu_tropic/zephyr/zephyr.bin
```

`zephyr.elf` is the linked ARM executable with debug symbols; `zephyr.bin` is
the raw flash image. This board configuration does not emit a HEX file.

The CUBS2 flash task uses pyOCD by default. Select another board-supported
runner with `CUBS2_FLASH_RUNNER`, or set it to an empty string to let Zephyr
choose:

```sh
CUBS2_FLASH_RUNNER=jlink devenv --profile cubs2 tasks run \
  cubs2:firmware:flash --input confirm=true
```

Build or test the ground station without the full CUBS2 tool environment:

```sh
devenv --profile electrode tasks run electrode-web:build
devenv --profile electrode tasks run electrode-web:test
```

For native Rumoca development, use Rumoca's project-owned flake so Cargo sees
the exact nightly toolchain, components, WASM target, Node release, and native
libraries selected by `rust-toolchain.toml` and `flake.nix`:

```sh
cd src/rumoca
nix develop
cargo xtask --help
cargo xtask vscode edit
```

The root `modelica` profile is the cross-repository integration environment:

```sh
devenv --profile modelica tasks run rumoca:compiler
devenv --profile modelica tasks run modelica-models:test
devenv --profile modelica tasks run modelica-models:cubs2:qualify
devenv --profile modelica tasks run modelica-models:rdd2:qualify
```

This is the aerospace-engineering home: reusable vehicle templates, named
CUBS2/RDD2 parameters, controllers, contact physics, missions, and FMI/eFMI
exports all live in `src/modelica_models`. Execution-mode names begin only at
the firmware and workspace integration boundary.

More goal-oriented command sequences are in
[Development workflows](docs/workflows.md).

## Run supervised processes

Run the deployed CUBS2 ground-station stack in the foreground with the native
Devenv process manager:

```sh
devenv --profile cubs2 up
```

This starts `electrode-ground-station`, waits for its health endpoint, and then
starts `electrode-ppm-bridge` against its allocated Zenoh endpoint. It expects
the configured PPM serial device.

Run the operator UI with a no-serial fake vehicle instead of aircraft firmware:

```sh
PPM_NO_SERIAL=true devenv --profile cubs2 up \
  electrode-ground-station electrode-ppm-bridge electrode-fake-vehicle
```

Select other focused process entry points when needed:

```sh
devenv --profile qualisys up synapse-qualisys-bridge
devenv --profile ppm up synapse-ppm-bridge
```

Use detached mode when you want to inspect or control processes from the same
terminal:

```sh
devenv --profile cubs2 up -d
devenv --profile cubs2 processes wait --timeout 300
devenv --profile cubs2 processes list
devenv --profile cubs2 processes logs electrode-ground-station
devenv --profile cubs2 processes restart electrode-ppm-bridge
devenv --profile cubs2 processes down
```

The CUBS2 PPM bridge defaults to `/dev/ttyACM0`; override it with
`PPM_SERIAL_DEVICE`. Its Zenoh connection follows the port allocated to the
ground station.

See [Supervised development processes](docs/processes.md) for the available
process matrix, startup lifecycle, process-versus-task guidance, and proposed
hardware-free development additions.

## Source and build boundaries

Every editable repository remains under `src/`. CUBS2 and RDD2 each own their
checked-in `west.yml` and use an isolated workspace under
`.devenv/state/west/`. The root never creates shared `.west/`, `zephyr/`,
`modules/`, or `models/` trees.

Cross-repository task edges generate local Synapse and Rumoca artifacts before
passing their editable paths to downstream native tools. Mutable Cargo targets,
node modules, West workspaces, and build directories remain outside the Nix
store and retain native incremental behavior. Shared ccache and sccache state
lives below `.devenv/cache/`.

See [Project environments](docs/project-environments.md) for the detailed
project-flake boundary.

## Qualify and publish the workspace environment

```sh
devenv --profile release tasks run release:qualify
devenv --profile release tasks run release:all
```

These commands qualify integrations, packages, and firmware without publishing
project releases. Each project remains responsible for its own release tag and
publication workflow.

A workspace `vX.Y.Z` tag has a separate purpose: CI qualifies the workspace,
builds the complete `workspace` profile as a Devenv OCI shell container, and
publishes the version plus `latest` to GitHub Container Registry:

```sh
docker pull ghcr.io/cognipilot/cognipilot-workspace:v0.1.0
docker run --rm -it -v "$PWD:/env" \
  ghcr.io/cognipilot/cognipilot-workspace:v0.1.0
```

The container contains the immutable tool environment, never a snapshot of
editable sources or build trees. It is generated by `devenv container`; there
is no Dockerfile or second package graph.

## Maintain the workspace

```sh
devenv tasks run sources:status
devenv test
devenv changelogs
devenv update
devenv gc
```

`devenv test` validates the configuration and runs the root hooks. CI also
evaluates every supported profile on x86_64 Linux, AArch64 Linux, and
Apple-silicon macOS. Zephyr firmware execution and flashing remain
Linux/hardware-specific.

The `cognipilot` and `ros` Cachix caches provide Nix-built environments and
packages. Native editable outputs are intentionally not Cachix artifacts.
`CACHIX_AUTH_TOKEN` is a CI write credential only and is not required for public
cache downloads.
