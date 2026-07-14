# CogniPilot workspace user guide

This workspace coordinates editable CogniPilot repositories through a
versioned Nix interface. Nix supplies the static package graph and exact plans;
the Rust `nixspace` client presents them and delegates work to Git, devenv,
process-compose, west, and the project-native build tools.

The workspace does not scan `src/`, infer dependencies, or maintain a second
Python/Bash project model.

## Bootstrap

Install `git` and `curl`, clone the repository, and run the checked-in setup
shim:

```sh
git clone https://github.com/CogniPilot/cognipilot_workspace.git
cd cognipilot_workspace
./setup
./ws doctor
```

`./setup` bootstraps Nix when it is missing, then hands host configuration to
the Nix-built `nixspace` client. It also realizes the repository-local `ws`
entrypoint. It is not necessary to install devenv manually or enter a shell
before using `ws`.

Use the read-only form to audit an existing host:

```sh
./setup --check
```

The generated host plan requires Nix 2.18 or newer, `nix-command`, flakes,
flake-config acceptance, the pinned devenv version, and the declared public
substituters and signing keys. Setup changes only the marked nixspace block and
keeps a one-time `.pre-nixspace` backup. It refuses a symlink-managed
`nix.conf` and NixOS, where the settings must be applied declaratively.

The host policy currently declares `trusted-users = *`. Nix trusted users are
effectively root-capable through the daemon, so use this policy only on a
developer machine where every local user is trusted.

The root flake supports `x86_64-linux`, `aarch64-linux`, and `aarch64-darwin`.
CI is configured with a native runner and main-only stable-cache publication
for every system. The branch is not yet protected, and this configuration is
not a claim that the remote matrix or cache publication has already passed.

## The command line

`ws` is a repository convenience alias for the independently locked
`nixspace` binary. Its command tree, usage, validation, and Bash/Zsh/Fish
completion are generated from Clap:

```sh
./ws --help
./ws package --help
./ws launch --help
```

The generic client can also be installed with `cargo install nixspace`. A
standalone installation needs explicit paths to the versioned index and plans;
the CogniPilot `ws` package supplies those paths automatically.

### Inspect the Nix interface

These commands read the generated index. They do not scan repositories, build
packages, or start processes:

```sh
./ws package list
./ws package show cerebri_cubs2
./ws target list cerebri_cubs2
./ws target show cerebri_cubs2/default
./ws artifact list synapse_fbs
./ws executable list electrode_web
./ws launch list
./ws graph
./ws graph --reverse electrode_web
./ws graph --dot
```

Collection list/show commands accept `--json`; `graph` accepts `--json` or
`--dot`. Nix has already validated IDs, references, cycles, artifact contracts,
and launch structure before emitting the index.

The index is an ordinary Nix output selected by the root flake. Interactive
queries consume its exact bytes; they do not repeatedly evaluate project
flakes.

### Editable repositories

The root flake emits a `SourceWorkspace` plan containing each repository path,
canonical Git URL and branch, precomputed package selection, and exact Git
argv. Git remains the authority for checkout and ancestry semantics.

```sh
./ws sync
./ws sync electrode_web
./ws status
./ws status electrode_web
./ws update electrode_web
```

`sync` clones only missing repositories. It preflights the entire selection
before the first clone and never changes an existing checkout. `status` is
local-only. `update` verifies every selected worktree, origin, branch, and clean
state before networking; it then fetches the full selection, proves every
update fast-forwardable, and only then performs fast-forward merges. No build
or launch command implicitly fetches or changes source control state.

With no selector, source commands use the Nix-emitted public default. The
explicit selector `all` includes private sources, currently FastDyn:

```sh
./ws sync all
./ws status all
./ws update all
```

This is an opt-in disclosure boundary, not a convenience synonym for the
default. A package selector uses that package's exact Nix-precomputed source
closure.

Editable repositories normally live below `src/`. Their exact paths are root
policy in the generated plan, not assumptions built into Rust.

### Editable actions

```sh
./ws build --plan cerebri_cubs2
./ws build --plan --json electrode_web
./ws build electrode_web
./ws test electrode_web
./ws env electrode_web --explain
./ws package prefix electrode_web
./ws resource electrode_web/web-assets
./ws run electrode_web/ground-station -- --help
```

Nix precomputes the selected devenv task roots and their dependency edges.
`nixspace` selects one emitted plan and calls its declared runner; devenv owns
task execution and the project-native tool owns the build. There is no Rust
dependency planner or generic workspace scheduler.

Every generated task contains one typed argv, cwd, environment overlay,
artifact inputs and outputs, and sorted exclusive lock paths. After a native
task succeeds, `nixspace` validates every declared output and invokes the exact
Nix-emitted `nix hash path` command. It installs an immutable generation and
atomically switches the task's `current` pointer only after the command,
validation, proof, manifest, and devenv result all succeed. Readers verify the
exact Nix-selected identity and proof under a shared generation lease. A
failed, missing, stale, or tampered local generation never falls back to a
locked result. Devenv's conventional task cache may restore a previous
successful task result when its declared inputs are unchanged; there is no
separate workspace cache implementation.

The Nix-generated `WorkspaceResolution` document selects a complete local or
locked dependency closure for each package command. `env --explain` reports
the same candidate, contract, path, and `LOCAL dirty`, `LOCAL commit`, or
`LOCKED` provenance used by prefix, resource, executable, build/test
authorization, and launch execution. Unsafe mixed compiled selections are
refused rather than repaired heuristically. `nixspace` does not search a
global `PATH`, `PYTHONPATH`, or CMake prefix for an alternative.

Cargo, npm, CMake/Ninja, west/Twister, and colcon keep their persistent native
incremental state. CPU and memory requirements are currently metadata only;
exclusive locks are enforced by the client. The atomic cutover deleted the old
central implementation, so every editable action comes from the generated Nix
graph with no compatibility fallback.

### West and Zephyr

West remains the authority for Zephyr manifest discovery, revisions, module
metadata, and commands. Nix selects one exact manifest source and emits a
versioned `WestWorkspace` plan. `nixspace` materializes its content-addressed
workspace and an isolated editable view, then invokes the Nix-selected west or
another emitted command:

```sh
./ws west validate
./ws west sync
./ws west status
./ws west path --mode local
./ws west path --mode release
./ws west exec -- build -b native_sim/native/64
```

The immutable workspace cache is below the platform cache in the namespace
chosen by Nix. Editable views are workspace-relative. West's native
`--path-cache` and narrow update support are used for reuse; there is no shared
global `src/.west` workspace and no duplicate West dependency graph in Nix or
Rust.

### Launches and sessions

Nix validates the launch IR and emits both the cached static plan and an exact
`LaunchExecution` plan. Devenv renders process configuration;
process-compose owns startup, readiness, restarts, logs, signals, and shutdown.

```sh
./ws launch list
./ws launch show electrode_web/ground-station
./ws launch plan electrode_web/ground-station --set health-port=8791
./ws launch up electrode_web/ground-station --name gcs --detach \
  --set health-port=8791
./ws launch sessions
./ws launch status gcs
./ws launch logs gcs
./ws launch attach gcs
./ws launch down gcs
```

Parameter-only changes require no Nix evaluation. `nixspace` validates typed
values, creates declared session-relative paths, fills socket/log placeholders,
and stores a redacted session record. Secret values are environment-only and
must not enter Nix, argv, plans, session metadata, completion, or normal logs.
The client routes later commands to the recorded process-compose socket; it
does not supervise processes itself.

TCP and exact HTTP GET/status readiness probes are implemented as small typed
client operations invoked by process-compose. Session state is isolated by
name. Nix may mark a port as automatic with a preferred value; the client
serializes allocation, reserves the selected endpoint through manager start,
records the resolved claim, and rejects incomplete cleanup. Two concurrent
fixture stacks prove distinct ports and session state without building any
CogniPilot application. Expensive end-to-end launch performance remains a
roadmap gate.

## Nix and Cachix caching

The host plan declares these public substituters:

- `devenv.cachix.org`
- `cachix.cachix.org`
- `cache.nixos.org`
- `cognipilot.cachix.org`
- `ros.cachix.org`

`./ws doctor` compares the effective Nix configuration with the generated
expectations. The public keys verify downloaded paths; they are not upload
credentials.

Main CI is configured to realize and publish the explicit root package on all
three native systems. Branch protection is still an explicit governance gate:

```sh
nix build --accept-flake-config --out-link result-public-cache-root \
  .#public-cache-root
./ws cache coverage --json
```

This root references the immutable public source/definition inputs, public
release roots, and shared Nix tooling selected for CogniPilot. CI can therefore
push a deliberate closure instead of scanning arbitrary flake attributes.
No successful remote union publication proof is retained yet, so the current
repository proves the publication configuration, not a successful upload.
When that out-link already exists, doctor and the dedicated cache command use
Nix's structured `path-info` interface to compare its complete local closure
with the Host v4 cache union. Cache report v2 retains per-store diagnostics and
reports union hit/miss path counts, NAR bytes, and the smallest compressed
download size advertised for each covered path. Completeness means every path
is present in at least one of `cache.nixos.org` or the public CogniPilot cache;
upstream paths need not be duplicated. A missing out-link is skipped before any
Nix command or cache connection, so routine doctor does not evaluate or realize
the flake.
`transferredBytes` remains `null`: a cache inventory does not prove how many
bytes an earlier build actually transferred, and the report never relabels
NAR/download sizes as observed network traffic.
Pull-request CI retains the read-only JSON inventory without requiring hits.
The configured main path repeats it after the Cachix publication step, requires
a complete closure, appends a table to the GitHub step summary, and retains the
JSON report as a 30-day workflow artifact. Neither reporting path receives or
prints the write token.

FastDyn's separately locked source is marked `private`. The public cache root
excludes that source store path and any private release outputs. Its integration
definition is committed to the public product source and is intentionally
public. Metadata is written without Nix string context so a private source store
path appearing as audit data cannot pull that content into the public closure.
A future private cache must have a separate token and access policy; cache
boundaries follow source/output visibility rather than repository count.

`CACHIX_AUTH_TOKEN` is the cache-scoped write token. It belongs in the
CogniPilot organization Actions secret (or a narrower repository secret), not
in `flake.nix`, `nix.conf`, a setup script, or a developer shell. Pull requests
are read-only. Main publication steps receive the token and upload the public
cache root; tag publication is intentionally disabled until a release-tag
convention exists.

Mutable editable outputs are intentionally outside Cachix. Native language and
build-system caches keep them fast; only immutable Nix store closures can be
substituted safely. A successful branch-protected main push and empty-host
whole-closure substitution are still explicit release roadmap gates.

## Project integration

A project definition is a flake-parts module exported as
`flakeModules.default`. It can be:

- in the source repository;
- an external flake wrapping an unmodified source; or
- a downstream fork selected as one complete replacement.

Source and definition are independently pinned root flake inputs. Exactly one
complete definition is selected; fields are never merged between authorities
and there is no runtime fallback.

Normal projects select a versioned Cargo, npm, CMake, west/Zephyr, Twister, or
combined preset and add only semantic differences: IDs, meaningful variants,
artifacts, resources, custom argv, or launches. Shared modules generate task
JSON, state roots, cache policy, checks, and conventional action plumbing.
Project definitions must not contain workspace-specific Python orchestration or
substantial Bash hidden in Nix strings.

The standalone Rust client has no path/Git dependencies on project crates, does
not join their Cargo workspaces, and contains no CogniPilot package names or
flake output names. Workspace-specific values cross the versioned Nix data and
process boundary.

## Immutable outputs and promotion

`packages.<system>.workspace`, `promotion-record`, `promotion-sbom`, and
`promotion-attestation` are Nix outputs derived from the committed root flake
and lock. The record captures exact package/source/definition/builder
identities and the transitive closure with NAR proofs. The SBOM is SPDX JSON
2.3; the attestation is an in-toto Statement v1 with the SLSA provenance v1
predicate. All three promotion documents are projections of the same Nix
product graph rather than editable manifests.

Most current project entries are qualification-only and do not expose natural
deployable release outputs. Mutable `src/`, native build directories, and local
artifact generations are never deployable merely because a workspace command
produced them. Pure product builds, release isolation, stable cache
publication, and second-host substitution remain Phase 5 gates.

## Further reference

- [Workspace contract glossary](workspace-contract-glossary.md)
- [Nix/Rust workflow boundary](nix-rust-workflow-boundary.md)
- [Project flake provider contract](devenv-flake-provider-contract.md)
- [Generated task layer](cognipilot-devenv-task-layer.md)
- [Generated launch layer](cognipilot-devenv-launch-layer.md)
- [West workspace design](nixspace-west-workspace.md)
- [Promotion record](cognipilot-promotion-record.md)
- [Implementation roadmap](devenv-implementation-roadmap.md)
