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
devenv tasks run workspace:sync
```

West pins each repository once in [`manifest/west.yml`](manifest/west.yml) and
its [`zephyr.yml`](manifest/zephyr.yml) submanifest.
Editable Cargo targets, CMake build trees, node modules, and West workspaces
stay outside `/nix/store` and retain their native incremental behavior.

## Work

Select a complete environment with a normal Devenv profile:

```sh
devenv --profile cubs2 shell
devenv --profile rdd2 shell
```

Inside or outside the shell, run the native Devenv tasks:

```sh
devenv --profile cubs2 tasks run cubs2:build
devenv --profile cubs2 tasks run cubs2:test
devenv --profile cubs2 tasks run electrode-web:test
devenv --profile release tasks run release:all
```

`release:all` runs the workspace integration checks, hardware firmware builds,
and host package dry runs without publishing. Its compliance gate rejects Rust
consumers that do not use the workspace Synapse version. Cross-platform
archives, wheels, VSIX packages, and publication remain authoritative in each
project's tag workflow. Projects have independent versions, so after this
preflight create each project's own release tag rather than inventing one
workspace-wide version.

Start supervised processes with Devenv itself:

```sh
devenv --profile cubs2 up
devenv --profile cubs2 processes list
devenv --profile cubs2 down
```

Available profiles include `cubs2`, `rdd2`, `modelica`, `qualisys`, `zros`,
`ros2`, `ppm`, `fastdyn`, `diagnostics`, `release`, `rust`, `web`, and `zephyr`.
The CUBS2 profile includes its simulation, ground station, and Qualisys
processes. Set a personal default without changing the repository:

```yaml
# devenv.local.yaml (ignored)
profile: cubs2
```

The supported hosts are x86_64 Linux, AArch64 Linux, and Apple-silicon macOS.
Zephyr firmware execution and flashing remain Linux/hardware-specific.

## Maintain

```sh
devenv tasks list
devenv tasks run workspace:status
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
