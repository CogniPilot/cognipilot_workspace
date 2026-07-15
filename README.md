# CogniPilot development workspace

This is a standard [Devenv](https://devenv.sh/) polyrepo workspace. Devenv
provides the shells, profiles, task DAG, processes, completion, and Cachix
integration. Each project keeps authority over its own build and release tools.

## Start

With Nix installed:

```sh
./setup
```

The script only installs or updates Devenv and enters `devenv shell`. It does
not edit system Nix configuration. To install manually instead:

```sh
nix profile add github:cachix/devenv/407080febcc800abfd0fd688a0d513884aad620c
devenv shell
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
devenv --profile ground-station shell
devenv --profile simulation shell
```

Inside or outside the shell, run the native Devenv tasks:

```sh
devenv --profile cubs2 tasks run cubs2:build
devenv --profile cubs2 tasks run cubs2:test
devenv --profile ground-station tasks run electrode-web:test
devenv --profile release tasks run release:all
```

`release:all` is deliberately a dry run. Publishing remains an explicit
project-native command after the dry run succeeds.

Start supervised processes with Devenv itself:

```sh
devenv --profile ground-station up
devenv --profile simulation up
devenv --profile simulation processes list
devenv --profile simulation down
```

Available profiles include `cubs2`, `rdd2`, `ground-station`, `simulation`,
`modelica`, `qualisys`, `zros`, `ros2`, `ppm`, `fastdyn`, `diagnostics`,
`release`, `rust`, `web`, and `zephyr`. Set a personal default without changing
the repository:

```yaml
# devenv.local.yaml (ignored)
profile: cubs2
```

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
Cargo uses the shared Devenv sccache directory instead. `CACHIX_AUTH_TOKEN` is
only a write credential for authorized CI or manual cache publication, never
the public signing key. Main CI realizes the complete `release` and `fastdyn`
tool environments before the Cachix action uploads new Nix-built paths.

See [Project environments](docs/project-environments.md) for the project-flake
boundary and local cross-repository workflow.
