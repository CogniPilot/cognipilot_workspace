# cognipilot_workspace

A ROS-workspace-style development environment implemented with devenv, Nix,
normal editable Git repositories, and each project's native build system. All
developer-owned repositories live under `src/`; generated state and dependency
checkouts do not.

## Bootstrap

From a checkout of this repository:

```sh
devenv shell
ws sync all        # the CUBS2-oriented default profile
ws status
```

The default profile expands to the CUBS2 and Modelica profiles. It excludes
RDD2, ROS 2, ZROS development, Qualisys, and FastDyn. Entering devenv itself
never clones or updates source repositories.

With direnv installed, `direnv allow` can replace the explicit
`devenv shell` step.

The base shell is intentionally small: Git, basic shell utilities, and the
Python/west runtime used by the workspace controller. It does not download
Rust, Node, C/C++ toolchains, a Zephyr SDK, FastDyn/QEMU dependencies, or ROS 2.
Those are supplied only by the component flake/profile when that component is
explicitly opened or built.

Repository metadata lives with the dependency graph in
[`nix/components.nix`](nix/components.nix). New clones always fetch over HTTPS,
so public bootstrap does not require an SSH key. Developers who push over SSH
can select an SSH push URL while retaining HTTPS fetches:

```sh
ws remotes ssh
ws remotes https  # switch back
```

## Source layout

```text
src/
├── FastDyn
├── cerebri_cubs2
├── cerebri_modules
├── cerebri_rdd2
├── csyn
├── csyn_ros2_bridge
├── electrode_web
├── modelica_models
├── qualisys_rust_sdk
├── rumoca
├── synapse_fbs
├── synapse_qualisys_bridge
├── zros
└── zros_drivers
```

## Workspace profiles

Profile definitions live in [`nix/profiles.nix`](nix/profiles.nix). A profile
can include other profiles; expansion is recursive, deduplicated, and rejects
include cycles. `default` includes `cubs2`, while both `cubs2` and `rdd2`
include the shared `modelica` profile (`rumoca` and `modelica_models`).

`ws sync all`, an unqualified `ws build`, and an unqualified `ws test` operate
on the active profiles. Selecting profiles alone performs no download:

```sh
ws profile default
ws sync all
ws build
```

Available profiles are `default`, `cubs2`, `rdd2`, `modelica`, `zros`,
`qualisys`, `ros2`, and `fastdyn`. Profiles can be combined. For example:

```sh
ws profile list          # includes and resolved repository membership
ws profile default qualisys  # CUBS2 plus the Qualisys bridge
ws sync all
ws build

ws profile rdd2          # RDD2 plus the shared Modelica profile
ws profile default       # return to the normal CUBS2 selection
```

Changing profiles only changes the selection in
`.devenv/state/workspace-profile`. It never deletes, resets, moves, fetches, or
otherwise modifies an existing `src/` checkout, so inactive repositories and
uncommitted work remain in place. `ws sync all` only clones missing repositories
for the new selection, and builds ignore checkouts outside the active profiles.
The generated West symlink view may be replaced, but its editable `src/`
targets are never removed.

If the private FastDyn clone is unavailable, copy a deployed FastDyn tree to
`src/FastDyn/` after selecting or explicitly syncing it.

`ws build FastDyn` and `ws shell FastDyn` activate the opt-in `fastdyn` devenv
profile. This keeps its native compiler and QEMU build prerequisites out of
normal setup.

Profile sync clones only selected repositories and their local dependency
closure, not every repository or all nested submodules. A local ROS bridge
build reuses `src/synapse_fbs`; its pinned nested copy is fetched only by an
explicit release-mode bridge build.

## Source synchronization and snapshots

`ws sync NAME` clones only missing repositories in NAME's local dependency
closure. It is idempotent and does not fetch, pull, switch, or reset an existing
checkout. Builds likewise never perform implicit network operations.

```sh
ws sync modelica_models  # rumoca, then modelica_models
ws sync cerebri_rdd2     # its complete local source closure
ws sync all              # every repository in the active profiles
```

Updating is separate and conservative. It fetches the configured branch and
uses a fast-forward-only merge; dirty, detached, differently branched, or
diverged repositories are refused:

```sh
ws update csyn
ws update all
```

Daily development follows branches and requires no workspace pin update. When
an exact integration state is needed for CI, release qualification, or a
bisect, generate a snapshot automatically:

```sh
ws freeze workspace.lock.json
git add workspace.lock.json
git commit -m "Freeze tested workspace"

ws restore workspace.lock.json
```

Freeze records full commit IDs, refuses dirty repositories, and normally
requires each commit to appear in an `origin/*` tracking ref. Restore refuses
local changes and checks out those exact commits in detached mode. Use
`git switch BRANCH` afterward to resume branch development. No Git version is
manually entered in Nix or JSON.

Repositories outside the default profile are excluded from snapshots by
default so an opt-in checkout cannot make a public snapshot unrestorable. Use
`ws freeze --include-optional FILE` only for an integration baseline that
intentionally requires every installed profile repository.

## CUBS2 release container

An exact `x.y.z` tag on `main` builds and publishes the CUBS2 development/build
image as both
`ghcr.io/cognipilot/cognipilot_workspace-cubs2:x.y.z` and `latest`. Pull
requests and ordinary branch pushes do not build this large image.

Release images require the exact component commits to be checked in as
`workspace.lock.json`. A normal release preparation is:

```sh
ws profile cubs2
ws sync all
ws mode local
ws build
ws freeze workspace.lock.json
git add workspace.lock.json
git commit -m "Freeze CUBS2 workspace for 1.2.3"
git tag 1.2.3
git push origin main 1.2.3
```

The workflow rejects non-semantic tags, tags whose commit is not reachable
from `main`, and releases without the snapshot. The image restores the
snapshot and builds the CUBS2 profile inside its pinned Nix/devenv base, then
starts in the workspace shell by default.

## Local and release dependency modes

Local mode is the default. It builds dependencies first and injects their
generated paths without changing component manifests:

```sh
ws mode local
ws build csyn
ws build electrode_web
ws build                 # complete active-profile local graph
```

Examples of the injected overrides are Cargo's command-line `paths` override,
temporary npm installs with lockfile writes disabled, CMake FetchContent source
overrides, local Python wheels, and a generated west view. The checked-in
`Cargo.toml`, `package.json`, `pyproject.toml`, CMake, and west files stay in
release form.

Release mode removes those local edges. Each component uses its checked-in
flake lock, package lock, west pins, and published dependencies, matching the
shape that its CI sees:

```sh
ws mode release
ws build csyn
ws test csyn
ws build                 # release-build every active-profile component
```

Use `ws shell COMPONENT` for a component's own pinned Nix development shell.
This is also the point at which that component's language tools are fetched.

## Dependency graph

The graph is declared in [`nix/components.nix`](nix/components.nix), composable
selections live in [`nix/profiles.nix`](nix/profiles.nix), and both are turned
into devenv task edges by [`nix/tasks.nix`](nix/tasks.nix). `ws build NAME`
runs the named node and its upstream local closure. `ws graph` prints the
summary and resolved profile membership.

```text
synapse_fbs ──┬──> csyn ───────────────┬──> cerebri_cubs2
              ├──> electrode_web       └──> cerebri_rdd2
              ├──> synapse_qualisys_bridge
              ├───────────────────────────> cerebri_cubs2
              └───────────────────────────> cerebri_rdd2
rumoca ───────┬──> modelica_models ──────> cerebri_cubs2
              ├──> electrode_web
              ├───────────────────────────> cerebri_cubs2
              └───────────────────────────> cerebri_rdd2
cerebri_modules ────────────────────────┬──> cerebri_cubs2
                                        └──> cerebri_rdd2
zros ──────────────────────────────────────> zros_drivers
qualisys_rust_sdk ─────────────────────────> synapse_qualisys_bridge
```

Nix still provides content-addressed package caching, while native Cargo, npm,
CMake, and west builds retain their own incremental build directories.
Synapse package staging is serialized across overlapping `ws` invocations so
one build cannot reset the generated bindings while another consumes them.

## One shared, reproducible west checkout

CUBS2 and RDD2 currently pin the same Zephyr and nine common west projects.
`workspace-west` reads the available application manifests and creates one
union in:

```text
${XDG_CACHE_HOME:-$HOME/.cache}/cognipilot_workspace/<workspace-id>/west/
├── shared/  # exact release pins, downloaded once
└── local/   # symlink view with local source overrides
```

The local view always maps `cerebri_modules`, `csyn`, and `modelica_models` to
the corresponding `src/` repositories. It maps `zros` only while the ZROS
profile is active; otherwise firmware uses the application manifests' pinned,
compatible ZROS revision. The release view always uses west's pinned checkouts.
Both app flakes support this through `CUBS2_WORKSPACE_ROOT` and
`RDD2_WORKSPACE_ROOT`.

Neither `devenv shell` nor `ws west validate` downloads the west projects.
The shared checkout is created lazily by `ws west sync`, or automatically the
first time a CUBS2/RDD2 build actually needs it. Initial updates use West's
narrow mode, so each GitHub repository fetches the pinned commit instead of
every branch and tag.

Only one firmware source repository is required initially. Its manifest creates
the first shared view; syncing the other firmware repository expands that same
checkout to the validated union without cloning the common projects again.

The merger compares the complete canonical definition of every duplicate
project. If URL, revision, path, or another project property differs, it fails
instead of choosing one silently. All current revisions are full Git commit
IDs. Validate or update the shared checkout with:

```sh
ws west validate       # no network
ws west sync           # fetch/update exact pins
ws west status
```

Set `COGNIPILOT_WEST_CACHE` to choose another cache location. If the applications
intentionally need different pins in the future, use their existing managed
workspace variables to create isolated west workspaces rather than weakening
the conflict check.

West itself defines exactly one manifest repository per workspace and checks
projects out at manifest revisions. See the upstream
[workspace documentation](https://docs.zephyrproject.org/latest/develop/west/basics.html)
and [manifest reference](https://docs.zephyrproject.org/latest/develop/west/manifest.html).

## Optional ROS 2 bridge

ROS 2 is not present in the base devenv closure and its flake is not evaluated
during ordinary builds. Enable it explicitly:

```sh
ws profile ros2
ws sync all
ws mode local
ws build
```

The bridge's own flake supplies ROS 2 Jazzy. Local mode builds from a disposable
source overlay under `$DEVENV_STATE`: schema generation points to
`src/synapse_fbs`, and only the disposable Cargo manifest points to the locally
staged Rust crate. The repository's manifests and pinned nested submodule stay
unchanged. Release mode runs its checked-in `nix run .#ci` flow.

## Commands

`ws` uses color for interactive status, profile, synchronization, and West
output. Colors are disabled automatically when output is redirected or piped.
Set `NO_COLOR=1` to disable them explicitly, or `CLICOLOR_FORCE=1` to preserve
colors through another command.

```text
ws sync COMPONENT|all
ws update COMPONENT|all
ws freeze [FILE]
ws restore [FILE]
ws mode [local|release]
ws profile [NAME...]
ws build [COMPONENT]
ws test [COMPONENT]
ws shell COMPONENT
ws status
ws graph
ws west {validate|sync|status|path}
ws remotes {ssh|https}
```

`ws bootstrap NAME` remains a compatibility alias for `ws sync NAME`.
