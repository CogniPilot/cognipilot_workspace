# CogniPilot workspace

A Nix-first development workspace for CogniPilot. Project flakes define the
package graph, tools, actions, artifacts, launches, and source selection.
flake-parts composes those definitions and Nix generates the editable task
graph. The standalone Rust [`nixspace`](tools/nixspace/) client exposes that
Nix-generated interface as `ws`; it does not maintain a second workspace model.

## Start here

Install `git` and `curl`, then run:

```sh
git clone https://github.com/CogniPilot/cognipilot_workspace.git
cd cognipilot_workspace
./setup
```

`./setup` is the only setup command. It bootstraps Nix if necessary, delegates
host configuration to the Nix-built client, and realizes the Nix-built `ws`
entrypoint. Nix substitutes it from the configured caches when available and
otherwise builds it locally. Setup finishes by running `ws doctor`. To verify an
already configured host without changing it:

```sh
./setup --check
```

After setup, use `./ws` directly; do not install devenv by hand or source a
workspace setup script. The CLI uses Clap, so `./ws --help` and each
subcommand's `--help` are the command reference.

## Common commands

```sh
./ws doctor
./ws cache coverage --json
./ws benchmark                 # bounded default performance cases
./ws benchmark --all           # include evaluator-heavy diagnostics
./ws package list
./ws package show electrode_web
./ws graph --dot

./ws sync                     # clone missing public repositories
./ws status                   # public repository status; no fetch
./ws update PACKAGE           # checked fast-forward update

./ws build --plan PACKAGE     # inspect Nix-selected devenv task roots
./ws build PACKAGE
./ws test PACKAGE
./ws env PACKAGE --explain    # show exact LOCAL/LOCKED selections and bindings
./ws package prefix PACKAGE
./ws resource PACKAGE/NAME
./ws run electrode_web/ground-station -- --help

./ws launch list
./ws launch show electrode_web/ground-station
./ws launch plan electrode_web/ground-station --set health-port=8791
./ws launch up electrode_web/ground-station --name demo --detach
./ws launch status demo
./ws launch logs demo
./ws launch down demo
```

Source commands default to the Nix-emitted public repository set. Explicit
`./ws sync all`, `status all`, or `update all` opts into every declared source,
including FastDyn's separately locked private source checkout; use it only when
that private checkout is intended.

## Project model

- A source repository may export `flakeModules.default`; otherwise an external
  flake or downstream fork can define it without modifying upstream. The
  product lock selects exactly one complete definition.
- Shared Cargo, npm, CMake, west/Zephyr, and Twister presets generate routine
  actions and state paths. Projects declare only meaningful native differences.
- Generated devenv tasks run editable actions while native tools keep their
  incremental build state. Devenv's task cache tracks declared inputs. The
  Nix-built client validates typed artifact results and publishes an atomic
  generation that readers hold through use.
- Each command uses one Nix-generated local/locked resolution. Missing,
  incompatible, stale, or unsafe mixed selections fail explicitly; a selected
  local result never falls back to a locked result after failure.
- West owns Zephyr workspace operations in isolated, content-addressed product
  workspaces. Devenv and process-compose own process supervision.
- The supported flake systems are `x86_64-linux`, `aarch64-linux`, and
  `aarch64-darwin`.
- `nixspace` is an independently locked, publishable Cargo package. It may be
  installed with `cargo install nixspace`; `ws` is only CogniPilot's
  Nix-provided alias.

## Caching

The public cache contract is the union of `cache.nixos.org` and the
`cognipilot` Cachix cache: upstream paths need not be mirrored into Cognipilot.
Main-only CI is configured to build and push the explicit
`.#public-cache-root`; it does not scan arbitrary flake outputs. No successful
remote union publication proof is retained yet. The root includes public
project inputs/outputs, the public product definition, and the exact
`nixspace-host` and `ws` wrappers used by `./setup`, their generated plans, and
completions. First-run workspace tooling can therefore be substituted as one
closure after main CI successfully publishes it. A local coverage report is an
inventory, not proof of publication; only a successful main run and a complete
union query establish that the closure is present. Protected `main` now
requires the strict three-system GitHub Actions check matrix, a CODEOWNER
approval, resolved conversations, and linear history; those controls protect
the publisher but do not substitute for a successful publication run.

FastDyn's separately locked source is private, and that source plus any private
release output are excluded from the public cache root. Its integration
definition is deliberately committed to this public product, so the definition
and its declared checkout metadata are public. `private` protects source/output
store closures; it cannot redact information intentionally committed to a
public flake. A future private cache must use separate credentials and retention
policy.

Setup also trusts upstream caches for independently pinned tools. Host v4 and
cache report v2 retain per-store diagnostics but decide completeness per path:
every closure path must exist in at least one declared union store.

Editable Cargo, npm, CMake/Ninja, west/Twister, and colcon outputs are mutable
native build state, not Cachix content. `CACHIX_AUTH_TOKEN` is a CI write
credential, not the public signing key; keep it in protected organization CI
secrets, never in a flake, repository, or developer setup.

The shared locked-Cargo actions use the Nix-selected `sccache` wrapper and share
`.nixspace/state/sccache`; CI exercises the same compiler-cache boundary with
GitHub's standard cache backend. Project definitions do not configure it.

## More detail

- [Workspace user guide](dev/workspace-user-guide.md)
- [Nix/Rust workflow boundary](dev/nix-rust-workflow-boundary.md)
- [Project flake provider contract](dev/devenv-flake-provider-contract.md)
- [Implementation roadmap](dev/devenv-implementation-roadmap.md)
- [Promotion record](dev/cognipilot-promotion-record.md)
