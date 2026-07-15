# CogniPilot development workspace

This is a standard [Devenv](https://devenv.sh/) polyrepo workspace. Devenv
provides the shells, profiles, task DAG, processes, completion, and Cachix
integration. Each project keeps authority over its own build and release tools.

## Start

Install Git and curl, then run:

```sh
./setup
```

The script installs Nix when it is missing, installs the workspace's pinned
Devenv release, and enters `devenv shell`. It does not edit an existing Nix
configuration. To install Devenv manually instead:

```sh
nix profile add github:cachix/devenv/407080febcc800abfd0fd688a0d513884aad620c
devenv shell
```

If Nix reports that cache settings are restricted, trust the public caches
once at the host level (the command prompts for elevation when required):

```sh
nix run nixpkgs#cachix -- use cognipilot
nix run nixpkgs#cachix -- use devenv
nix run nixpkgs#cachix -- use ros
```

Then fetch the editable repositories:

```sh
devenv tasks run sources:sync
```

Generated Devenv tasks clone missing repositories under `src/` and fetch
`origin/main` for existing checkouts without changing their active branches.
Vehicle builds never use a root West manifest. CUBS2 and RDD2 each use their
own checked-in `west.yml` and isolated workspace below
`.devenv/state/west/`. No shared `zephyr/`, `modules/`, `models/`, or `.west/`
tree is created at the root. Editable targets and build trees stay outside
`/nix/store` and retain their native incremental behavior.

## Work

Select a complete environment with a normal Devenv profile:

```sh
devenv --profile cubs2 shell
devenv --profile rdd2 shell
```

Inside or outside the shell, run the native Devenv tasks:

```sh
devenv --profile cubs2 tasks run cubs2:build-native-64
devenv --profile cubs2 tasks run cubs2:test
devenv --profile rdd2 tasks run rdd2:build
devenv --profile cubs2 tasks run electrode-web:test
devenv --profile release tasks run release:all
```

The first vehicle build updates only that vehicle's West workspace. You can do
so explicitly with `cubs2:west-update` or `rdd2:west-update`.

### Common workflows

```sh
# Start only the Electrode ground station.
devenv --profile cubs2 up ground-station

# Build CUBS2 native simulator firmware (64-bit or 32-bit).
devenv --profile cubs2 tasks run cubs2:build-native-64
devenv --profile cubs2 tasks run cubs2:build-native-32

# Build CUBS2 firmware for mr_vmu_tropic. The task also prepares the
# vehicle's isolated West workspace and generated local dependencies.
devenv --profile cubs2 tasks run cubs2:build-hardware

# Flash that build to a connected mr_vmu_tropic board (pyOCD by default).
devenv --profile cubs2 tasks run cubs2:flash

# Start the Synapse-to-PPM serial bridge.
devenv --profile ppm up ppm-bridge

# Build the Rumoca compiler.
devenv --profile modelica tasks run rumoca:compiler
```

After `cubs2:build-hardware` succeeds, the principal firmware files are:

```text
src/cerebri_cubs2/build-mr_vmu_tropic/zephyr/zephyr.elf
src/cerebri_cubs2/build-mr_vmu_tropic/zephyr/zephyr.bin
```

`zephyr.elf` is the linked ARM executable with debug symbols; `zephyr.bin` is
the raw flash image. This board configuration does not emit a HEX file.

`cubs2:flash` rebuilds only when an input changed, then invokes `west flash`
against that same build directory. It uses the `pyocd` runner by default. Set
`CUBS2_FLASH_RUNNER` to another Zephyr runner, or to an empty string to let
Zephyr choose the board default:

```sh
CUBS2_FLASH_RUNNER=openocd devenv --profile cubs2 tasks run cubs2:flash
```

The PPM bridge defaults to `/dev/ttyACM0` and Zenoh at
`udp/127.0.0.1:7447`. Override these with `PPM_SERIAL_DEVICE` and
`ZENOH_CONNECT` when starting it.

To edit the Rumoca VS Code extension from its repository:

```sh
devenv --profile modelica shell
cd src/rumoca
cargo xtask vscode edit
```

`--profile modelica` adds the Rust/Web/Synapse toolchain plus Julia and the
scientific Python packages used for Modelica development. Without a profile,
the intentionally small base shell contains only Git and `jq`.

The Devenv shell obtains its Nix toolchain and system dependencies from the
configured binary caches. Editable Rust artifacts remain in Rumoca's `target/`
tree and are reused through Cargo plus the workspace sccache; they are not
copied into the Nix store on every edit.

`release:all` runs the workspace integration checks, hardware firmware builds,
and host package dry runs without publishing. Its compliance gate rejects Rust
consumers that do not use the workspace Synapse version. Cross-platform
archives, wheels, VSIX packages, and publication remain authoritative in each
project's tag workflow. Projects have independent versions, so after this
preflight create each project's own release tag rather than inventing one
workspace-wide version for project packages.

A workspace `vX.Y.Z` tag has one separate purpose: CI builds the complete
`workspace` profile as a Devenv OCI shell container and publishes both that
version and `latest` to GitHub Container Registry:

```sh
docker pull ghcr.io/cognipilot/cognipilot-workspace:v0.1.0
docker run --rm -it ghcr.io/cognipilot/cognipilot-workspace:v0.1.0
```

The container is generated by `devenv container`; there is no Dockerfile or
second package graph. Tag CI also writes every Nix path it builds to the
`cognipilot` Cachix cache. GHCR authentication uses GitHub Actions' built-in
`GITHUB_TOKEN`, so no registry secret is required. The organization must only
provide the existing `CACHIX_AUTH_TOKEN` Actions secret for cache uploads.

Start supervised processes with Devenv itself:

```sh
devenv --profile cubs2 up
devenv --profile cubs2 processes list
devenv --profile cubs2 down
```

## Profiles

A profile composes the packages, environment, and processes needed for one
kind of work. It does not change repository revisions; the same editable
sources and Devenv tasks are used by every profile.

Every profile supports these forms:

```sh
devenv --profile <name> shell            # interactive tool environment
devenv --profile <name> tasks list       # inspect the task DAG
devenv --profile <name> tasks run <task> # build or test
devenv --profile <name> up [process]     # supervise one or all profile processes
devenv --profile <name> down             # stop detached processes
```

The example column below is the portion after `devenv --profile <profile>`.

| Profile | Purpose | Common examples |
| --- | --- | --- |
| `rust` | Rust compiler, Cargo, rust-analyzer, Clippy, rustfmt, and sccache | `shell`, then use project-native `cargo build` or `cargo test` |
| `web` | Node.js, wasm-pack, and browser-package development | `shell`, then use project-native `npm run build` or `npm test` |
| `zephyr` | West, Zephyr host tools, ARM toolchain, flashing, and native simulation | `tasks run cerebri-modules:test` |
| `synapse` | Rust + Web with Synapse generation and publishing tools | `tasks run synapse-fbs:build`; `tasks run synapse-fbs:test` |
| `modelica` | Synapse plus Julia and scientific Python for Rumoca/Modelica work | `tasks run rumoca:compiler`; `tasks run modelica-models:test` |
| `qualisys` | Synapse, Zenoh, Playwright, and the Qualisys bridge process | `tasks run qualisys-bridge:test`; `up qualisys-bridge` |
| `ppm` | Rust plus the supervised `ppm-bridge` process | `tasks run ppm:test`; `up ppm-bridge` |
| `zros` | Synapse plus the Zephyr environment for ZROS/module work | `tasks run zros:build`; `tasks run zros:test` |
| `ros2` | Synapse environment for the project-owned ROS 2 flake workflow | `tasks run ros2:test` |
| `cubs2` | Complete CUBS2 environment, ground station, simulation, and Qualisys bridge | `tasks run cubs2:test`; `up ground-station`; `up` for the full process set |
| `rdd2` | Complete RDD2 Modelica and Zephyr environment | `tasks run rdd2:build`; `tasks run rdd2:build-hardware` |
| `fastdyn` | FastDyn, QEMU, C/C++, Rust, and build-system tooling | `tasks run fastdyn:build`; `tasks run fastdyn:test` (slow cold QEMU setup) |
| `diagnostics` | Clang diagnostics and benchmarking tools | `tasks run workspace:validate`; `test` |
| `release` | CUBS2 environment plus release and Cachix tooling | `tasks run release:qualify`; `tasks run release:all` |
| `workspace` | Complete release and FastDyn toolchain; source of the published development container | `shell`; `container build shell` |
| `ci` | Minimal diagnostics environment used by workspace CI | `test` |

Set a personal default without changing the repository:

```yaml
# devenv.local.yaml (ignored)
profile: cubs2
```

The supported hosts are x86_64 Linux, AArch64 Linux, and Apple-silicon macOS.
Zephyr firmware execution and flashing remain Linux/hardware-specific.

## Maintain

```sh
devenv tasks list
devenv tasks run sources:status
devenv --profile cubs2 tasks run cubs2:west-update
devenv --profile rdd2 tasks run rdd2:west-update
devenv test
devenv update
devenv gc
```

The `cognipilot` Cachix cache stores Nix-built environments and packages.
Native editable build directories are intentionally not Cachix artifacts;
all profiles share workspace-local ccache and sccache directories instead.
`CACHIX_AUTH_TOKEN` is only a write credential for authorized CI, never the
public signing key. Fast PR CI only evaluates the workspace. The separate
`Warm project Nix cache` workflow realizes tool environments and bounded
project flake outputs on main, weekly, or by manual dispatch, then Cachix
uploads every new Nix path. Protect its `cachix-write` GitHub environment and
restrict the organization secret to repositories allowed to populate the
trusted cache.

See [Project environments](docs/project-environments.md) for the project-flake
boundary and local cross-repository workflow.
