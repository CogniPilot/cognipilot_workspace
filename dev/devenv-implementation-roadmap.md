# CogniPilot development-environment implementation roadmap

Status: implementation roadmap based on the six-agent adversarial review,
2026-07-13.

This roadmap turns `dev/adversarial-devenv-direction-review.md` into an ordered
implementation program for this workspace. It deliberately separates
correctness repairs, control-plane work, package migration, launch ergonomics,
and release work so the current fast build paths remain usable throughout.

## Target outcome

The completed workspace provides:

- static, package-owned manifests with arbitrary internal repository layouts;
- a fast cached package/resource index that does not evaluate package Nix;
- target-, variant-, action-, and artifact-qualified DAGs;
- native incremental builds with typed artifact proof and provenance;
- safe local package overrides without a sourced setup script;
- package-owned declarative launches with typed runtime parameters;
- explicit cross-package wiring and root-owned product bundles;
- preflight, named launch sessions, multiple instances, logs, and recovery;
- a checked-in `./ws` entry point that can diagnose setup before shell entry;
- immutable, product-locked Nix release outputs isolated from `devel/`;
- the useful ROS/colcon conveniences without imposing ROS layouts on non-ROS
  repositories.

Success is measured by developer workflows and correctness, not merely by the
presence of manifest files.

## Non-goals

- Converting all repositories to ROS 2, ament, or colcon.
- Requiring a shared `src/`, `include/`, `launch/`, or test layout inside
  package repositories.
- Replacing Cargo, npm, CMake/Ninja, west, Twister, Modelica tooling, or Nix.
- Writing a new process supervisor.
- Running CI, release packaging, exhaustive tests, or network synchronization
  from `ws build` or `ws launch`.
- Treating `build/`, `devel/`, or `release-results/` as deployable artifacts.
- Supporting every ROS launch syntax or event before CogniPilot has a concrete
  use case.
- Adopting Bazel or remote execution during this program.

## Architecture to implement

```text
repository catalog           static package manifests
 URLs/trust/products        IDs/targets/artifacts/launches
         \                         /
          \                       /
       format-independent manifest compiler
                       │
             normalized cached JSON index
               │                 │
       package/DAG planner    launch planner
               │                 │
               └────────┬────────┘
                        │ selected closure only
             shared adapter or lazy provider
                        │
        devenv tasks / native tools / process-compose
               │                       │
      artifact generations       named sessions

committed product lock → pure top-level Nix graph → deployable store outputs
```

The static control plane is authoritative for discovery, graph semantics,
artifacts, parameters, and policy-visible declarations. Package-owned Nix,
devenv modules, flakes, and custom executables are trusted implementation
providers loaded only for an explicitly selected closure.

## Sequencing and dependencies

```text
Phase 0: stabilize correctness and current behavior
    │
    ├── Phase 1: static schema, index, and bootstrap CLI
    │       │
    │       ├── Phase 2: artifact-qualified build/test DAG
    │       │       │
    │       │       ├── Phase 3: launch IR and named sessions
    │       │       └── Phase 4: local/locked overlay and resource index
    │       │
    │       └────────── Phase 5: product lock and pure release graph
    │
    └── Continuous: benchmarks, migration compatibility, documentation

Phases 3, 4, and 5 may proceed in parallel after their Phase 1/2 data contracts
are stable. Phase 6 migrates the remaining repositories only after the pilots
pass all gates.
```

## Authority and migration rules

Every package has exactly one authority at a time:

- `legacy`: `nix/components/default.nix` and current launch registry;
- `manifest`: the package's static manifest and selected provider.

During shadow validation, both views may be compared, but execution reads only
the declared authority. Never merge missing fields from two authorities; that
would hide drift and make rollback ambiguous.

Cutover procedure for each package:

1. add and validate its static manifest;
2. compare normalized legacy and manifest plans;
3. run build/test/launch and performance equivalence tests;
4. change its authority atomically to `manifest`;
5. retain a short, explicit rollback switch;
6. remove its internal commands and paths from the central registry after the
   observation window;
7. remove the rollback path on a named cleanup milestone.

## Phase 0: stabilize correctness before adding abstractions

Goal: establish one evaluable, tested task graph and correct the known cache,
artifact, state, and release terminology problems.

### 0.1 Resolve `nix/tasks.nix` intentionally

Current blocker: `nix/tasks.nix` has an unresolved conflict between the
upstream `execIfModified` implementation and the stashed output-aware cache and
release wrapper implementation.

Resolution requirements:

- [ ] Preserve separate source-selection dependencies and actual artifact
      build-order dependencies.
- [ ] Use `buildDependencies`, not the broader source dependency list, for
      local task ordering.
- [ ] Preserve the release environment wrapper until the pure release graph
      replaces shell qualification tasks.
- [ ] Preserve state-root overrides used by isolated benchmarks/tests.
- [ ] Retain the measured native CMake/Ninja hot path.
- [ ] Treat the existing custom cache as a temporary compatibility layer, not
      the final artifact model.
- [ ] Add a regression fixture proving the selected resolution's dependency
      ordering and state behavior before removing conflict markers.
- [ ] Run all current unit tests and the existing warm-build budget check.

The conflict should be resolved as its own reviewable change. Do not combine it
with the new schema or CLI.

### 0.2 Fix known artifact-cache correctness holes

Files initially in scope:

- `scripts/workspace-task-cache`
- `nix/components/default.nix`
- `launch/simulation.nix`
- `tests/test_workspace_task_cache.py`
- `tests/test_devenv_hot_path.py`

Work:

- [ ] Record and validate output kind: regular file, executable, directory,
      or symlink.
- [ ] Record regular-file mode and symlink target.
- [ ] Add a content proof for files and a deterministic manifest/proof for
      exported trees.
- [ ] Reject truncated, modified, retargeted, or permission-changed outputs.
- [ ] Make cache publication atomic and impossible after interruption or
      partial failure.
- [ ] Lock the target coordinate during check/build/record so two terminals do
      not mutate the same build tree concurrently.
- [ ] Add `electrode-fake-sim` to the outputs produced/validated by the build
      that launch recommends.
- [ ] Strengthen generated Synapse output validation beyond a few sentinels.
- [ ] Correct the Modelica/Rumoca invalidation gap until artifact-level edges
      replace repository-level dependencies.
- [ ] Prove launch's suggested repair command recreates every missing required
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

### 0.3 Make release invocation command-scoped

- [ ] Add `ws release build TARGET` and `ws release test TARGET`.
- [ ] Make normal `ws build` and `ws test` unambiguously local.
- [ ] Deprecate persistent `ws mode release`; retain a warning-only
      compatibility path for a defined window.
- [ ] Snapshot effective product selection and mode at command start so
      another terminal cannot change an in-flight operation.
- [ ] Rename public repository `profile` terminology to `product` or
      `selection`; keep devenv profiles as an implementation term.

### 0.4 Freeze expanded baselines

- [ ] Retain current per-target workspace-cold and warm measurements.
- [ ] Rename the current “cold” category to `workspace-cold`, since shared Nix,
      language, and download caches remain populated.
- [ ] Add measurements for shell evaluation, `ws` dispatch, graph/list output,
      one implementation-file edit, one interface/schema edit, variant switch,
      and launch plan/start/readiness.
- [ ] Record p50 and p95 over repeated runs on the named reference host.
- [ ] Preserve logs and exact graph/cache-hit explanations.

### Phase 0 exit gate

- No conflict markers remain.
- Current tests pass.
- Existing warm target budgets pass.
- Tampering with any declared launch/build artifact causes a cache miss or
  explicit invalid-artifact error.
- The fake-simulator repair loop is correct.
- Release mode cannot change silently between terminals.
- No package-schema implementation has begun before this gate.

## Phase 1: static package control plane and bootstrap CLI

Goal: make package data discoverable, validatable, and fast without importing
package-owned Nix.

### 1.1 Freeze terminology

Write a short contract glossary and use it consistently in schema, CLI, docs,
and errors:

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

### 1.2 Decide YAML versus TOML with representative data

The architecture requires static data, not a particular serialization. Before
schema v1 is authoritative:

- [ ] Encode `synapse_fbs`, `cerebri_cubs2`, and `electrode_web` manifests in
      both YAML and TOML.
- [ ] Include multiple targets/variants, artifact edges, and a parameterized
      launch so the comparison is realistic.
- [ ] Compare line count, repetition, readability, diff/merge behavior,
      comments, editor completion, source-located validation errors, parser
      availability, and strictness.
- [ ] Ask at least one package author unfamiliar with the schema to modify
      build, artifact, and launch parameter fields in each format.
- [ ] Select one canonical source format and document why.
- [ ] Keep the normalized internal JSON model format-independent.

If YAML wins, require a strict YAML 1.2/JSON-compatible subset and reject
duplicate keys, merge keys, anchors/aliases, custom tags, non-string keys, and
implicit timestamp behavior. If TOML wins, require equally precise duplicate,
dotted-key, and array-of-table diagnostics.

### 1.3 Define schema v1

Provisional source path until the format decision:

```text
<repository>/cognipilot.package.{yaml|toml}
```

The one fixed root marker may declare multiple logical packages with explicit
repository-relative roots. Internal layouts remain arbitrary.

Schema areas:

- [ ] `apiVersion`, repository ID, package IDs, roots, aliases, lifecycle;
- [ ] package software-version source separate from schema version;
- [ ] provider/adapter identity and lazy implementation reference;
- [ ] targets, variant dimensions/defaults/constraints, and actions;
- [ ] explicit source inputs and artifact inputs;
- [ ] typed artifact outputs and compatibility/interface versions;
- [ ] action CPU/memory/exclusive lock requirements;
- [ ] named resources and executable exports;
- [ ] launch index, parameters, artifacts, capabilities, and includes;
- [ ] deployability class and immutable release-output reference;
- [ ] owner/license fields, initially warning-level during migration.

Static validators must reject:

- duplicate or invalid package/target/artifact/resource/launch IDs;
- package roots or referenced paths escaping through `..` or symlinks;
- overlapping writable output ownership;
- unresolved dependency/artifact/include references;
- cycles in each applicable DAG;
- invalid defaults, variants, parameter types, and include forwarding;
- process/resource name collisions;
- unqualified shell interpolation where argv is required;
- deployable outputs that reference mutable paths or custom providers without
  an approved exception.

### 1.4 Build `ws-core`

Add a typed core for static data and planning. Do not rewrite the safe Git and
sync portions of `scripts/ws` during this phase.

Responsibilities:

- parse strict source manifests;
- validate schema and semantic graphs with source-located diagnostics;
- combine them with the approved central repository/product catalog;
- emit canonical normalized JSON;
- cache by manifest/catalog/compiler contents, not package source contents;
- list/show packages, targets, artifacts, resources, launches, and parameters;
- compute dependency and reverse-dependency queries;
- emit human, JSON, and Graphviz-ready plan views;
- never build, fetch, import Nix, or execute package commands while parsing.

Implementation-language gate:

- Begin with the smallest implementation that meets diagnostics and the 100 ms
  hot-query budget; the current Python environment is a reasonable prototype.
- Preserve a process boundary/API so the core can become a packaged Rust binary
  later if measurements or bootstrap requirements justify it.
- Do not select Rust merely to make the implementation “typed”; measure parser
  startup, distribution, editor/schema libraries, and maintenance cost.

Proposed new files/directories after the format spike:

```text
schemas/cognipilot-package-v1.schema.json
scripts/ws-core/ or tools/ws-core/
tests/fixtures/package-manifests/
tests/test_workspace_manifest_*.py
```

### 1.5 Add a checked-in `./ws` bootstrap frontend

Required behavior outside an activated shell:

```sh
./ws doctor
./ws setup cubs2
./ws package list
./ws build cerebri_cubs2
```

- [ ] `./ws doctor` performs host-level checks without first requiring a valid
      package/devenv evaluation.
- [ ] Commands needing the pinned environment re-enter it automatically.
- [ ] Inside an active environment, the wrapper directly uses the fast path
      without nesting another shell.
- [ ] Manifest syntax errors and merge conflicts remain diagnosable.
- [ ] Completion reads cached normalized JSON and never evaluates Nix.
- [ ] `--json` is supported for doctor, package queries, and plans.

Doctor checks:

- host/platform support;
- Nix executable, daemon, required features, and disk space;
- pinned devenv availability/version;
- substituter/key configuration and trust without changing it silently;
- direnv state;
- unresolved Git conflicts;
- selected product and missing repositories;
- static manifest/compiler/index health;
- offline readiness;
- active launch sessions and occupied declared ports.

### 1.6 Shadow-index the legacy registry

- [ ] Generate normalized package views for pilot repositories without making
      them authoritative.
- [ ] Compare repository ID, package ID, source/build edges, commands, outputs,
      toolchain provider, and launch requirements.
- [ ] Report every disagreement; do not fill it from the legacy registry.
- [ ] Add a per-package authority field controlled by the root workspace.

### Phase 1 exit gate

- Canonical YAML or TOML format is selected from the bakeoff.
- Schema v1 has valid and invalid golden fixtures.
- Package list/show/graph/launch-show run from the cached index without Nix.
- One malformed optional manifest does not block shell entry or unrelated
  package queries/builds.
- Warm help/list/completion backend is within 100 ms p95.
- A 100-package synthetic manifest rebuild is within 1 second p95.
- No package has changed authority yet.

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

- [ ] Define default target/variant behavior for convenient package-only
      commands.
- [ ] Reject unknown or incompatible variant combinations before execution.
- [ ] Keep different boards/profiles/features in non-colliding directories.
- [ ] Permit two variants to build concurrently when they do not declare a
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
- selected toolchain/provider identity;
- dependency artifact identities/digests;
- output paths, kinds, modes, targets, and content/tree proof;
- start/end/duration/exit status and retained log paths;
- local/locked provenance.

Rules:

- [ ] Consumers invalidate from artifacts they consume, not complete dependency
      repositories.
- [ ] A docs/test-only producer change does not invalidate unrelated exports.
- [ ] A byte-identical exported artifact may preserve downstream validity.
- [ ] Native build directories remain stable for Cargo/CMake/Ninja
      incrementality.
- [ ] Published generations appear atomically only after all validation passes.
- [ ] A failed selected local action never activates a previous or locked
      artifact as if it were the requested result.

### 2.3 Select an execution-backend integration

Prototype these approaches on the simple and Zephyr pilots:

1. continue using root-defined devenv tasks with manifest-generated static
   inputs;
2. generate a selected-closure devenv task project and invoke it through
   `devenv --from`;
3. let `ws-core` schedule actions while invoking adapters/package devenv tasks
   as workers.

Decision criteria:

- no-op dispatch and selected-provider evaluation latency;
- dependency state and output propagation;
- bounded parallelism and resource locks;
- cancellation and log behavior;
- error quality and failure isolation;
- standalone package reuse;
- amount of duplicate scheduler logic.

Default preference: use devenv tasks/native tools for execution and keep
`ws-core` responsible for static planning, provenance, and artifact proof.
Choose a custom scheduler only if the spike proves the existing backend cannot
meet required semantics.

### 2.4 Add package-oriented DAG UX

```sh
ws build PACKAGE...
ws build --plan PACKAGE
ws build --why PACKAGE
ws build --up-to PACKAGE
ws build --dependencies-only PACKAGE
ws build --reverse-dependents PACKAGE
ws build --changed REVISION
ws build --failed
ws build --keep-going
ws build --jobs N

ws test PACKAGE
ws test --failed
ws test-result [--all] [--verbose]
```

- [ ] Plan shows every node, hit/run/block decision, provenance, and reason.
- [ ] Every node has a retained exact command, sanitized environment identity,
      duration, status, and individual logs.
- [ ] Tests ingest JUnit or adapter-native results into one index without
      discarding Cargo/Twister/colcon-native logs.
- [ ] CPU, memory, and exclusive locks prevent oversubscription and races.

### 2.5 Pilot package sequence

Do not promote only an easy Cargo package. The authority pilots are:

| Order | Package | What it proves |
| ---: | --- | --- |
| 1 | `synapse_ppm_bridge` | Small Cargo provider and basic manifest authoring |
| 2 | `synapse_fbs` | Generated multi-language targets and exported artifact completeness |
| 3 | `cerebri_modules` | Zephyr module/test behavior and shared west resources |
| 4 | `cerebri_cubs2` | Target variants, generated dependencies, guarded CMake/Ninja hot path |
| 5 | `electrode_web` | Cargo/npm monorepo, multiple artifacts, web assets, and launch consumers |

For each pilot:

- [ ] manifest remains readable and within the complexity budget;
- [ ] standalone provider behavior is tested where supported;
- [ ] shadow and legacy normalized plans agree or differences are intentional;
- [ ] current warm budget and edit-path budgets pass;
- [ ] authority cutover and rollback both work;
- [ ] central package-internal commands are removed after cutover observation.

### Phase 2 exit gate

- Five pilot shapes are manifest-authoritative.
- Artifact tamper, missing output, source edit, interface edit, dependency
  artifact edit, variant switch, interruption, and concurrency tests pass.
- Two CUBS variants build without path/cache collision.
- Default target commands remain simple.
- Framework overhead before the first native task is below 0.5 seconds p95.
- Existing no-op build budgets regress by no more than 10% without an approved
  and documented tradeoff.
- Fewer than 20% of ordinary pilot actions require custom executable providers;
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

The launch IR must be plan/renderable without provider evaluation. Expert
provider hooks may extend selected behavior but cannot hide required artifacts,
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

- [ ] Unknown values and invalid types fail before process startup.
- [ ] Includes explicitly forward/rename parameters.
- [ ] Runtime values are written to a redacted session input file and consumed
      by process wrappers.
- [ ] Host/port/vehicle/log-level changes perform zero Nix evaluation.
- [ ] Commands are argv arrays; shell strings require an explicit reviewed
      escape hatch.
- [ ] Secret values never enter Nix, argv, plans, completion, session metadata,
      or normal logs.

### 3.3 Make bundle wiring explicit

Move away from process-set introspection such as `builtins.hasAttr`.

- [ ] Electrode ground station declares its router and HTTP exports.
- [ ] Simulation and mocap declare consumed router/endpoints.
- [ ] Root `simulation-stack` selects providers and supplies wiring.
- [ ] Missing or multiple providers fail at plan time.
- [ ] Optional Qualisys behavior is an explicit bundle variant/capability, not
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

- [ ] Runtime/socket/log/process namespaces are isolated per session.
- [ ] Two copies of the same stack run concurrently with automatic ports.
- [ ] A new terminal can list, inspect, attach to, and stop existing sessions.
- [ ] Terminal loss does not make detached processes undiscoverable.
- [ ] `down` leaves no owned processes, sockets, locks, or ports.
- [ ] Use process-compose/devenv; do not implement supervision in `ws-core`.

### 3.6 Launch migration sequence

1. `electrode_web/ground-station`
2. `electrode_web/simulation`
3. root `simulation-stack` bundle with explicit wiring
4. `synapse_qualisys_bridge/mocap` when its repository is present
5. actual ROS launches through the ROS adapter

Keep legacy aliases for a defined transition window, but do not add the
ambiguous two-token `ws launch PACKAGE LAUNCH` form.

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

- [ ] Selected local packages shadow locked exports only in local mode.
- [ ] Compatibility/interface constraints are checked before substitution.
- [ ] Compiled non-leaf overrides rebuild the affected reverse closure or are
      refused.
- [ ] Locked fallback is allowed only for absent/unselected local packages,
      never after selected local failure.
- [ ] Every plan shows `LOCAL dirty`, `LOCAL commit`, or `LOCKED` provenance.
- [ ] `ws env --explain` and launch/build plans agree on every resolved export.
- [ ] Atomic generation switching prevents readers seeing partially updated
      artifacts.

### Phase 4 exit gate

- The default product can mix selected local packages with compatible locked
  packages without ambient-path dependence.
- Generated-interface and compiled non-leaf override tests rebuild/refuse the
  correct reverse closure.
- A locked/store-backed resource fixture still resolves after its source
  checkout is removed; complete product closure is proved in Phase 5.
- No local failure silently falls back.
- Manual native commands through `ws exec` receive the same resolution shown
  by `ws env --explain`.

## Phase 5: product lock and pure Nix release graph

Goal: make “fixed package versions” and deployment identity exact.

### 5.1 Define product lock v2

Extend or replace `workspace.lock.json` with a committed lock containing:

- product/selection ID;
- package IDs and software versions;
- canonical repository URLs and exact commits/tree identities;
- package roots and manifest digests;
- schema/compiler and adapter versions;
- selected targets/variants;
- complete immutable dependency closure;
- top-level and package Nix output identities/NAR hashes;
- target platform/system;
- allowed external-source exceptions.

The integration lock used for normal developer fallback and the promoted
product lock may share a schema but have different policy states.

### 5.2 Implement the top-level release graph

- [ ] Add a pure top-level product flake/module that consumes the product lock,
      not whichever clean worktrees happen to exist.
- [ ] Require package repositories to expose real Nix derivation outputs before
      they are marked deployable.
- [ ] Mark shell Cargo/Twister/QEMU builds as qualification tasks only.
- [ ] Forbid mutable source dependencies, `devel/`, workspace build paths, and
      non-Git deployed trees from deployable closures.
- [ ] Build/store resources required by release launches.
- [ ] Record output/NAR digests, package revisions, builder identity, SBOM, and
      provenance/attestation.
- [ ] Pin third-party release workflow actions immutably.

### 5.3 Prove release isolation

- [ ] Place unique dirty sentinels in source, `build/`, and `devel/`.
- [ ] Build the locked product and inspect references/closure/output bytes.
- [ ] Confirm no sentinel or mutable workspace path is present.
- [ ] Remove/rename source checkouts and run release resource/launch lookup.
- [ ] Restore the lock on a second prepared host and reproduce output identity
      or explain any declared non-reproducible boundary.

### Phase 5 exit gate

- One representative product is built exclusively from a committed lock and
  pure Nix derivations.
- Its promoted manifest records the complete package/output closure.
- Local state contamination tests pass.
- Release launches/resources work without development checkouts.
- Qualification outputs are never presented as deployable artifacts.

## Phase 6: workspace-wide migration and governance

Goal: migrate remaining packages without permanent dual truth or regressions.

### Migration waves

| Wave | Packages | Notes |
| --- | --- | --- |
| Pilot | `synapse_ppm_bridge`, `synapse_fbs`, `cerebri_modules`, `cerebri_cubs2`, `electrode_web` | Exercises all difficult contract shapes |
| Default core | `rumoca`, `modelica_models`, `csyn` | Completes normal CUBS2/modelica graph |
| Optional firmware/bridges | `cerebri_rdd2`, `zros`, `zros_drivers`, `qualisys_rust_sdk`, `synapse_qualisys_bridge` | Migrate when repositories are materialized and package PRs can be tested |
| ROS adapter | `csyn_ros2_bridge` | Read/check `package.xml`; do not duplicate ROS metadata |
| External exception | `FastDyn` | Development/qualification manifest; remain non-deployable until an approved immutable source/output path exists |

For repositories not currently present, prepare workspace schema/expected IDs
but land the package-owned manifest in that repository before authority
cutover.

### Governance gates

- [ ] Central catalog owns repository URL/trust and package namespace mapping.
- [ ] Package manifests cannot self-select products or source URLs.
- [ ] Owner teams reference an approved registry.
- [ ] Licenses use SPDX expressions.
- [ ] Lifecycle is one of experimental, stable, deprecated, or retired.
- [ ] Namespace, owner, deployability, capability, dependency-scope, and release
      changes receive the required platform/release review.
- [ ] `custom` providers require an approved exception before deployment.
- [ ] Compiler supports current and previous schema major during a documented
      window with golden compatibility fixtures.
- [ ] CLI compatibility aliases have an owner and removal milestone.
- [ ] Legacy package definitions are deleted after cutover; dual authority is
      never permanent.

### CI placement

Package repository CI:

- static schema and semantic validation;
- identity/namespace check;
- path/symlink confinement;
- provider/action/launch evaluation for that package;
- focused build/test and warm budget;
- secret-redaction checks.

Workspace integration CI:

- exact integration lock resolution;
- package/resource uniqueness and all graph validation;
- cross-repository artifact compatibility;
- launch planning without process startup;
- legacy/manifest equivalence during migration;
- representative launch smoke tests.

Release CI:

- exact product lock and pure output closure;
- no local fallback/external mutable input;
- SBOM, output hashes, signed provenance, and promotion policy.

None of these exhaustive gates belongs in the `ws build` hot path.

### Phase 6 exit gate

- Every central component is manifest-authoritative or an explicit documented
  external exception.
- Root registry contains acquisition/policy only, not package-internal commands
  or paths.
- Root launch bundles contain composition/wiring only, not leaf process
  implementations.
- Default product build/test/launch/release workflows use the new model.
- Legacy task/launch compatibility code has a removal release or is gone.
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
| manifest rebuild for 100 packages | 1 s |
| orchestration before first native task | 0.5 s |
| launch preflight | 250 ms |
| launch dispatch after preflight | 1 s |
| unchanged default-product build graph | 8 s |
| small CUBS2 source edit | 10 s |

Rules:

- ordinary package source edits never invalidate the root shell environment;
- package/list/graph/completion and runtime parameter changes never evaluate
  Nix;
- launch metadata changes never invalidate build artifacts;
- existing per-target warm budgets remain enforced;
- package-contract orchestration may not add more than 10% to an existing warm
  target without explicit review;
- machine-cold/network benchmarks are recorded but not gated by one absolute
  noisy sample.

## Required test matrix

### Static control plane

- valid/invalid format fixtures;
- duplicate keys/IDs and unsupported schema;
- path traversal and symlink escape;
- missing/cyclic artifact and launch references;
- malformed optional package failure isolation;
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
- deterministic logs and JUnit/test-result aggregation.

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

- more than 20% of ordinary package actions require custom executable
  providers;
- a simple package manifest needs roughly more than 50 lines or a representative
  complex package roughly more than 150 lines without meaningful behavior;
- an invalid optional manifest/provider blocks unrelated shell entry/builds;
- package list/graph or completion requires Nix evaluation;
- warm build budgets regress by more than 10% due to orchestration;
- runtime launch parameter changes require Nix evaluation;
- the selected execution backend requires duplicating a full task scheduler;
- local override safety cannot be explained and enforced at artifact level;
- release outputs cannot prove exclusion of local state.

If the design fails these gates, improve the central registry rather than
forcing distributed manifests. Do not move to full ROS 2 or Bazel merely
because this particular contract needs revision.

## Recommended first implementation changes

Keep early changes small and reviewable:

1. Resolve `nix/tasks.nix` and restore one passing task baseline.
2. Add failing artifact tamper/mode/missing-simulator tests.
3. Harden the temporary cache and align Electrode launch/build outputs.
4. Add command-scoped release mode and concurrency tests.
5. Add the checked-in `./ws doctor` bootstrap skeleton.
6. Run the YAML/TOML bakeoff and approve the format-independent normalized
   model.
7. Add schema fixtures and a read-only `ws-core validate/list/show/graph`.
8. Add `synapse_ppm_bridge` in shadow mode.
9. Add artifact coordinates/manifests and cut over that simple pilot.
10. Add Synapse/CUBS2 generated-artifact and variant pilots.
11. Add Electrode artifacts, launch IR, typed parameters, and named sessions.
12. Prove the product-lock/release boundary before broad migration.

Each step should leave the legacy workspace usable and retain an explicit
rollback until its exit gate passes.

## Definition of done

This roadmap is complete when all of the following are true:

- a developer with Nix installed can run `./ws doctor`, set up a product, and
  reach the first package build in at most three workspace commands;
- every supported package is discoverable and its graph/resources/launch
  arguments are inspectable without package Nix evaluation;
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
