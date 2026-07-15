# Nix/Rust workflow boundary and shell deletion ledger

Status: implemented architecture and retained cutover ledger, 2026-07-14. The
cutover retained no compatibility wrappers.

## Decision

The workspace has four owners, and a piece of behavior must have exactly one of
them:

| Owner | Owns | Must not own |
| --- | --- | --- |
| Nix modules and flake outputs | Project declarations, source bindings, normalized package/product indexes, validated dependency closures, action and launch DAGs, tool versions, immutable packages, release artifacts, OCI images, checks, cache roots, and public substituter configuration | Mutable checkout operations, interactive prompts, process supervision, or opaque shell programs |
| Generated devenv tasks/processes | Thin editable-action nodes and process-manager records rendered from the normalized Nix index | Discovering projects, planning a graph, implementing a build system, or embedding multi-step Bash programs |
| Reusable Rust `nixspace` client (`ws` is a root alias) | Typed CLI parsing, reading versioned Nix-generated plans, atomic local state, diagnostics, presentation, and coordinating calls to established tools | CogniPilot-specific semantics, defining workspace semantics, discovering a graph, emulating Git/west/colcon/CMake/Cargo/npm, or deciding undeclared dependencies |
| Project-native tools | Git checkout semantics; west manifest and Zephyr operations; colcon/ament ROS discovery, ordering, install prefixes, and environment hooks; Cargo, npm, CMake/CTest, and Twister builds/tests | Defining the cross-project CogniPilot product graph |

The only checked-in executable shell shims are root `setup` and root
`ws`. `setup` installs Nix when necessary and immediately transfers control to a
Nix-built setup command. `ws` immediately transfers control to the Nix-built
`nixspace` client. Neither shim parses the workspace model. `.envrc` remains a
small conventional direnv/flake-root declaration, not a workspace control-plane
script. Shell completion adapters are generated from the Rust CLI and packaged
by Nix; they are not maintained by hand.

This division also fixes caching semantics:

- Release and distributable results are Nix derivations. Main-only CI is
  configured to upload the explicit `public-cache-root` closure to the
  `cache.nixos.org` + `cognipilot.cachix.org` coverage union; private sources
  and outputs are excluded by Nix visibility policy. Publication is not
  claimed until that workflow and a complete union query succeed, and branch
  protection remains a separate governance gate.
- Editable builds use the native incremental state of Cargo, npm, CMake/Ninja,
  west/Twister, and colcon. They are not falsely presented as Cachix artifacts.
- Generated devenv tasks express ordering. The workspace does not add a second
  generic content cache around the native build systems.
- The flake's `nixConfig` is the public read/trust declaration. The
  `CACHIX_AUTH_TOKEN` remains a CI-only write credential and is never placed in
  Nix data, generated files, or a developer setup script.
- FastDyn's separately locked source and any private release output are private;
  its integration definition is committed inside the public product and is
  intentionally public. Even when the private source store path is recorded as
  audit data, discarded Nix string context prevents that text from pulling the
  source into the public closure.

`nixspace` is a real standalone product, not a repository-local Cargo xtask.
The crate and binary are both named `nixspace`, have their own lock, build
without Nix or any workspace source dependency, and are publishable. The
checkout installs with `cargo install --locked --path tools/nixspace`; the
standard `cargo install nixspace` command becomes available after the initial
crates.io release. Its role is analogous to colcon's role over project build
tools: it presents one workspace UX while delegating actual semantics and
execution. The important difference is that Nix evaluates the complete,
versioned workspace interface; `nixspace` does not scan directories or infer a
second graph.

The generic interface contains packages, targets, actions, artifacts,
resources, executables, launches, precomputed plans, and external-tool
invocations. A flake-parts module materializes that interface as a conventional
buildable index output. All flake references, output names, state locations,
and expected cache settings are explicit interface/CLI inputs. CogniPilot owns
the Nix modules that populate the interface and may expose `ws` as a convenience
alias, but the Rust crate contains no `COGNIPILOT_*` protocol or package names.

## Checked-in shell deletion and move table (completed)

All paths below were tracked before the atomic cutover and are now deleted.
The table preserves the reasoning for their authoritative replacements.

| Current path | Current responsibility | Cutover disposition | Authoritative replacement |
| --- | --- | --- | --- |
| `.envrc` | Enter devenv through direnv | Keep as conventional two-line host integration; it must remain free of workspace decisions | direnv + devenv |
| `completions/ws.bash` | Bash adapter around `ws _complete` | Delete as hand-maintained source | Generate with the Rust CLI completion implementation and package as a Nix output |
| `completions/ws.fish` | Fish adapter around `ws _complete` | Delete as hand-maintained source | Generate with the Rust CLI completion implementation and package as a Nix output |
| `completions/_ws` | Zsh adapter around `ws _complete` | Delete as hand-maintained source | Generate with the Rust CLI completion implementation and package as a Nix output |
| `scripts/benchmark-workspace` | Select build targets, run cold/warm builds, time them, write reports, and enforce budgets | Delete | Defer a replacement until Nix emits a typed benchmark plan; native tools must still perform builds, and expensive benchmark proof remains a roadmap gate |
| `scripts/build-csyn-ros2` | Copy a synthetic ROS source overlay, rewrite a Cargo manifest, run colcon build/test, and inspect results | Delete, with no generic Rust port | The project flake declares local dependency bindings; generated tasks invoke colcon directly; colcon/ament own discovery, ordering, build/install/log roots, tests, and environment hooks |
| `scripts/lint-workspace` | Discover files and sequence actionlint, nixfmt, deadnix, statix, shfmt, shellcheck, and ruff | Delete | Root flake `checks`/treefmt configuration declares checks; CI and a generated `lint` task invoke `nix flake check` or the declared formatter |
| `scripts/render-workspace-dockerfile` | Generate a Dockerfile that re-bootstraps Nix/devenv and runs `ws` in Docker layers | Delete | Product flake exports cacheable OCI packages using `dockerTools.buildLayeredImage` or `nix2container`; CI copies/pushes that Nix output |
| `scripts/west-workspace` | Choose a product shell, inject Zephyr paths and source directory, then run west | Delete | Nix emits the selected product's west input record; `ws west` coordinates a content-addressed local view; native west owns `build`, `twister`, `list`, `status`, and manifest behavior |
| `scripts/workspace-cached-task` | Lock a task, consult the custom cache, run a command, and publish a stamp | Delete without replacement | Generated devenv task ordering plus project-native incremental builds; immutable results use Nix/Cachix |
| `scripts/workspace-cmake-build` | Hash configuration, guess whether a Ninja tree is reusable, fall back to west, and stamp it | Delete | CMake presets and `cmake --build`, or west's native `--pristine=auto`; project flake/action data supplies declared arguments |
| `scripts/workspace-flake-ref` | Inspect a Git worktree and synthesize `git+file` flake URLs | Delete | Flake inputs and Nix source bindings are the sole source authority; `ws doctor` may report untracked-file hazards from the generated binding but cannot invent a second source coordinate |
| `scripts/workspace-release-environment` | Check Git cleanliness and scrub development paths before release commands | Delete | Promotion consumes committed flake inputs through explicit immutable Nix packages/checks; there is no replacement mutable `ws release` mode |
| `scripts/workspace-task-cache` | Invent metadata/content hashing for repositories and mutable outputs | Delete without a Rust rewrite | Nix store hashes immutable outputs; each project-native build tool owns editable incremental validity |
| `scripts/ws` | Entire legacy workspace control plane in Bash | Delete | Normalized Nix outputs + generated devenv tasks/processes + the Nix-built Rust client + project-native tools, as detailed below |

Workspace-control-plane and workspace-test Python were deleted during the
direct cutover. Static contract checks now use Nix assertions and client
behavior uses Cargo tests. Project-native Python under `src/` is unaffected.

### Working-tree shell candidates reviewed during the cutover

These paths were reviewed so the cutover did not replace one shell control
plane with another. Root `setup` and `ws` now have only the accepted bootstrap
and exec behavior; the rejected candidates were not adopted.

| Candidate | Disposition |
| --- | --- |
| `setup` | Keep only after reducing it to Nix installation detection/bootstrap and `exec nix run ...#nixspace -- setup`; move host diagnosis/configuration to `nixspace` with expected public cache data supplied by Nix |
| `ws` | Keep only after reducing it to `exec` of the Nix-built `nixspace` client; delete Python/devenv/Bash command routing |
| `scripts/archive-workspace-sources` | Do not adopt; export a Nix source-closure/bundle derivation from exact flake inputs |
| `scripts/workspace-cache-config.sh` | Do not adopt; place public URLs/keys in flake `nixConfig` and expose any setup metadata as generated JSON |
| `scripts/ws-doctor` | Do not adopt; move typed host/cache/index checks and fixes to `ws doctor`; Nix emits expected values |
| `scripts/ws-index` | Do not adopt; normalized index is a Nix output; Rust may atomically materialize/cache that exact output but may not recompute it |
| `scripts/ws-query` | Do not adopt; move display/filter/completion over the normalized index to typed Rust |
| `tests/test-cognipilot-static-query` | Do not adopt as a shell test; split semantic evaluation into Nix checks and CLI behavior into Rust integration tests |

## `scripts/ws` function-by-function cutover

The Nix-owned plan mentioned below must contain already-resolved products,
closures, task IDs, launch records, and source bindings. Rust selects among
declared plans and executes them; it does not traverse the dependency graph to
create a new plan.

| Bash function | Delete/move result |
| --- | --- |
| `register_cleanup` | Rust RAII temporary-file/directory guards |
| `cleanup` | Rust RAII/drop cleanup |
| `component_is_active` | Delete; read Nix-generated product membership/status fields |
| `state_write` | Rust atomic state-file helper with typed schemas |
| `print_worktree_status` | Rust presentation over `git status --porcelain=v2`; Git retains status semantics |
| `require_manifest` | Rust typed loading/version check of the Nix-generated normalized index |
| `require_launch_manifest` | Rust typed loading/version check of the Nix-generated launch plan |
| `launch_profile_names` | Read names from the Nix-generated launch index |
| `validate_launch_profile` | Nix validates references; Rust only validates a CLI value against generated names |
| `launch_processes` | Delete graph merge logic; Nix launch renderer emits the resolved process set |
| `validate_launch_process` | Nix validates membership; Rust checks the requested name against the resolved generated set |
| `list_launch_profiles` | Rust presentation of generated launch records |
| `launch_workspace` | Rust argument translation to generated devenv profiles/process-manager commands; devenv/process-compose supervise |
| `complete_workspace_command` | Rust CLI completion over Nix-generated indexes; adapters generated and packaged by Nix |
| `require_component` | Rust typed lookup in the normalized Nix index |
| `component_value` | Delete ad-hoc jq queries; use typed generated records |
| `component_path` | Read the resolved local source binding from Nix-generated data |
| `component_has_task` | Read generated action/task availability |
| `devenv_profile_args` | Delete profile inference; Nix emits exact generated task/profile invocation data |
| `require_ros_cache_trust` | `ws doctor` compares `nix config show --json` with flake-generated expected public cache settings; setup applies the fix |
| `component_closure_field` | Delete graph traversal; Nix computes and validates named closures |
| `component_closure` | Read the Nix-generated source closure |
| `component_build_closure` | Read the Nix-generated action/artifact closure |
| `clear_selection` | Ordinary local Rust collection initialization; no persisted semantic state |
| `add_target` | Delete graph-building helper; consume the generated ordered plan |
| `add_closure` | Delete graph-building helper; consume the generated source closure |
| `add_build_closure` | Delete graph-building helper; consume the generated build/action closure |
| `read_current_products` | Rust typed state read; default product name comes from Nix data |
| `current_products` | Rust typed command context |
| `snapshot_products` | Rust immutable per-command context |
| `west_product_signature` | Nix emits canonical west product identity/content key |
| `selected_west_signature` | Nix emits canonical combined product identity/content key |
| `select_command_product` | Delete ownership/signature inference; Nix emits valid owner choices and disambiguation records, Rust reports ambiguity |
| `product_label` | Rust presentation only |
| `products_csv` | Replace environment-string protocol with a typed generated record/path passed to `ws west` |
| `validate_product` | Nix validates definitions; Rust checks CLI values against generated product IDs |
| `select_components` | Delete traversal; select a named precomputed Nix source plan |
| `select_build_components` | Delete traversal; select a named precomputed Nix action plan |
| `current_protocol` | Rust typed local preference read; this is presentation policy, not graph semantics |
| `github_urls` | Read declared, normalized fetch/push URLs from Nix data; do not reconstruct provider URLs in Rust |
| `set_push_protocol` | Rust coordinates native `git remote`/`git config` calls |
| `sync_one` | Rust performs atomic path handling and invokes native `git clone`; all repository coordinates come from Nix data |
| `sync_selection` | Rust iterates the already-ordered Nix source plan and calls native Git |
| `check_update_one` | Rust preflight around native Git status/branch queries |
| `fetch_update_one` | Rust invokes native `git fetch` with generated repository data |
| `check_update_fast_forward` | Rust invokes native `git merge-base`; Git defines ancestry |
| `merge_update_one` | Rust invokes native `git merge --ff-only` |
| `update_selection` | Rust coordinates the generated repository plan's preflight/fetch/check/merge phases |
| `lifecycle_selection` | Delete closure calculation; Rust maps a generated repository plan to local bindings and appends the root only when the command contract declares it |
| `commit_selection` | Rust prompt/presentation and sequencing around native `git add`/`git commit` |
| `push_selection` | Rust bounded concurrency and reporting around native `git push` |
| `check_selection_present` | Rust checks paths in the generated source plan; deployed-tree permission is normalized Nix data |
| `freeze_workspace` | Rust writes a typed lock from generated repository declarations and native Git revisions after clean/pushed preflight |
| `materialize_snapshot_one` | Rust atomic checkout materialization around native Git/bundle operations |
| `restore_workspace` | Rust validates the typed lock against the Nix-generated repository plan, preflights all repositories, then invokes native Git |
| `run_workspace_tasks` | Delete graph planning; Rust invokes the exact generated devenv task IDs/plan; devenv schedules nodes and native tools build/test |
| `usage` | Rust CLI help generated from typed command definitions |

### Legacy command-dispatch arms

Some `scripts/ws` behavior is implemented directly in the final `case`, not in
a named function. Its destination is still explicit:

| Command arm | Destination |
| --- | --- |
| `help`, `_complete` | Rust CLI; completion adapters generated by Nix |
| `sync`, `update`, `commit`, `push`, `remotes`, `status` | Rust coordination over generated repository plans and native Git |
| `freeze`, `restore` | Remove from the current command surface; the committed root `flake.lock` is the product input authority and Nix may export immutable source archives |
| `product` | Remove mutable product-selection state; product definitions and closures remain root Nix data |
| `build`, `test`, `release` | `build`/`test` select exact generated devenv task plans; remove the legacy release arm and expose immutable promotion as flake packages/checks |
| `benchmark` | Remove until a typed Nix benchmark plan and performance contract are implemented |
| `shell` | Remove the workspace-specific shell router; project shells remain conventional flake outputs when explicitly requested through Nix |
| `launch` | Generated devenv process profiles plus process-compose; Rust is only the typed frontend |
| `west` | Nix-generated west input plus Rust materialization adapter plus native west |
| `graph` | Render the normalized Nix graph; no runtime jq traversal |
| `banner` | Rust presentation or remove entirely; never a shell-entry side effect |

## Hidden shell in Nix strings

Tiny Nix builders that copy or symlink a known store file are not workspace
orchestration. Multi-step command programs, graph/cache logic, runtime
preflights, loops, mutation, and nested `bash -c` are. The direct cutover
deleted the old component, task, launch, completion, and script control planes.
The remaining production-Nix boundary is:

| Nix location | Current hidden shell | Disposition |
| --- | --- | --- |
| `nix/cognipilot/devenv-task-generator.nix` | One escaped invocation of the Nix-built typed action runner | Accepted process boundary; task policy and DAG semantics remain Nix data |
| `nix/cognipilot/devenv-launch-renderer.nix` | Generated argv-safe `exec` adapters | Accepted process-compose boundary; probing, ports, and session state use the typed Rust client rather than shell logic |
| `nix/cognipilot/product-flake-module.nix` (`promotionRecordPackage.buildCommand`, `promotionAttestationPackage.buildCommand`) | One `_materialize-closure` invocation per immutable document | Accepted builder boundary; Nix supplies each versioned seed and `exportReferencesGraph`, while generic Rust validates, proves, sorts, and atomically serializes it; the SPDX document is a pure `writeTextFile` projection |
| `nix/cognipilot/product-flake-module.nix` (`showIndex`, contract check) | One `cat` adapter and a tiny store-file check/link builder | Acceptable generated Nix adapters, though `show-index` may call the Rust client for uniform output; no workspace orchestration is present |
| `nix/cognipilot/project-flake-module.nix` (`showIndex`, contract check) | One `cat` adapter and a tiny store-file check/link builder | Acceptable generated Nix adapters |
| `nix/cognipilot/compliance-flake-module.nix` (check builder) | Copy a Nix-generated report to `$out` | Acceptable tiny Nix builder |

Workflow YAML only wires pinned Actions and invokes named Nix outputs. The
retired container-release workflow and its embedded shell control plane were
deleted; public cache publication realizes exactly `public-cache-root` and
pushes that explicit result link.

## ROS 2/colcon/ament boundary

The deleted `scripts/build-csyn-ros2` created a second source tree, copied two
ROS packages, linked a third-party schema tree, rewrote `Cargo.toml`, chose
colcon base paths and package closure, destroyed build/install/log directories,
ran tests, and interpreted results. That duplicated or bypassed established
ecosystem behavior:

- colcon already discovers packages, computes their dependency order, selects
  `--packages-up-to`, owns build/install/log bases, and dispatches build/test;
- ament owns package metadata, install prefixes, resource indexes, and
  environment hooks;
- Cargo owns Rust dependency resolution and supports declared patch/path source
  configuration without mutating a checked-in manifest copy;
- rosdep/Nix project shells own system dependency provisioning.

The replacement contract is therefore:

1. The `csyn_ros2_bridge` project flake exports its package metadata, dev shell,
   and direct colcon action argv. Local schema bindings are declared as a typed
   action input/variant or project-native Cargo configuration.
2. Nix validates the cross-project artifact edge and generates the task order.
3. The generated task invokes `colcon build`/`colcon test` directly with declared
   base/build/install/log paths and package selection.
4. Rust may select and invoke that generated task and present results. It does
   not copy packages, parse `package.xml`, topologically order ROS packages,
   synthesize an ament overlay, or rewrite Cargo manifests.

## Completed cutover order and acceptance conditions

1. Make the normalized Nix project/product index the only semantic input.
2. Generate editable tasks, launch processes, source plans, action plans, west
   inputs, and completions from that index.
3. Land the independently locked Rust client and reduce root `ws` to its exec
   shim.
4. Land the Nix-built setup command, move cache declarations into flake
   `nixConfig`, and reduce root `setup` to bootstrap/exec only.
5. Replace each legacy call site, then delete all scripts marked delete above,
   legacy tests, `nix/components/default.nix`, `nix/tasks.nix`, and the legacy
   launch registry in the same change. No compatibility aliases remain.
6. Convert CI to named Nix outputs and typed commands, then remove inline
   workflow programs.

The control-plane boundary is complete because all of these are true:

- `git ls-files` contains no executable workspace shell other than `setup` and
  `ws`; `.envrc` contains only the conventional direnv declaration.
- Searching production Nix finds no multiline orchestration programs; remaining
  builder snippets only materialize already-computed store data or invoke one
  typed/native command.
- Every project action shown by `ws` is traceable to the normalized Nix index
  and then to a generated devenv task or immutable flake package.
- Rust contains no dependency-graph discovery/planning and no implementation of
  west, colcon/ament, CMake, Cargo, npm, Twister, or Git semantics.
- ROS builds leave package discovery, ordering, install overlays, hooks, and test
  result interpretation to colcon/ament.
- Immutable release packages, when a project declares them, and the current
  qualification/interface closure are Nix store outputs reachable from
  explicit product/public cache roots. Promotion of real deployable project
  outputs remains a release-roadmap item, not an alternate control plane.
