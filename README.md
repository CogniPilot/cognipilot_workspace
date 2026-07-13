# cognipilot_workspace

A native Nix development environment for coordinating multiple editable Git
repositories with devenv and each project's own build system. All
developer-owned repositories live under `src/`. Physical dependency checkouts
stay in the shared user cache, with generated links exposed below `src/` for
native West discovery.

## Prerequisites

- x86-64 Linux for the qualified CUBS2 container and firmware workflow.
- direnv is optional.

### Fresh Linux host

Start by installing the host tools used to download Nix and clone this
workspace. For example, on Debian or Ubuntu:

```sh
sudo apt-get update
sudo apt-get install -y ca-certificates curl git xz-utils
```

Use the equivalent packages from the host distribution on other Linux
systems. The official Nix installer requires an HTTPS downloader and may
require `xz`; this workspace also requires Git.

Install Nix with the official recommended multi-user installer:

```sh
curl --proto '=https' --tlsv1.2 -L https://nixos.org/nix/install \
  | sh -s -- --daemon
```

Use the official [Nix download instructions](https://nixos.org/download/) for
a single-user installation, WSL, a system without `sudo`, or a distribution
whose native Nix package is preferred. Open a new terminal after installation,
then verify it:

```sh
nix --version
```

Enable the Nix CLI and flake features used by this workspace in
`~/.config/nix/nix.conf`:

```text
experimental-features = nix-command flakes
accept-flake-config = true
```

Install the exact devenv CLI version required by this checkout:

```sh
nix profile add --accept-flake-config "github:cachix/devenv/v2.1.2"
devenv version
```

The version command must report `2.1.2`. Ensure that `~/.nix-profile/bin` is
on `PATH` if the newly installed `devenv` command is not found in a fresh
terminal.

Finally, clone this workspace over HTTPS if it is not already checked out:

```sh
git clone https://github.com/CogniPilot/cognipilot_workspace.git
cd cognipilot_workspace
```

The devenv and Cachix binary caches substantially reduce the first install.
On a multi-user Nix installation, a system administrator may add these lines
to the system Nix configuration before installing devenv, then restart the Nix
daemon:

```text
extra-substituters = https://devenv.cachix.org https://cachix.cachix.org
extra-trusted-public-keys = devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw= cachix.cachix.org-1:eWNHQldwUO7G2VkjpnjDbWwy4KQ/HNxht7H4SSoMckM=
```

Trusting a binary cache allows its correctly signed builds into the local Nix
store, so this remains an explicit administrator decision. Without these
entries the setup still works, but it may download or build a much larger
dependency closure.

For ROS 2, add the component flake's cache to the system-level trusted Nix
configuration so Nix does not rebuild ROS Jazzy from source:

```text
trusted-substituters = https://cache.nixos.org https://ros.cachix.org
trusted-public-keys = cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY= ros.cachix.org-1:dSyZxI8geDCJrwgvCOHDoAfOm5sV1wCPjBkKL+38Rvo=
```

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

With direnv installed, `direnv allow` loads the same environment through the
checked-in `.envrc` and can replace the explicit `devenv shell` step.

The base shell is intentionally small: Git, basic shell utilities, and the
Python/west runtime used by the workspace controller. It does not download
Rust, Node, C/C++ toolchains, a Zephyr SDK, FastDyn/QEMU dependencies, or ROS 2.
Those are supplied only by the component flake/profile when that component is
explicitly opened or built.

Repository metadata lives with the dependency graph in
[`nix/components/default.nix`](nix/components/default.nix). New clones always fetch over HTTPS,
so public bootstrap does not require an SSH key. Developers who push over SSH
can select an SSH push URL while retaining HTTPS fetches:

```sh
ws remotes ssh
ws remotes https  # switch back
```

## Source layout

```text
src/
├── .west -> shared local West view
├── FastDyn
├── cerebri_cubs2
├── cerebri_modules
├── cerebri_rdd2
├── csyn
├── csyn_ros2_bridge
├── electrode_web
├── modelica_models
├── modules/... -> shared pinned West projects
├── qualisys_rust_sdk
├── rumoca
├── synapse_fbs
├── synapse_ppm_bridge
├── synapse_qualisys_bridge
├── zephyr -> shared pinned Zephyr checkout
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
`qualisys`, `ros2`, `ppm`, and `fastdyn`. Profiles can be combined. For example:

```sh
ws profile list          # includes and resolved repository membership
ws profile default qualisys  # CUBS2 plus the Qualisys bridge
ws sync all
ws build

ws profile ppm           # host PPM serial bridge only
ws sync all
ws build synapse_ppm_bridge

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

`ws status` shows repositories selected by the active profiles plus their local
dependency closure. Use `ws status --all` to include inactive and optional
repositories as well. Both forms are local-only and perform no fetch.

If the private FastDyn clone is unavailable, copy a deployed FastDyn tree to
`src/FastDyn/` after selecting or explicitly syncing it.

`ws build FastDyn` and `ws shell FastDyn` activate its declared opt-in devenv
toolchain profile. This keeps its compiler and QEMU prerequisites out of normal
setup. FastDyn is externally owned and is never part of a workspace release.

The CUBS2/default selection includes the host-side Rust bridge from Synapse
manual-control messages to a serial PPM encoder; the standalone `ppm` profile
selects only that bridge. Its Rust and libudev toolchain is loaded only by a
build, test, or component shell, and the bridge is not started automatically.

Profile sync clones only selected repositories and their local dependency
closure, not every repository or all nested submodules. A local ROS bridge
build reuses `src/synapse_fbs`; its pinned nested copy is fetched only by an
explicit release-mode bridge build.

## Supervised launch profiles

Devenv launch profiles are small, composable process sets. They are separate
from `ws` repository profiles: `ws profile` selects repositories and build
tasks, while `ws launch` selects long-running processes and never changes the
repository selection.

The current launch profiles are:

- `simulation`: Electrode's fake Synapse vehicle publisher. It publishes both
  autopilot and mocap data alone, or only autopilot data when `mocap` is also
  selected.
- `ground-station`: the Electrode Ground Station daemon and built web UI at
  <http://127.0.0.1:8790/>.
- `mocap`: the Synapse Qualisys bridge and dashboard at
  <http://127.0.0.1:8787/>. Set `QUALISYS_HOST` to select a QTM host; its
  repository default is `127.0.0.1`.
- `simulation-stack`: a convenience profile extending all three profiles.

List the available launch profiles, launch an ad-hoc combination, or use the
convenience stack:

```sh
ws launch list
ws launch simulation ground-station mocap
ws launch simulation-stack
```

Profile and process names complete after pressing Tab in an interactive devenv
Bash shell. Completion definitions are also installed in the environment for
Bash, Zsh, and Fish. The direct devenv form remains available when needed:
`devenv -P simulation -P ground-station -P mocap up`.

The stacked profiles coordinate their Zenoh topology automatically. The Ground
Station owns the local UDP/WebSocket router when selected; otherwise the mocap
bridge owns it, followed by the simulator as the standalone fallback. Other
selected processes connect as clients, so combining profiles does not create a
port-7447 bind race.

The launch commands consume the local generated Synapse and Rumoca packages.
Prepare them once with the normal workspace graph before the first launch:

```sh
ws profile default qualisys
ws sync all
ws build electrode_web
ws build synapse_qualisys_bridge
```

Use the same launch profile when controlling or inspecting its process manager:

```sh
ws launch simulation-stack status
ws launch simulation-stack logs simulation
ws launch simulation-stack restart mocap
ws launch simulation-stack stop ground-station
ws launch simulation-stack start ground-station
ws launch simulation-stack down
```

Process definitions and their central registry live in [`launch/`](launch/)
and use devenv readiness, startup ordering, restart policies, per-process
working directories, and profile-local runtime state. Component repositories
do not need their own launch files; their toolchains are still provided lazily
by their own flakes when the process starts.

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

Release CI archives every frozen commit as a standalone Git bundle and attaches
the bundle set to the workspace GitHub release. If an upstream branch or tag is
later deleted, extract that archive and restore without contacting the original
repositories:

```sh
ws restore --bundle-dir /path/to/extracted/workspace-sources workspace.lock.json
```

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
ws test
ws freeze workspace.lock.json
git add workspace.lock.json
git commit -m "Freeze CUBS2 workspace for 1.2.3"
git tag 1.2.3
git push origin main 1.2.3
```

The workflow rejects non-semantic tags, tags whose commit is not reachable
from `main`, and missing or empty snapshots. It generates its temporary
Dockerfile with `scripts/render-workspace-dockerfile cubs2 OUTPUT`, restores the
snapshot, builds and tests the selected profile, and only then publishes the
final image as `ghcr.io/cognipilot/cognipilot_workspace-cubs2`. Each release is
tagged with the semantic version, its immutable `sha-COMMIT` identifier, and
`latest`:

```sh
docker pull ghcr.io/cognipilot/cognipilot_workspace-cubs2:1.2.3
docker run --rm -it ghcr.io/cognipilot/cognipilot_workspace-cubs2:1.2.3
```

The same renderer can produce another profile image without adding a
Dockerfile to the repository root.

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
overrides, Rumoca's store-native Python environment, and a generated west view. The checked-in
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

Component flake references use Git filtering, so `.git`, `target/`,
`node_modules/`, and ignored build outputs are not copied into the Nix store.
Tracked modifications are visible. A new untracked source file is rejected
with an actionable `git add -N` command rather than being silently omitted;
stage it normally or use intent-to-add before building.

## Dependency graph

The typed graph is declared in [`nix/components/default.nix`](nix/components/default.nix), composable
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

After the shared checkout is prepared, the workspace creates a generated
`src/.west` anchor and links pinned dependency projects such as `src/zephyr`
and `src/modules/...` into the local view. Existing real directories are never
replaced or deleted, so editable component repositories remain ordinary Git
worktrees. This also prevents West from walking past the workspace and
accidentally selecting an unrelated parent `.west` directory.

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

The `west` command in the workspace shell is a lazy wrapper. It prepares the
shared view, then enters the owning CUBS2/RDD2 Nix shell (or the generic Zephyr
tools profile) before invoking the pinned West executable. Direct native usage
therefore works without installing Python requirements globally:

```sh
cd src/cerebri_cubs2
west build -b native_sim -p
```

Use `ws build cerebri_cubs2` when the full local dependency DAG and generated
local Synapse/Rumoca artifacts are required.

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
ws commit [COMPONENT|all] [-m MESSAGE]
ws push [COMPONENT|all]
ws freeze [--allow-unpushed] [--include-optional] [FILE]
ws restore [FILE]
ws mode [local|release]
ws profile [NAME...]
ws build [COMPONENT]
ws test [COMPONENT]
ws shell COMPONENT
ws launch [list|PROFILE...] [ACTION [PROCESS]]
ws status [--all]
ws graph
ws west {validate|sync|status|path}
ws remotes {ssh|https}
```
