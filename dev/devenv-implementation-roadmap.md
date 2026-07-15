# CogniPilot development-environment implementation roadmap

Status: active implementation roadmap, updated 2026-07-14.

This roadmap turns `dev/adversarial-devenv-direction-review.md` into an ordered
implementation program for this workspace. It deliberately separates
correctness repairs, control-plane work, package migration, launch ergonomics,
and release work so the current fast build paths remain usable throughout.

## Target outcome

The completed workspace provides:

- typed project flake modules with arbitrary source layouts and in-tree,
  external, or downstream-fork ownership;
- standardized, lazily evaluated west/devenv/flake integration boundaries;
- a fast cached package/resource index that does not re-evaluate project flakes
  for interactive queries;
- target-, variant-, action-, and artifact-qualified DAGs;
- native incremental builds with typed artifact proof and provenance;
- safe local package overrides without a sourced setup script;
- package-owned declarative launches with typed runtime parameters;
- explicit cross-package wiring and root-owned product bundles;
- preflight, named launch sessions, multiple instances, logs, and recovery;
- a checked-in `./ws` entry point that can diagnose setup before shell entry;
- immutable, product-locked Nix release outputs isolated from editable native
  build state;
- the useful ROS/colcon conveniences without imposing ROS layouts on non-ROS
  repositories.

Success is measured by developer workflows and correctness, not merely by the
presence of flake files.

## Current implementation status

The Nix-first control-plane foundation and atomic workspace cutover are
complete:

- typed project modules, provider selection, presets, normalized indexes,
  generated devenv tasks, and the public cache root are composed by Nix;
- the independently locked Rust `nixspace` client provides the generic typed
  CLI, host setup/doctor, source operations, plans, builds/tests, and named
  process-compose sessions;
- package definitions use one selected in-tree, external, or fork authority;
  the replaced central task, launch, script, and completion implementations
  have been deleted;
- the typed `mocap` launch and root `simulation-stack` provide explicit
  process/artifact wiring; and
- editable task caching uses devenv's declared-input/output protocol while
  immutable public Nix closures use the explicit `public-cache-root`.

The typed local/locked overlay, atomic generation reader/writer protocol,
multi-instance launch edge cases, and a source-absent pure release fixture are
complete. `synapse_ppm_bridge` is the first real locked deployable output. The
full native performance matrix, protected-cache publication, and second-host
substitution remain evidence gates.
At the user's direction, the cold QEMU and complete post-cutover warm-build
matrix are explicitly deferred; no passing result is inferred for an unrun
build. The workspace test control plane is now Nix-native: four independently
diagnosable flake-check reports cover 19 golden modules, 74 fail-closed module
fixtures, exact preset/task payloads, generic/provider interfaces, host policy,
west, launch rendering, source plans, bootstrap and GitHub workflow policy,
standalone external definitions, and realized release/promotion isolation.
Pure contracts evaluate for every supported flake system; Devenv's
task executable is bound explicitly from its own pinned, upstream-cache-aligned
flake output, so the complete three-system flake evaluates with
import-from-derivation disabled.
There is no Python test runner or nested private Nix store. The separate Rust
package passes all 148 tests with its locked dependency graph, passes clippy
with warnings denied, and packages as one standalone Cargo crate. The
Nix-emitted lightweight matrix
passes all eight default cases plus both explicit evaluator cases (20/20
p50/p95 gates); the strict lifecycle matrix adds four passing cases and eight
passing p50/p95 gates. Reports and per-command logs are retained under
`dev/benchmarks/nixspace/`.

## Non-goals

- Converting all repositories to ROS 2, ament, or colcon.
- Requiring a shared `src/`, `include/`, `launch/`, or test layout inside
  package repositories.
- Replacing Cargo, npm, CMake/Ninja, west, Twister, Modelica tooling, or Nix.
- Writing a new process supervisor.
- Running CI, release packaging, exhaustive tests, or network synchronization
  from `ws build` or `ws launch`.
- Treating editable build trees or convenience result links as deployable
  artifacts.
- Supporting every ROS launch syntax or event before CogniPilot has a concrete
  use case.
- Adopting Bazel or remote execution during this program.

## Target architecture

```text
root/product flake inputs      project or external flakes
 source/trust/products         flakeModules.default
           \                         /
            \                       /
       typed flake-parts module composition
                       │
       ┌───────────────┼──────────────────┐
       │               │                  │
 normalized cached  packages/checks  devenv tasks/processes
   JSON index       pure build DAG     local action DAG
       │                                  │
 typed index/plan client         native incremental tools
       │                                  │
 named sessions                  validated artifact proofs

committed product flake.lock → packages.workspace → deployable store outputs
```

The typed flake module is authoritative for discovery, graph semantics,
artifacts, parameters, and policy-visible declarations. The selected in-tree,
external, or fork-owned project flake delegates execution to west, devenv
tasks, native tools, and conventional flake outputs. Its normalized index is
cached so interactive queries do not repeatedly evaluate the graph.

## Sequencing and dependencies

```text
Phase 0: stabilize correctness and current behavior
    │
    ├── Phase 1: flake module schema, index, and bootstrap CLI
    │       │
    │       ├── Phase 2: artifact-qualified build/test DAG
    │       │       │
    │       │       ├── Phase 3: launch IR and named sessions
    │       │       └── Phase 4: local/locked overlay and resource index
    │       │
    │       └────────── Phase 5: product lock and pure release graph
    │
    └── Continuous: benchmarks, atomic migration, documentation

Phases 3, 4, and 5 may proceed in parallel after their Phase 1/2 data contracts
are stable. The workspace-definition cutover proceeded after the semantic
audits passed; deferred performance and release gates remain explicit work.
```

## Authority and replacement rules

Every package has exactly one execution authority. The atomic workspace
cutover installed the selected project modules and deleted the central task and
launch implementations. There is no dual-authority mode, compatibility alias,
runtime fallback, or rollback switch. A bad change is reverted through version
control.

The integration authority selects exactly one complete definition
origin: `in-tree`, `external`, or `fork`. Source and definition revisions are
explicit catalog coordinates. Earlier selection wins as a unit, following the
downstream west-manifest override model; missing fields are never inherited
from another origin.

Cutover procedure for each package:

1. add and validate its project flake module;
2. compare the current and proposed normalized plans in an isolated test;
3. run build/test/launch and performance equivalence tests;
4. replace its central definition and delete the replaced commands, paths, and
   aliases in the same change;
5. make only the project module authoritative.

## Phase 0: stabilize correctness before adding abstractions

Goal: establish one evaluable, tested task graph and correct the known cache,
artifact, state, and release terminology problems.

### 0.1 Resolve `nix/tasks.nix` intentionally

Current status: the conflict was resolved in favor of explicit artifact build
ordering, command state overrides, and the native hot path. The temporary
workspace cache was subsequently replaced by generated devenv task caching and
typed Nix artifact results. A clean warm-budget run against the current set of
checked-out revisions remains deliberately deferred.

Resolution requirements:

- [x] Preserve separate source-selection dependencies and actual artifact
      build-order dependencies.
- [x] Use `buildDependencies`, not the broader source dependency list, for
      local task ordering.
- [x] Preserve the release environment wrapper until the pure release graph
      replaces shell qualification tasks.
- [x] Preserve state-root overrides used by isolated benchmarks/tests.
- [x] Retain the measured native CMake/Ninja hot path.
- [x] Treat the existing custom cache as a temporary migration layer, not
      the final artifact model.
- [x] Add a regression fixture proving the selected resolution's dependency
      ordering and state behavior before removing conflict markers.
- [x] Run the complete current unit-test suite cleanly after the final
      correction. Static producer semantics are checked directly by Nix: the
      aggregate links four domain reports, including 19 golden and 74 invalid
      module fixtures. Executable client semantics remain in the independently
      locked Rust package's 148 tests. The replaced Python harness and its
      nested evaluator store were deleted in the same cutover.
- [x] Replace the orphaned legacy warm-budget file with Nix-emitted benchmark
      cases. The selected `x86_64-linux` BenchmarkPlan now owns all thirteen
      non-QEMU package budgets and exact typed build commands with no
      per-package runner. The one remaining execution result is tracked once
      at the Phase 2 pilot/matrix gate below. Existing evidence remains
      `synapse_fbs` at 0.61-0.62s and `synapse_ppm_bridge` at 1.34-1.35s; no
      complete result is inferred.

The conflict should be resolved as its own reviewable change. Do not combine it
with the new schema or CLI.

### 0.2 Fix known artifact-cache correctness holes

Files initially in scope:

- `scripts/workspace-task-cache`
- `nix/components/default.nix`
- `launch/simulation.nix`
- `tests/test_workspace_task_cache.py`
- `tests/test_devenv_hot_path.py`

These temporary implementations and their replaced tests were deleted during
the atomic cutover. Generated tasks now use devenv `execIfModified` and its
task-output file protocol; `nixspace` validates typed outputs/NAR proofs and
publishes task results atomically.

Work:

- [x] Record and validate output kind: regular file, executable, directory,
      or symlink.
- [x] Record regular-file mode and symlink target.
- [x] Add a content proof for files and a deterministic manifest/proof for
      exported trees.
- [x] Reject truncated, modified, retargeted, or permission-changed outputs.
- [x] Make cache publication atomic and impossible after interruption or
      partial failure.
- [x] Lock the target coordinate during check/build/record so two terminals do
      not mutate the same build tree concurrently.
- [x] Add `electrode-fake-sim` to the outputs produced/validated by the build
      that launch recommends.
- [x] Strengthen generated Synapse output validation beyond a few sentinels.
- [x] Correct the Modelica/Rumoca invalidation gap until artifact-level edges
      replace repository-level dependencies.
- [x] Prove launch's suggested repair command recreates every missing required
      artifact.

Required adversarial tests:

- output deleted;
- output bytes changed;
- executable bit removed;
- file changed to directory or symlink;
- symlink target changed;
- one generated file removed from an exported tree;
- build interrupted before record;
- concurrent request for one target;
- unrelated launch-only edit does not invalidate a build.

### 0.3 Remove ambient release mode

- [x] Make normal `ws build` and `ws test` unambiguously editable/local.
- [x] Remove persistent `ws mode` and the legacy `ws release` command surface;
      immutable release and promotion work is exposed as explicit flake
      packages/checks instead of a mutable CLI mode.
- [x] Snapshot each selected Nix-generated action plan at command start so
      another terminal cannot change an in-flight invocation.
- [x] Reserve `product` for root-owned immutable selection/composition; do not
      conflate it with devenv profiles or launch sessions.

### 0.4 Freeze expanded baselines

- [x] Retain current per-target workspace-cold and warm measurements.
- [x] Rename the current “cold” category to `workspace-cold`, since shared Nix,
      language, and download caches remain populated.
- [x] Add measurements for shell evaluation, `ws` dispatch, graph/list output,
      one implementation-file edit, one interface/schema edit, variant switch,
      and launch plan/start/readiness. The strict BenchmarkPlan v3 lifecycle
      adds one-time setup/teardown and per-sample preparation/cleanup around
      exact measured command arrays. The retained lifecycle report records a
      dependency-free Cargo implementation edit at 225.007ms p50 / 226.218ms
      p95, an interface-major edit at 329.831ms / 337.393ms, a Zephyr board
      variant switch at 329.782ms / 333.887ms, and generated process-compose
      start/readiness at 172.664ms / 175.682ms. The earlier dispatch report
      records 7.533ms / 8.674ms without starting devenv or a native task.
- [x] Record p50 and p95 over repeated runs on the named reference host. The
      2026-07-14 `storm` run (AMD Ryzen 9 5950X, measured Nix 2.34.8,
      plan evaluator Nix 2.34.7, `x86_64-linux`) records seven measured samples
      after one warmup for each ordinary case; the evaluator-cold case has no
      warmup by definition.
- [x] Preserve logs and exact graph/cache-hit explanations. The versioned
      report at `dev/benchmarks/nixspace/20260714-final-v3.json` retains exact
      argv, lock hash, system, Nix version, declared warm-cache state, p50/p95,
      gates, and every stdout/stderr log path. The complementary strict-v3
      lifecycle evidence and logs are retained at
      `dev/benchmarks/nixspace/20260714T195127Z-lifecycle-v3.json`.

### Phase 0 exit gate

- [x] No conflict markers remain.
- [x] The current test suite has one clean post-correction full run.
- [x] Carry the deferred native performance result forward as one canonical
      Phase 2 gate instead of maintaining a duplicate Phase 0 checkbox. The
      Nix plan and retained pilot evidence are present; cold QEMU remains
      explicitly outside ordinary verification.
- [x] Tampering with any declared launch/build artifact causes a cache miss or
      explicit invalid-artifact error.
- [x] The fake-simulator repair loop is correct.
- [x] Release mode cannot change silently between terminals.

Sequencing decision (2026-07-14): the Phase 1 schema, typed client, artifact
proofs, and launch parity were completed before the expensive full-workspace
warm gate. Once the static/native equivalence audit reached zero editable-task
blockers, the generated graph was made authoritative and the replaced control
plane was deleted atomically. The deferred QEMU/warm matrix remains an explicit
performance-validation gap, not a compatibility gate or a claimed pass.

## Phase 1: typed flake control plane and bootstrap CLI

Goal: compose selected project flakes through typed reusable modules, then make
their normalized data discoverable, validatable, and fast from a cached index.

### 1.1 Freeze terminology

The normative glossary is `dev/workspace-contract-glossary.md`; use it
consistently in schema, CLI, docs, and errors:

- repository: acquisition/source-control unit;
- package: independently selectable logical ownership/version unit;
- target: independently buildable output family;
- variant: board/profile/feature/platform choice for a target;
- action: build, generate, test, or other DAG operation;
- artifact/export: typed result consumed by another action or launch;
- resource: named configuration/data/model/plugin/executable lookup entry;
- product/selection: root-owned package/repository set;
- launch: reusable process description;
- process: supervised executable within a launch;
- session: one running, resolved launch instance;
- qualification output: result proving behavior but not deployable;
- deployable output: immutable Nix product artifact.

### 1.2 Prove the flake composition choice

The ecosystem review selects Nix flakes, flake-parts, and devenv tasks as the
default stack. Validate that choice with representative data before making its
module schema authoritative:

- [x] Survey flakes, flake-parts, devenv composition/tasks, Nix Standard,
      Blueprint, `zephyr-nix`, and `west2nix` against workspace requirements.
- [x] Select flake-parts as the minimal distributed composition layer and
      devenv tasks as the existing mutable action DAG.
- [x] Express `synapse_fbs`, `cerebri_cubs2`, and `electrode_web` through typed
      flake-parts modules, including variants, artifact edges, and a launch.
- [x] Prove a project can export and consume `flakeModules.default` both
      standalone and from the root product flake.
- [x] Prove an external flake can wrap an unmodified `flake = false` source and
      a downstream fork can replace it as one complete authority.
- [x] Compare the same fixture with Nix Standard Cells/Blocks. The pinned
      executable comparison rejected its second discovery hierarchy and
      incompatible extra nixpkgs universe.
- [x] Complete a build-free `zephyr-nix`/`west2nix` interface spike against
      west imports, module discovery, fork precedence, native_sim, and offline
      materialization semantics. Cache-backed firmware realization remains an
      explicit release/performance gate.
- [x] Measure cold selected-flake evaluation and cached interactive queries.
      On `storm`, seven evaluator-cold samples with `--no-eval-cache` fully
      serialized and hashed the selected index at 250.373ms p50 / 251.715ms
      p95. Immutable flake inputs and the Nix store remained warm, and no
      derivation was realized. The same BenchmarkReport v1 generated from a
      BenchmarkPlan v2 records the generated-index query and 100-project
      serialization cases separately.

### 1.3 Define the CogniPilot flake module v1

CogniPilot publishes one flake-parts module that declares typed options under
`cognipilot.projects`. A selected project flake exports one complete definition
as `flakeModules.default`. The flake may be in the source repository, in a
separate integration directory/repository, or in a downstream fork; an external
definition takes its source as an explicit flake input. Source and definition
locations remain independent.

Schema areas:

- [x] interface version, repository ID, package IDs, and safe project roots;
- [x] globally unique aliases and lifecycle;
- [x] package software-version source separate from schema version;
- [x] source input identity, one complete definition origin, and versioned
      native adapter preset;
- [x] targets, variant dimensions/defaults/constraints, and actions;
- [x] explicit source coordinates and typed artifact inputs;
- [x] typed artifact outputs and compatibility/interface versions;
- [x] action CPU/memory/exclusive lock requirements;
- [x] named resources and executable exports;
- [x] launch index, parameters, artifacts, capabilities, and includes;
- [x] immutable release-output reference;
- [x] deployability class plus owner/SPDX-license fields, with missing
      owner/license initially warning-level during migration.

Nix module assertions and structural checks must reject:

- duplicate or invalid package/target/artifact/resource/launch IDs;
- package roots or referenced paths escaping through `..` or symlinks;
- overlapping writable output ownership;
- unresolved dependency/artifact/include references;
- cycles in each applicable DAG;
- invalid defaults, variants, parameter types, and include forwarding;
- process/resource name collisions;
- unqualified shell interpolation where argv is required;
- deployable outputs that reference mutable paths or impure adapters without
  an approved exception.

The project flake must expose conventional `packages`, `checks`, `devShells`,
apps, and formatters. Its exported module maps those outputs and declares the
devenv tasks from the same project definition, plus one JSON-safe normalized
index. Do not maintain separate handwritten metadata for local and pure DAGs.

Package-authoring budget:

- [x] Ship versioned Cargo, npm, CMake preset, west/Zephyr, and Twister presets
      that generate native action tasks, standard output mappings, contract
      checks, state paths, and normalized index data.
- [x] Define versioned Cargo, npm, CMake, west, and Twister schema presets that
      generate normalized default targets/actions without per-package command,
      cache, task, or state plumbing.
- [x] Keep organization cache, system, toolchain, compliance, and product-root
      policy in shared root modules, never repeated in package definitions.
- [x] Make each preset's golden minimum fixture require only one module import,
      project identity/source, and the preset selection, with no handwritten
      action command or generic build/test/check wrapper. The CMake fixture and
      Twister build/test expectation pass in the focused schema suite.
- [x] Reject generic cache/task/state plumbing and limit the current schema to
      targets, artifacts, variants, resources, executable exports, and genuine
      custom-action exceptions.
- [x] Extend the semantic-delta surface to launches without introducing generic
      command wrappers.
- [x] Normalize equivalent in-tree and external-wrapper definitions to the
      same project index so definition location adds no schema boilerplate.
- [x] Count every custom action command as a bespoke adapter.
- [x] Promote repeated exceptions into shared preset capabilities before
      another package copies them. The final catalog audit found no duplicated
      generic task/cache/state plumbing; the metadata test rejects redundant
      conventional source, repository, and definition fields.

This behavioral budget is enforced with golden fixtures rather than a line
count: terse Nix can still duplicate policy, while a larger target/launch data
set may be irreducibly meaningful.

### 1.4 Standardize project integration providers

Use `dev/devenv-flake-provider-contract.md` as the initial contract and audit.
Make a flake the integration boundary, reuse west for Zephyr discovery,
devenv modules/tasks for mutable actions, and conventional flake outputs for
reusable Nix and release products. A source repository need not carry a flake
when a pinned external integration flake wraps it.

- [x] Define independent pinned source and integration-definition coordinates.
- [x] Support exactly one complete `in-tree`, `external`, or `fork` definition
      without field-level merging or fallback.
- [x] Add a locked product-flake composition proof that imports only its
      selected project `flakeModules.default` definitions, including separate
      external source/definition inputs. The first real product closure is the
      locked `synapse_ppm_bridge` release.
- [x] Expose `packages.<system>.workspace` as the immutable selected-workspace
      derivation root.
- [x] Add named deployable product packages when projects expose natural pure
      release outputs. `synapse_ppm_bridge` uses `buildRustPackage` in its
      selected definition, exports `target-synapse_ppm_bridge--default` on all
      three systems, and was built through the root as the same derivation as
      its provider package. The root intentionally has no arbitrary
      `packages.default` selection.
- [x] Preserve west manifests, `zephyr/module.yml`, CMake, and Twister as the
      authority for Zephyr-specific project/module behavior.
- [x] Replace the global west union with content-addressed workspaces for the
      selected product manifest closure; seed new closures through west's
      native `--path-cache` and keep local override views product-specific.
- [x] Reuse `zephyr-nix`/`west2nix` only if the compatibility spike passes. It
      did not: it could not represent the selected downstream manifest/module
      precedence without a second dependency universe, so native west plus the
      Nix-emitted `WestWorkspace` plan remains authoritative.
- [x] Map non-Zephyr actions to explicit devenv tasks using its JSON task input
      and task output file rather than a custom provider executable.
- [x] Bind local worktree sources in the snapshotted task plan, not as path
      flake inputs copied into the store on the incremental hot path.
- [x] Publish shared devenv adapters for common Cargo, npm, CMake preset, west,
      and Twister entry points without replacing project-owned commands.
- [x] Keep cache configuration, task JSON plumbing, standard contract checks,
      and state-root wiring out of every per-project module; presets and the
      root product module generate them once.
- [x] Export project definitions through `flakeModules.default`; do not require
      an in-tree flake when an external definition is the cleaner authority.
- [x] Require a committed lock for every selected integration/product flake.
- [x] Make `packages.default` mean one explicitly selected natural deployable
      product, never CI; omit it when no such target is selected.
- [x] Keep qualification out of deployable product packages. Qualification is
      an explicit devenv action or flake check; only targets with a declared
      immutable release output enter `packages.workspace` and promotion.
- [x] Define command-scoped local input overrides separately from locked
      package and product release inputs.
- [x] Generate `checks.<system>.nixspace-interface` for standalone project
      evaluation/structure and a product-level cross-project contract check.
- [x] Add isolated behavioral fixtures for task JSON I/O, declared writes,
      interruption, concurrency, and artifact proofs.
- [x] Expose an explicit `public-cache-root` package that references the
      selected public immutable inputs and outputs; do not discover cache roots
      by scanning arbitrary flake attributes in CI.
- [x] Reference useful native project apps/tasks from presets and delete the
      replaced central workspace entries at package cutover.
- [x] Add constrained-integration policy for ROS trust/toolchain coupling and
      externally owned packages such as FastDyn.

The typed project module remains the authority for action/artifact semantics.
Root evaluation is limited to the selected product closure, and its normalized
result is cached for interactive use.

### 1.5 Build the reusable `nixspace` client

Add a publishable, independently locked Rust package for interacting with a
versioned Nix workspace interface. The crate and binary are both named
`nixspace`; it must install with `cargo install nixspace`. CogniPilot's `ws`
name is only a root convenience alias. The client consumes the JSON-safe result
of flake module evaluation; it does not parse Nix, scan for packages, or invent
a second project configuration language.

Responsibilities:

- invoke the configured flake index output exactly once when explicitly
  refreshing, or consume the cached normalized result for interactive reads;
- preserve Nix command/module assertion context in diagnostics;
- reject unsupported interface versions and malformed transport data without
  reimplementing Nix-owned schema, graph, or policy validation;
- atomically cache the exact Nix-generated bytes;
- list/show packages, targets, artifacts, resources, launches, and parameters;
- display Nix-precomputed dependency, reverse-dependency, launch, and action
  plans in human, JSON, and Graphviz-ready views;
- never build package outputs or execute project commands while indexing.

Packaging and reuse gate:

- no path or Git dependencies, no workspace project crates, and no membership
  in a project Cargo workspace;
- no CogniPilot package names, environment-variable protocol, state paths, or
  flake output names in Rust;
- explicit complete opaque Nix installable, generated-file path, and state
  directory inputs;
- conventional Cargo metadata, README, license, package test, and non-CogniPilot
  fixtures proving third-party reuse;
- Nix and Cargo both build the same independently locked package.

Implemented read-only slice:

- [x] Consume an explicit or cached normalized index without evaluating Nix;
      Nix owns target-qualified action and artifact DAG validation.
- [x] Provide package list/show and declared action/artifact graph queries in
      human and JSON forms, with actionable missing/stale/corrupt diagnostics.
- [x] Add target/artifact list/show queries over the cached normalized index.
- [x] Add recursive reverse-dependency and Graphviz graph queries.
- [x] Add resource, launch, parameter, and executable plan queries over the v1
      interface.
- [x] Implement one-build, exact-byte, atomic index refresh and filesystem-only
      status/path/completion behavior.
- [x] Complete the `nixspace` rename, publishable Cargo package metadata, and
      non-CogniPilot reuse fixture.
- [ ] Publish the initial `nixspace` crate release and verify the registry
      command `cargo install nixspace`. The independently locked crate already
      packages without path/Git dependencies and installs from this checkout
      with `cargo install --locked --path tools/nixspace`; crates.io ownership
      and publication are external release evidence, not inferred locally.

Proposed new files/directories after the composition spike:

```text
nix/cognipilot/flake-module.nix
flake.nix
tools/nixspace/
tests/fixtures/project-flakes/
tests/fixtures/nixspace-generic/
```

### 1.6 Add a checked-in `./ws` bootstrap frontend

Required behavior outside an activated shell:

```sh
./setup
./ws doctor
./ws package list
./ws build --plan cerebri_cubs2
```

- [x] `./ws doctor` performs host-level checks without first requiring a valid
      package/devenv evaluation.
- [x] The root wrapper invokes the cached Nix-built client directly and never
      nests or implicitly enters a development shell.
- [x] Flake/module evaluation errors and merge conflicts remain diagnosable.
- [x] Root `./ws` routes package, target, artifact, and graph queries directly
      to the read-only cached-index core without entering devenv.
- [x] Bootstrap completion reads cached normalized JSON for package, target,
      artifact, and graph coordinates and never evaluates Nix.
- [x] `--json` is supported for doctor, package queries, and plans.

Doctor checks:

- [x] host/platform support;
- [x] Nix executable, mode, minimum version, and required features;
- [x] pinned devenv availability/version;
- [x] substituter/key/trusted-user configuration without changing it silently;
- [x] disk-space policy and daemon liveness;
- [x] unresolved Git conflicts and missing selected repositories;
- [x] generated index/plan offline readiness;
- [x] active launch sessions and occupied declared ports.

The bootstrap frontend and host-plan doctor slice are implemented. The checked-
in shims only bootstrap/exec; Rust reads the Nix-emitted host expectations and
reports Nix version/features, pinned devenv, cache trust, configuration mode,
disk/daemon health, repository/index readiness, and active-session/port
conflicts in human or JSON form. It never evaluates or builds a package during
host diagnosis.

### 1.7 Validate direct package cutover

- [x] Generate the proposed normalized package view in isolated fixtures.
- [x] Compare repository ID, package ID, source/build edges, commands, outputs,
      toolchain provider, and launch requirements before cutover.
- [x] Report every disagreement; do not fill missing data from the current
      central definition.
- [x] Cut over by replacing and deleting the central definitions in one
      change; no authority switch remains.

Completed status (2026-07-14): both the fast-pilot and complete catalog audits
reached zero editable-task blockers. Shared Nix tool profiles supply the CMake,
ZROS formatting, PPM bridge, and FastDyn environments; RDD2 consumes declared
Rumoca and Synapse artifacts; and FastDyn's exact setup/QEMU actions are
project-defined. Modelica's Docker/`omc` requirement is an explicit host
precondition, `zros_drivers` is intentionally resource-only, and FastDyn's
private opt-in and lowercase ID are accepted declarations rather than hidden
fallbacks.

The generated devenv graph is now the sole editable execution authority. The
old task/launch/script/completion implementations, audit switch, catalogs, and
replaced tests were deleted in the same cutover, and the workspace-policy check
rejects their reintroduction. Immutable Rumoca, Twister, and Qualisys outputs
remain release gaps; they do not reintroduce a local execution authority.

### Phase 1 exit gate

- [x] Project flakes export the typed v1 module and compose into the root flake.
- [x] Module schema v1 has valid and invalid golden fixtures.
- [x] Package list/show/graph/launch-show run from the cached index without Nix.
- [x] One unselected malformed/absent optional project does not block shell entry
      or unrelated package queries/builds.
- [x] Warm help/list/completion backend is within 100 ms p95. The retained
      seven-sample run measures 2.3--5.5 ms p95 across those cases.
- [x] A 100-project synthetic module/index rebuild is within 1 second p95. The
      fully serialized index measures 271.9 ms p95; one test also fully
      serializes 1/15/50/100/200-project fixtures.
- [x] The authority change was atomic after semantic audits reached zero
      blockers; no alternate authority remains.

## Phase 2: artifact-qualified build and test DAG

Goal: prove the package model on hard package shapes while retaining native
incremental build behavior.

### 2.1 Define coordinates and variants

Canonical internal identity:

```text
package + target + variant set + mode + action + dependency resolution
```

The exact display syntax is a CLI design detail, but it must serialize stably
and appear in build directories, cache/artifact manifests, locks, logs, and
launch requirements.

- [x] Define default target/variant behavior for convenient package-only
      commands. Package-only actions select the declared `default` target;
      explicit target records carry their finite variant defaults.
- [x] Reject unknown or incompatible variant combinations before execution.
      Nix validates dimension values, defaults, and complete allowed
      combinations before emitting tasks.
- [x] Keep different boards/profiles/features in non-colliding directories.
      Explicit variant targets have validated distinct literal artifact paths.
- [x] Permit two variants to build concurrently when they do not declare a
      shared lock.

State layout:

```text
.devenv/state/build/<package>/<target>/<variant>/
.devenv/state/devel/<package>/<target>/<variant>/generations/<id>/
.devenv/state/devel/<package>/<target>/<variant>/current
.devenv/state/log/build/<run-id>/<coordinate>/
```

### 2.2 Define action and artifact manifests

Each successful action atomically records:

- coordinate and effective variants/mode;
- normalized action definition and adapter version;
- source-input proof;
- selected toolchain/adapter identity;
- dependency artifact identities/digests;
- output paths, kinds, modes, targets, and content/tree proof;
- start/end/duration/exit status and retained log paths;
- local/locked provenance.

Rules:

- [x] Consumers invalidate from artifacts they consume, not complete dependency
      repositories.
- [x] A docs/test-only producer declaration change does not alter build tasks
      or unrelated consumers; the generated-task fixture compares the complete
      records.
- [x] A byte-identical exported artifact may preserve downstream validity:
      consumers track the declared artifact path/content, not a generation
      timestamp or whole producer repository identity.
- [x] Native build directories remain stable for Cargo/CMake/Ninja
      incrementality. Generations contain proof metadata and point to native
      outputs; they never copy or relocate build trees.
- [x] Published generations appear atomically only after command success,
      output validation, proof generation, immutable manifest installation,
      and devenv result publication. Failure preserves the prior pointer.
- [x] A failed selected local action never activates a previous or locked
      artifact as if it were the requested result.

### 2.3 Select a devenv task composition mode

Use devenv tasks as the execution protocol and prototype these composition
approaches on the simple and Zephyr pilots:

1. import selected integration modules into the root devenv task graph;
2. generate a selected-closure devenv project and invoke it through
   `devenv --from`.

Decision criteria:

- no-op dispatch and selected-project-flake evaluation latency;
- dependency state and output propagation;
- bounded parallelism and resource locks;
- cancellation and log behavior;
- error quality and failure isolation;
- standalone package reuse;
- amount of duplicate scheduler logic.

In both cases, use native tools for work and keep `nixspace` responsible for
static planning, provenance, and artifact proof. A custom scheduler/provider
protocol requires a separate ADR with measured evidence that devenv's task DAG
cannot meet a required semantic; it is not part of the default design.

### 2.4 Add package-oriented DAG UX

```sh
ws build [--plan [--json]] [PACKAGE]
ws test [--plan [--json]] [PACKAGE]
ws env PACKAGE --explain [--json]
```

The earlier colcon-shaped command list is intentionally not implemented.
Nix emits the complete static graph and selected task roots; devenv owns task
dependency modes, cache decisions, execution, TUI/tracing, and concurrency;
native tools retain their own test results. `nixspace` must not grow a second
scheduler, failed-test database, or log aggregation format.

- [x] The static plan shows exact selected task roots and local/locked
      provenance; devenv reports runtime cache/run/failure state through its
      conventional TUI/output and trace interface.
- [x] Each successful artifact generation retains exact argv, cwd, sanitized
      declared environment identity, duration, exit status, output contract,
      and proof. Devenv/native-tool output remains authoritative rather than
      being copied into another workspace log database.
- [x] Cargo, CTest, Twister, and colcon retain their native test results and
      logs; the workspace does not ingest them into a lossy parallel index.
- [x] Exact Nix-declared exclusive locks and per-generation publication locks
      prevent conflicting mutations. CPU/memory declarations remain plan
      metadata for native/devenv job policy rather than a Rust scheduler.

### 2.5 Pilot package sequence

Do not promote only an easy Cargo package. The authority pilots are:

| Order | Package | What it proves |
| ---: | --- | --- |
| 1 | `synapse_ppm_bridge` | Small Cargo external flake and basic project-module authoring |
| 2 | `synapse_fbs` | Generated multi-language targets and exported artifact completeness |
| 3 | `cerebri_modules` | Zephyr module/test behavior and product-locked west resources |
| 4 | `cerebri_cubs2` | Target variants, generated dependencies, guarded CMake/Ninja hot path |
| 5 | `electrode_web` | Cargo/npm monorepo, multiple artifacts, web assets, and launch consumers |

For each pilot:

- [x] project module remains readable and within the complexity budget; golden
      minimum fixtures and the catalog boilerplate audit enforce this while
      allowing irreducible launch/variant data;
- [x] standalone project-flake behavior is tested where supported;
- [x] current and proposed normalized plans agree or differences are intentional;
- [ ] the Nix-emitted native warm matrix passes on the reference host. The plan
      now declares every non-QEMU package row, one warmup, three unchanged
      samples, and its historical p50/p95 budget. Generic Cargo implementation,
      interface-major, Zephyr variant, and launch lifecycle edit paths pass;
      `synapse_fbs` has a real 0.61-0.62s result and `synapse_ppm_bridge` has a
      1.34-1.35s result. The remaining rows have not been run after cutover and
      are not inferred from those pilots. FastDyn's cold QEMU prerequisite is a
      separate explicitly deferred measurement;
- [x] direct cutover deletes the replaced central definition atomically;
- [x] central package-internal commands are removed rather than retained for
      an observation or compatibility period.

### Phase 2 exit gate

- Five pilot shapes are project-module-authoritative.
- Artifact tamper, missing output, source edit, interface edit, dependency
  artifact edit, variant switch, interruption, and concurrency tests pass.
- Two CUBS variants build without path/cache collision.
- Default target commands remain simple.
- Framework overhead before the first native task is below 0.5 seconds p95.
- Existing no-op build budgets regress by no more than 10% without an approved
  and documented tradeoff.
- Fewer than 20% of ordinary pilot actions require bespoke executable adapters;
  otherwise stop and revise the schema.

## Phase 3: declarative launches, parameters, and named sessions

Goal: provide the high-value ROS launch experience through a static,
inspectable contract and the existing process supervisor.

### 3.1 Define launch IR v1

Static launch fields:

- qualified package launch ID and description;
- exact artifact/resource requirements;
- package-launch includes and argument forwarding;
- process groups/namespaces and conditions;
- argv arrays, working directory, and structured environment mapping;
- typed inputs, provided endpoints, and explicit bundle wiring;
- port/device/path/GUI/secret/resource claims;
- readiness probe and timeout;
- dependency state (`started`, `ready`, `succeeded`, `completed`);
- restart/backoff and shutdown/signal escalation policy;
- response to required-process exit, failure, and readiness loss;
- reported URLs/endpoints after allocation.

The launch IR must be plan/renderable from the cached normalized index. Expert
adapter hooks may extend selected behavior but cannot hide required artifacts,
parameters, resources, or processes from the plan.

### 3.2 Implement typed runtime parameters

Initial types:

- string, integer, float, boolean, enum;
- host/IP, URL, port;
- bounded number and duration;
- path with allowed-root/existence/read/write policy;
- list/map only where a concrete launch needs them;
- secret reference;
- auto-allocated resource.

Precedence:

```text
package default < bundle default < parameter file < command line
```

- [x] Unknown values and invalid types fail before process startup.
- [x] Includes explicitly forward/rename parameters.
- [x] Runtime values are written to a redacted session input file and consumed
      by process wrappers.
- [x] Host/port/vehicle/log-level changes perform zero Nix evaluation.
- [x] Commands are argv arrays; shell strings require an explicit reviewed
      escape hatch.
- [x] Secret values never enter Nix, argv, plans, completion, session metadata,
      or framework-controlled logs. Application code remains responsible for
      not printing secrets that it explicitly receives at runtime.

### 3.3 Make bundle wiring explicit

Move away from process-set introspection such as `builtins.hasAttr`.

- [x] Electrode ground station declares its router and HTTP exports.
- [x] Simulation and mocap declare consumed router/endpoints.
- [x] Root `simulation-stack` selects providers and supplies wiring.
- [x] Missing or multiple providers fail at plan time.
- [x] Optional Qualisys behavior is an explicit bundle variant/capability, not
      a default stack that is unusable in the default checkout.

### 3.4 Add preflight

`ws launch plan` and `ws launch up` validate before starting the supervisor:

- repositories and exact required artifact generations;
- artifact type/mode/proof/freshness;
- parameters and includes;
- process/startup/include cycles;
- ports and declared resources;
- secret references without exposing values;
- device/path existence and permissions;
- working directories and executables;
- local/release provenance policy.

Failures are consolidated and include exact remediation. No process starts on
any preflight error.

### 3.5 Add persistent named sessions

Command grammar:

```sh
ws launch list
ws launch show electrode_web/ground-station
ws launch plan simulation-stack --set ground-station.http.port=8791
ws launch up simulation-stack --name sim-a --params scenarios/sim-a.yaml
ws launch status [SESSION]
ws launch logs SESSION/PROCESS --follow
ws launch restart SESSION/PROCESS
ws launch stop SESSION/PROCESS
ws launch start SESSION/PROCESS
ws launch events SESSION
ws launch down SESSION
ws launch prune
```

Each session records the resolved graph, redacted inputs, assigned resources,
artifact/package provenance, commands, supervisor socket, PIDs, readiness,
restarts, exits, and logs.

- [x] Runtime/socket/log/process namespaces are isolated per session.
- [x] Two copies of the same stack run concurrently with Nix-declared automatic
      ports, serialized allocation, and distinct recorded claims.
- [x] A new terminal can list, inspect, attach to, and stop existing sessions.
- [x] Terminal loss does not make detached processes undiscoverable.
- [x] `down` requires the manager to succeed, verifies recorded ports are
      released, and removes session state; failed starts and incomplete
      cleanup preserve no false success.
- [x] Use process-compose/devenv; do not implement supervision in `nixspace`.

### 3.6 Launch migration sequence

1. [x] `electrode_web/ground-station`
2. [x] `electrode_web/simulation`
3. [x] root `simulation-stack` bundle with explicit wiring
4. [x] `synapse_qualisys_bridge/mocap`
5. [x] Keep ROS launch adaptation conditional on a selected project exporting
   a real project-native launch. `csyn_ros2_bridge` currently exports colcon
   build/test behavior but no launch, so the workspace does not fabricate one.

Delete replaced launch aliases at cutover, and do not add the ambiguous
two-token `ws launch PACKAGE LAUNCH` form.

### Phase 3 exit gate

- Package launch list/show/plan requires no Nix evaluation.
- Runtime parameter-only changes require no Nix evaluation.
- Preflight completes within 250 ms p95 on the reference host.
- Dispatch begins within 1 second after successful preflight.
- Two concurrent simulation stacks pass isolation and cleanup tests.
- Port, secret, quoting/injection, include-cycle, wiring, readiness-loss,
  restart, and Ctrl-C tests pass.
- A store-backed launch fixture works with its source checkout absent; the full
  product-release proof remains a Phase 5 gate.

## Phase 4: typed local/locked overlay and resource index

Goal: deliver the ROS-overlay convenience without ambient path ambiguity or
deployment contamination.

### 4.1 Build the package/resource index

Index qualified names for:

- package/version/root/provenance;
- targets, variants, actions, and artifact generations;
- executables;
- configuration, model, schema, and data resources;
- package launches;
- plugins and named capabilities;
- locked release outputs and store-relative resources.

Commands:

```sh
ws resource PACKAGE/NAME
ws run PACKAGE/EXECUTABLE -- ARGS...
ws env PACKAGE --explain
ws package prefix PACKAGE
```

### 4.2 Implement adapter-specific consumption

There is no universal overlay environment. Adapters translate selected
artifact entries into only the consumer's environment/configuration:

- Cargo path/config and generated Rust crate inputs;
- CMake variables/prefixes/package config;
- Python interpreter/module paths;
- npm/package overrides;
- Zephyr module and west paths;
- executable/resource lookup.

Do not globally append every export to `PATH`, `PYTHONPATH`,
`CMAKE_PREFIX_PATH`, or similar variables.

### 4.3 Enforce local override safety

- [x] Selected local packages replace locked exports only for the exact
      command-scoped package plan.
- [x] Local task source paths are command-scoped runtime inputs; only explicit
      pure-build testing uses a temporary flake input override.
- [x] Compatibility/interface constraints are checked before substitution and
      every action binding carries the exact selected artifact contract.
- [x] Compiled non-leaf overrides rebuild the affected reverse closure or are
      refused. The current v1 policy refuses unsafe mixed reverse closures
      before build/test/run/launch rather than inventing implicit rebuilds.
- [x] Locked fallback is allowed only for an explicitly selected locked
      candidate, never after selected local failure, missing state, stale
      identity, or failed proof.
- [x] Every resolution explanation shows `LOCAL dirty`, `LOCAL commit`, or
      `LOCKED` provenance from the Nix-declared inspection.
- [x] `ws env --explain`, prefix/resource/run, build/test authorization, and
      generated launches consume the same concrete resolution document and
      selection root.
- [x] Atomic generation switching plus a shared reader lease prevents readers
      from observing a partial or concurrently replaced publication.

### Phase 4 exit gate

- The default product can mix selected local packages with compatible locked
  packages without ambient-path dependence.
- Generated-interface and compiled non-leaf override tests rebuild/refuse the
  correct reverse closure.
- A locked/store-backed resource fixture still resolves after its source
  checkout is removed; complete product closure is proved in Phase 5.
- No local failure silently falls back.
- Manual native commands through `ws run` receive the same resolution shown
  by `ws env --explain`.

## Phase 5: product lock and pure Nix release graph

Goal: make “fixed package versions” and deployment identity exact.

### 5.1 Define the product flake and promotion record

Make the committed root/product `flake.nix` and `flake.lock` the source and
tool dependency authority. Generate a normalized promotion record containing:

- product/selection ID;
- package IDs and software versions;
- canonical repository URLs and exact commits/tree identities;
- package roots and selected project-module/input identities;
- project-module interface and adapter versions;
- selected targets/variants;
- complete immutable dependency closure;
- top-level and project Nix output identities/NAR hashes;
- target platform/system;
- allowed external-source exceptions.

The promotion record is derived from and checked against the product flake; it
does not become a second editable dependency graph. Local development may use a
different command-scoped source override, but promotion always uses the
committed root lock.

### 5.2 Implement the top-level release graph

- [x] Harden `packages.<system>.workspace` and named product outputs so they
      consume only committed flake inputs, not whichever worktrees exist.
- [x] Require package repositories to expose real Nix derivation outputs before
      they are marked deployable.
- [x] Mark shell Cargo/Twister/QEMU builds as qualification tasks only.
- [x] Forbid mutable source dependencies, `devel/`, workspace build paths, and
      non-Git deployed trees from deployable closures.
- [x] Build/store resources and executable artifacts required by release
      launches. The locked release fixture stores both and resolves them from
      the immutable output.
- [x] Record output/NAR digests, package revisions, builder identity, SBOM, and
      provenance/attestation. Promotion schema v2 emits an SPDX JSON 2.3 SBOM
      and an in-toto Statement v1/SLSA provenance v1 payload from the same
      product graph. ClosureMaterialization v2 binds Nix-proved NAR SHA-256
      values into standard lowercase-hex digest fields during realization; the
      tiny locked release suite proves the record, SBOM, and attestation without
      building firmware or QEMU.
- [x] Pin every third-party GitHub Actions dependency by its full commit SHA;
      the current main-only cache publication uses the same pinned actions as
      read-only pull-request validation.

### 5.3 Publish pure closures through a standard binary cache

Use Cachix initially and keep the implementation compatible with any standard
Nix substituter, including a later self-hosted Attic deployment.

- [x] Wire the public `cognipilot` cache into devenv, installation docs,
      and read-only pull-request CI.
- [x] Store a cache-scoped write token as the protected
      `CACHIX_AUTH_TOKEN` organization secret.
- [x] Make the secret available to all organization repositories under the
      explicit policy that every CogniPilot repository is a trusted public-cache
      publisher.
- [ ] Verify a protected `main` build pushes every system-specific
      `public-cache-root` to the `cognipilot` cache and reports complete
      coverage. At clean commit `da4254e`, the realized `x86_64-linux` root had
      214 paths and 3,643,434,392 NAR bytes. The public union covered 128 paths;
      86 locally built paths were missing, and the `cognipilot` endpoint had
      0/214. Publication is therefore correctly not claimed before the
      protected workflow runs.
- [x] Enforce an explicit public-cache closure boundary from source/output
      visibility; FastDyn's separately locked private source and any private
      output cannot enter `public-cache-root` through links or metadata string
      context. Its integration definition is intentionally committed inside
      the public product source and is therefore public.
- [x] Define the private-cache trigger: provision one organization-level
      private cache, separate credentials, and an explicit retention policy
      before the first private immutable output is published. FastDyn currently
      exports no such output, so creating an empty cache now would add no
      security boundary and one cache per repository is explicitly rejected.
- [x] Configure cache URL/public-key reads for developer hosts through the
      managed setup block; keep private tokens and all write credentials
      outside flakes and repositories. Verified against the effective
      multi-user daemon configuration on 2026-07-14. The URLs and keys are
      effective, but the latest doctor run still observes `trusted-users` as
      `root` rather than the separately declared developer-host policy of `*`;
      that host mutation/restart is not claimed complete by this evidence.
- [x] Advertise only the public cache URL/key from the product flake's
      `nixConfig`, with explicit acceptance and `ws doctor` trust diagnostics.
- [x] Give stable-cache write credentials only to main publication steps;
      the pinned workflow exposes `CACHIX_AUTH_TOKEN` only to a `push` on
      `main`, while pull requests remain credential-free and read-only. All
      three native system rows publish their own closure. Tags remain disabled
      because this repository has no established release-tag convention.
      Protected `main` now requires the strict native check matrix from the
      GitHub Actions app, CODEOWNER approval, resolved conversations, and
      linear history.
- [ ] Build `packages.<system>.workspace`, every promoted product/variant root,
      contract check, and shareable development-tool closure on each
      supported system. The flake and native-runner CI matrix now select exactly
      `x86_64-linux`, `aarch64-linux`, and `aarch64-darwin`, and a static test
      rejects drift. Each row selects its visibility-filtered
      `public-cache-root`, including the public workspace/product, promoted
      release targets, contract checks, generated plans, and Nix-selected
      sccache toolchain without firmware or QEMU. The editable default devenv
      shell requires impure `PWD` by upstream design and is deliberately not
      misrepresented as a cross-host immutable root. The complete local
      `x86_64-linux` root realized successfully at `da4254e`; its clean dry run
      required only 31 small generated metadata/check derivations after the
      Devenv task package was realigned with its upstream binary cache. The
      current contract root is Nix-native and has no Python runtime. The
      independently locked Rust client passes all 148 tests. The two other
      systems and a successful remote three-system realization are not
      claimed.
- [ ] Push complete build/runtime closures and archive public flake input store
      paths. Every native row runs one blocking `cachix push` of its exact
      visibility-filtered root, whose direct public input links retain the
      selected archive paths, then requires complete endpoint coverage. No
      successful main publication run has yet proved publication; the exact
      local coverage report above proves the current revision still needs it.
- [x] Apply bounded retention to published development roots. CI pins
      `main-${system}` with 30 retained revisions; pull requests are read-only
      and create no branch roots. Workflow concurrency cancels an overlapping
      older run so a mutable rolling pin cannot regress after a newer run.
      Release pins remain conditional on adopting a real release-tag convention
      rather than inventing one here.
- [ ] Verify an empty second host substitutes the whole product closure without
      local builds and rejects unsigned/untrusted paths.
- [x] Add cache hit/miss and transferred-byte availability reporting to CI and
      `ws doctor`.
      Host interface v4 now gives `ws doctor` a Nix-owned, credential-free
      union contract across `cache.nixos.org` and `cognipilot`; cache report v2
      keeps per-store diagnostics while completeness requires each path from at
      least one observed store. For an already-realized
      `result-public-cache-root`, it reports exact path hits/misses, NAR bytes,
      and advertised compressed download bytes through structured
      `nix path-info` JSON; it skips absent roots without evaluation or network
      access. Pull requests retain a read-only report; main repeats the report
      after publication, requires complete coverage, writes the GitHub step
      summary, and retains the JSON artifact for 30 days. Protected-main
      governance is enforced; a successful main publication remains an
      independent evidence gate. Nix's stable JSON does not expose bytes
      actually transferred by a prior
      build, so the versioned report records `transferredBytes: null` and the
      text/CI summary says unavailable instead of fabricating a byte count.
- [x] Pilot shared `sccache` for mutable builds instead of pretending non-store
      devenv task outputs are Nix-cacheable. The locked-Cargo preset now uses
      the central `rust-libudev-sccache-v1` profile, Nix-selected
      wrapper, disabled incremental compilation, and workspace-relative shared
      cache with no per-project configuration. A clean local replay with
      sccache 0.16.0 recorded one Rust miss, one subsequent Rust hit, and one
      write. Pinned CI repeats the miss-to-hit proof with the conventional GHA
      backend and retains native JSON statistics; its remote run remains an
      operational evidence gate, not a second implementation.

### 5.4 Prove release isolation

- [x] Place unique dirty sentinels in source, `build/`, and `devel/` in the
      locked release fixture.
- [x] Build the locked fixture product and inspect references, closure, NAR
      hashes/sizes, and output bytes against `nix path-info --recursive`.
- [x] Confirm no sentinel or mutable workspace path is present in the
      materialized promotion record or any regular file in its closure.
- [x] Remove/rename source and definition checkouts and run release prefix,
      resource, launch-plan, and executable lookup through the Nix-built
      client. The destructive fixture succeeds entirely from store outputs.
- [ ] Restore the lock on a second prepared host and reproduce output identity
      or explain any declared non-reproducible boundary.

### Phase 5 exit gate

- One representative product is built exclusively from a committed lock and
  pure Nix derivations.
- Its promotion record contains the complete package/output closure.
- A clean prepared host downloads that closure from the trusted binary cache
  without rebuilding it.
- Local state contamination tests pass.
- Release launches/resources work without development checkouts.
- Qualification outputs are never presented as deployable artifacts.

## Phase 6: workspace-wide migration and governance

Goal: migrate remaining packages without permanent dual truth or regressions.
All listed workspace definitions now use one selected project module. Remaining
migration work is owner adoption, natural release-output qualification, and
moving an external definition in-tree where that improves project ownership.

### Migration waves

| Wave | Packages | Notes |
| --- | --- | --- |
| Pilot | `synapse_ppm_bridge`, `synapse_fbs`, `cerebri_modules`, `cerebri_cubs2`, `electrode_web` | Exercises all difficult contract shapes |
| Default core | `rumoca`, `modelica_models`, `csyn` | Completes normal CUBS2/modelica graph |
| Optional firmware/bridges | `cerebri_rdd2`, `zros`, `zros_drivers`, `qualisys_rust_sdk`, `synapse_qualisys_bridge` | Migrate when repositories are materialized and package PRs can be tested |
| ROS adapter | `csyn_ros2_bridge` | Read/check `package.xml`; do not duplicate ROS metadata |
| External exception | `FastDyn` | External integration flake for development/qualification; remain non-deployable until an approved immutable source/output path exists |

For repositories not currently present, a pinned external integration flake
provides the authoritative definition and expected IDs. It can remain there or
move in-tree later by selecting the repository's exported module as one
complete replacement.

### Governance gates

- [x] Root/product flake inputs own selected source/definition URLs, trust, and
      package namespace mapping.
- [x] Project modules cannot self-select products; standalone source defaults
      are overridden explicitly by the root product input graph.
- [x] The product lock pins every selected external definition and source.
      In-tree or fork definitions may pin and export their module from the
      owner repository when adopted, but source repositories are not required
      to carry a workspace flake solely to duplicate root-owned pins.
- [x] Every enforced public package declares a validated SPDX expression.
      Private FastDyn remains outside public compliance until its owner
      supplies an approved license declaration.
- [x] Lifecycle is one of experimental, stable, deprecated, or retired.
- [x] Namespace, owner, deployability, capability, dependency-scope, and release
      changes receive enforced platform/release review. `.github/CODEOWNERS`
      assigns the relevant Nix/interface/workflow paths to the organization
      admins team. Protected `main` requires one CODEOWNER approval, dismisses
      stale approvals, requires resolved conversations and linear history,
      enforces the strict three-system check matrix from the GitHub Actions app,
      and applies the rules to administrators.
- [x] Bespoke impure adapters require an exact root-approved action coordinate
      before deployment.
- [x] Root and project definitions support one interface major at a time; a
      major upgrade is a coordinated, atomic update with golden fixtures.
- [x] Replaced package definitions and CLI forms are deleted at cutover.
- [x] Dual execution authority and runtime fallback are rejected; shadow audit
      data and authority switches were deleted with the old implementation.

### CI placement

Package repository CI:

- flake module option and semantic validation;
- identity/namespace check;
- path/symlink confinement;
- package/check/task/launch evaluation for that project;
- focused build/test and warm budget;
- secret-redaction checks.

Workspace integration CI:

- exact integration lock resolution;
- package/resource uniqueness and all graph validation;
- cross-repository artifact compatibility;
- launch planning without process startup;
- current/proposed equivalence fixtures before each atomic cutover;
- representative launch smoke tests.

Release CI:

- exact product lock and pure output closure;
- no local fallback/external mutable input;
- SBOM, output hashes, signed provenance, and promotion policy.

None of these exhaustive gates belongs in the `ws build` hot path.

### Phase 6 exit gate

- Every central component is project-module-authoritative or an explicit documented
  external exception.
- Root registry contains acquisition/policy only, not package-internal commands
  or paths.
- Root launch bundles contain composition/wiring only, not leaf process
  implementations.
- Default product build/test/launch/release workflows use the new model.
- No replaced task, launch, alias, or compatibility implementation remains.
- Package authors can add a normal build, test, and parameterized launch without
  editing root implementation code.

## Cross-cutting performance SLOs

Measure p50/p95 on a named reference developer host and a separate CI class.

| Operation | p95 budget |
| --- | ---: |
| `ws help`, package list, launch list, completion backend | 100 ms |
| graph/build/launch plan from cached index | 200 ms |
| warm shell activation | 0.5 s |
| cold Nix/devenv evaluation on a provisioned host | 5 s |
| module/index rebuild for 100 packages | 1 s |
| orchestration before first native task | 0.5 s |
| launch preflight | 250 ms |
| launch dispatch after preflight | 1 s |
| unchanged default-product build graph | 8 s |
| small CUBS2 source edit | 10 s |

Rules:

- ordinary package source edits never invalidate the root shell environment;
- package/list/graph/completion and runtime parameter changes never re-evaluate
  Nix;
- launch metadata changes never invalidate build artifacts;
- existing per-target warm budgets remain enforced;
- package-contract orchestration may not add more than 10% to an existing warm
  target without explicit review;
- machine-cold/network benchmarks are recorded but not gated by one absolute
  noisy sample.

## Required test matrix

### Flake control plane

- valid/invalid project-module fixtures;
- duplicate IDs and unsupported interface versions;
- path traversal and symlink escape;
- missing/cyclic artifact and launch references;
- malformed unselected optional project failure isolation;
- 1/15/50/100/200-package scale fixtures;
- source-located human and JSON diagnostics.

### Artifact DAG

- implementation edit versus docs-only edit;
- public generated interface change;
- dependency artifact changed/unchanged;
- toolchain, adapter, recipe, and lock changes;
- output deletion, tamper, mode/type/symlink change;
- interruption and atomic publication;
- same-coordinate concurrency and different-variant concurrency;
- CPU/memory/exclusive-lock scheduling;
- deterministic devenv task output/trace retention plus Cargo, CTest,
  Twister, colcon, and other native-tool result/log retention without a custom
  workspace aggregation database.

### Launch

- typed/defaulted/unknown parameters;
- include forwarding and precedence;
- argv quoting and injection attempts;
- secret redaction across plans, argv, logs, and state;
- resource and port collision/auto-allocation;
- explicit capability wiring and ambiguity;
- readiness timeout/loss, restart, required-process exit, and shutdown;
- two concurrent sessions, reattachment, terminal loss, and cleanup;
- source-absent release launch.

### Overlay and release

- local leaf override;
- generated-interface and compiled non-leaf override;
- reverse-closure rebuild/refusal;
- failed-local-no-fallback;
- local/locked provenance agreement across plan/env/launch;
- dirty sentinel contamination;
- exact product-lock restoration and output identity.

## Stop/go criteria

Stop and revise the static/lazy design if any of these remain true after the
pilots:

- more than 20% of ordinary package actions require bespoke executable
  adapters;
- a simple project module needs roughly more than 50 lines or a representative
  complex project roughly more than 150 lines without meaningful behavior;
- an invalid unselected project flake blocks unrelated shell entry/builds;
- package list/graph or completion repeatedly evaluates Nix instead of using
  the normalized cache;
- warm build budgets regress by more than 10% due to orchestration;
- runtime launch parameter changes require Nix evaluation;
- the selected execution backend requires duplicating a full task scheduler;
- local override safety cannot be explained and enforced at artifact level;
- release outputs cannot prove exclusion of local state.

If the design fails these gates, simplify the flake module or improve the root
composition rather than forcing boilerplate into every source repository. Do
not move to full ROS 2 or Bazel merely because this particular contract needs
revision.

## Next implementation priorities

1. Run the remaining five-pilot edit/warm rows and the deferred cold QEMU/full
   matrix when multi-minute native builds are acceptable.
2. Commit the cutover, enforce protected-main/CODEOWNERS review, and obtain one
   successful three-system workflow run that publishes, pins, inventories, and
   substitutes every public root with builders disabled.
3. Record second-host output identity from that fresh workflow and investigate
   the repository-level GitHub Actions startup failures if they persist.
4. Add a ROS launch, permanent release-tag pins, or a private cache only when a
   selected project exports the corresponding real launch, tag convention, or
   private immutable output.

## Definition of done

This roadmap is complete when all of the following are true:

- a developer with Nix installed can run `./ws doctor`, set up a product, and
  reach the first package build in at most three workspace commands;
- every supported package is discoverable and its graph/resources/launch
  arguments are inspectable from the normalized cache without repeated Nix
  evaluation;
- builds are artifact/variant correct, retain native incrementality, explain
  cache decisions, and stay within budgets;
- local package resolution is automatic, provenance-visible, and safe for
  reverse dependencies without sourcing a script;
- package launches are parameterized, composable, preflighted, recoverable,
  multi-instance, and package-owned;
- release products use exact locked versions and immutable Nix outputs with no
  path from `devel/` into deployment;
- the central workspace owns policy and product composition, while package
  repositories own their implementation details;
- no exhaustive CI behavior has returned to the developer hot path.
