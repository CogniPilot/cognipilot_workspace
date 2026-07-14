# Six-agent adversarial review of the CogniPilot development environment

Status: historical pre-cutover direction review, 2026-07-13. The implemented
Nix/Rust boundary and current roadmap supersede its migration scaffolding and
central-registry recommendations; they remain here as decision context.

The concrete implementation sequence, phase gates, package migration waves,
and initial change list are in `dev/devenv-implementation-roadmap.md`.

This review challenges the direction proposed in
`dev/devenv-package-and-launch-architecture.md`. Six independent reviewers
examined it from these perspectives:

1. ROS 2, colcon, package discovery, and launch ergonomics
2. Nix/devenv composition, evaluation, and failure isolation
3. new, occasional, and expert developer experience
4. incremental build DAGs, cache correctness, and performance
5. polyrepo governance, schema evolution, security, and releases
6. competing architectures and falsification criteria

The reviewers worked independently. All six reached the same high-level
conclusion:

> Keep ROS-like developer semantics with devenv/Nix mechanics, but do not
> implement the original executable `cognipilot.nix` package contract as
> written.

The recommended target is a **static package control plane with lazy
devenv/Nix execution providers**.

## Executive decision

Keep:

- package self-description without a mandatory internal directory layout;
- package-owned build/test knowledge and package-owned leaf launches;
- root-owned repository acquisition, product selections, namespace policy,
  and cross-package launch bundles;
- native Cargo, npm, CMake/Ninja, west/Zephyr, Modelica, and custom hot paths;
- a small base devenv shell and automatically managed local artifacts;
- immutable Nix outputs as the only deployable product artifacts;
- no sourced setup scripts;
- no implicit sync, build, test, release packaging, or CI from launch.

Change:

- executable `cognipilot.nix` discovery becomes a static,
  schema-validated `cognipilot.package.yaml` manifest;
- eager import of every package environment becomes selected-only lazy
  providers/adapters;
- a package-name dependency graph becomes an action, target, variant, and
  artifact graph;
- `devel/` becomes an explicit artifact registry with isolated, atomic package
  generations rather than a vaguely ordered shadow directory;
- launch definitions become a declarative, statically inspectable launch
  intermediate representation, with process-compose/devenv as the backend;
- launch parameters are validated and resolved at runtime without Nix
  reevaluation;
- launches produce persistent named sessions with plans, logs, ports,
  provenance, and lifecycle state;
- sticky global release mode becomes an explicit command-scoped operation;
- repository snapshots become complete product locks and promotion manifests.

Reject:

- converting every repository to ROS 2/ament/colcon;
- eager package-owned Nix modules as the universal manifest;
- a universal mutable install prefix used for deployment;
- recursive package discovery through arbitrary source trees;
- arbitrary executable launch programs as the normal public launch API;
- package-level cache stamps as the final correctness model;
- automatic local shadowing without compatibility and reverse-dependency
  handling;
- writing a second process supervisor instead of using devenv/process-compose.

## The most important correction

The earlier proposal combined static package identity with executable Nix:

```text
cognipilot.nix → Nix module evaluation → package graph + tasks + launches
```

That has poor failure isolation. Nix option typing constrains the returned
configuration, not what package-owned Nix evaluates. One syntax error,
expensive path coercion, incompatible module, or unexpected import can prevent
shell entry and break unrelated package listing or recovery commands.

`src/` contains independent ignored Git worktrees. One installed worktree,
`src/electrode_web`, is currently approximately 14 GB because it contains
native build products. A reviewer measured approximately 28 seconds when a Nix
probe coerced that repository path. Repository roots and mutable build trees
must therefore never become Nix path values merely to describe package roots.

The corrected boundary is:

```text
                         STATIC CONTROL PLANE

 central repository catalog      package-owned static manifests
      URLs/trust/policy      +    IDs/tasks/artifacts/launch schemas
                \                    /
                 \                  /
                  manifest compiler
                          │
              normalized cached JSON index
                  │                 │
          package/DAG queries   launch planning
                  │                 │
                  └────────┬────────┘
                           │ selected closure only

                         EXECUTION PLANE

                 shared adapter or lazy provider
                           │
            devenv tasks / native build / process-compose
                           │
            typed local artifact and session manifests

                         RELEASE PLANE

          committed product lock → pure top-level Nix graph
                           │
              immutable store outputs + attestation
```

Package list, help, completion, graph queries, launch argument help, and launch
planning use the cached static index. They do not import package Nix or inspect
ordinary source files. Package-local devenv/Nix is evaluated only when the user
selects an action that needs it.

## Current implementation evidence

The review found several concrete issues that must shape the design.

### `devel/` is not yet a general overlay

The shell exports `COGNIPILOT_DEVEL_ROOT`, and several build recipes place
artifacts under it, but it is not generally translated into ordered `PATH`,
`CMAKE_PREFIX_PATH`, `PYTHONPATH`, `PKG_CONFIG_PATH`, npm, Zephyr, or Cargo
resolution. Consumers pass package-specific paths such as
`FETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C` and `CUBS2_RUMOCA_PYTHON`. Launches
currently execute binaries directly from repository `target/debug` trees.

This is useful local artifact staging, but calling it a complete automatic
overlay is premature. The new model must specify exports, ordering,
compatibility, provenance, and language-specific consumption.

### Cache hits do not prove artifact integrity

One reviewer demonstrated that the current task cache can still report a hit
after changing a declared output's executable mode or bytes. It validates
source fingerprints and output existence, not the complete type, mode, and
content identity of published artifacts.

There is also a direct launch/build mismatch:

- `launch/simulation.nix` requires
  `src/electrode_web/target/debug/electrode-fake-sim`;
- Electrode's current declared build outputs include the ground-station binary
  and web index, but not the fake simulator;
- deleting the fake simulator can make launch recommend `ws build
  electrode_web`, while that build is allowed to cache-hit without repairing
  the missing executable.

These are correctness problems, not only performance details.

### Package-level identity is too coarse

One package can produce multiple boards, target triples, feature sets,
generated languages, debug/release variants, executables, libraries, web
assets, and simulation configurations. A node called only `cerebri_cubs2` or a
launch requirement called only `electrode_web` cannot distinguish them.

Without variant-aware identity, alternating or concurrent builds can overwrite
one another and launches can consume whichever output happened to be produced
last.

### Global state can surprise concurrent terminals

The former `ws mode` and `ws profile` commands wrote shared state files. A
release-mode change in
one terminal can change the behavior of a build in another terminal. The word
“profile” is also overloaded across repository selection, devenv toolchain
profiles, and launch profiles.

Local development should be the command default. Release should be explicit:

```sh
ws build cerebri_cubs2
ws release build cerebri_cubs2
```

A saved product selection can remain convenient, but each command must capture
and print its effective immutable selection at start.

### Current launch composition is implicit

The simulation, ground-station, and mocap modules use `builtins.hasAttr` on the
global process set to change network roles, router ownership, arguments, and
startup relationships. Adding a process can silently change another launch's
behavior.

Composition must instead expose and wire explicit capabilities:

```text
ground-station exports: router.endpoint, http.url
simulation consumes:    router.endpoint
mocap consumes:         router.endpoint
```

The root product bundle chooses providers and wiring. Missing or ambiguous
providers fail during launch planning.

### Preflight happens too late

Required-artifact checks currently execute inside supervised process commands.
The process manager can start, retry a process that can never succeed, and hide
the useful remediation among TUI/log output. Artifact, parameter, port, secret,
device, permission, and graph checks should complete before any process starts.

### The recovery tool is unavailable before the shell

`ws` currently exists only inside the activated devenv shell. It cannot run
`doctor` when a Nix/devenv/cache/evaluation problem is the reason the shell
cannot be entered. A checked-in `./ws` bootstrap frontend is required.

### The current release name overstates some outputs

Several current release-mode tasks run mutable Cargo, Twister, QEMU, or other
shell builds. Those are valuable pinned qualification tasks, but they are not
automatically promotable immutable product artifacts. The current workspace
snapshot records repository commits, but not a complete product selection,
package/variant closure, schema/compiler version, Nix output identities, or
content/NAR digests.

Only a pinned Nix derivation in a complete product closure should be called a
deployable release output.

## Static package contract

Require one data-only root marker, provisionally
`cognipilot.package.yaml`. It may declare multiple logical packages with
explicit roots, so existing monorepos keep arbitrary internal layouts.

Illustrative shape:

```yaml
apiVersion: cognipilot.dev/v1
repository: electrode_web

packages:
  - id: electrode_web
    root: .
    ownerTeam: cognipilot/flight-software
    lifecycle: stable

    version:
      source: cargo
      path: Cargo.toml

    provider:
      adapter: cargo-npm@1
      module: nix/cognipilot-development.nix

    targets:
      ground_station:
        variants:
          profile: [debug, release]
        action:
          command: [cargo, xtask, workspace-build]
        inputs:
          artifacts:
            - synapse_fbs:bindings@language=rust
        outputs:
          executable:
            kind: executable
            path: target/{profile}/electrode-ground-station
          web:
            kind: tree
            path: apps/web/build

    launches:
      ground-station:
        requires:
          - target: ground_station
            output: executable
        parameters:
          http.port:
            type: port
            default: 8790
        capabilities:
          listenPorts: [parameter:http.port]
```

The exact serialization and field names must be tested in the pilot. The
non-negotiable properties are:

- static parsing performs no package code evaluation;
- `apiVersion` and the package's software version are separate;
- repository and package identities are separate;
- logical package roots are explicit and repository-relative;
- actions, variants, artifact inputs/outputs, resources, launches, and release
  references are inspectable without Nix;
- commands are argv arrays, never unquoted shell templates by default;
- executable providers are references evaluated only for a selected closure;
- realpath and symlink-safe confinement prevents manifest paths escaping the
  repository;
- an unsupported or malformed optional package does not brick unrelated shell
  entry or package queries.

### What remains central

The package cannot self-authorize organization policy. The root catalog remains
authoritative for:

- approved repository ID, fetch URL, visibility, and trust tier;
- package-ID-to-repository namespace ownership;
- expected/allowed package IDs;
- product selections;
- cross-package product launch bundles;
- external/non-Git exceptions;
- release and promotion policy.

The small duplication of an expected package ID is intentional: the repository
declares what it contains, while the workspace approves where that name may
come from. Validation rejects disagreement.

### Logical package granularity

Do not turn every Cargo crate, npm workspace, Nix output, or source directory
into a CogniPilot package. A logical package should be independently selectable
and have at least one of:

- an independently consumed export;
- an independently useful build/test target;
- an independently owned launch/resource set;
- a compatibility/version boundary;
- an independent release unit.

Nested roots may be allowed when explicit. Two packages must never own the same
writable build or artifact location.

### Static data and lazy providers

Common centrally versioned adapters should cover Cargo, Cargo/npm, CMake,
Zephyr, Nix, and simple custom command execution. A package may reference a
repository-owned devenv module or flake app when an adapter is insufficient,
but that code is trusted executable configuration and activates only after an
explicit task or launch selection.

The `custom` adapter is an escape hatch, not a way to bypass all enforcement.
It must still declare actions, artifacts, resources, and provenance. A custom
provider should require review before the package becomes deployable.

## DAG and artifact model

There is no single universal package DAG. Maintain separate graphs with
separate semantics:

1. repository acquisition graph;
2. build action/artifact DAG;
3. focused test DAG;
4. launch include DAG;
5. process startup/readiness DAG;
6. immutable deployment closure.

The execution/cache identity is conceptually:

```text
package + target + variant + mode + action + dependency resolution
```

For example:

```text
synapse_fbs:bindings@language=c+local
    → cerebri_cubs2:configure@board=native_sim_native_64+local
    → cerebri_cubs2:firmware@board=native_sim_native_64+local
```

The user may still type the convenient package form:

```sh
ws build cerebri_cubs2
```

The package defines its default target/variant. `ws build --plan` prints the
fully qualified graph before execution.

### Dependency semantics

Edges should point to exact actions or exported artifacts where execution
depends on them. Keep these concepts distinct:

- source availability/acquisition;
- action ordering;
- exported build interface;
- build-tool input;
- runtime/deployment closure;
- test-only input;
- launch/resource dependency;
- capability/provider selection;
- conditional platform/variant compatibility.

The contract need not copy every `package.xml` tag, but each supported edge
kind needs documented transitivity, local override, release, and selector
semantics.

### Successful artifact manifest

Every successful action atomically publishes a typed artifact manifest:

```text
target coordinate and effective variants
normalized action/provider identity
source-input digest
toolchain identity
dependency artifact digests
output paths, kinds, modes, and content/tree proof
timings and retained log location
```

Consumers invalidate from consumed artifact identities, not entire dependency
repository hashes. A producer edit that leaves the exported bytes unchanged
does not need to rebuild downstream consumers.

Native build directories stay stable so Cargo/CMake/Ninja retain fine-grained
incrementality:

```text
build/<package>/<target>/<variant>/
```

Published local artifacts use generation-based staging and an atomic active
link:

```text
devel/<package>/<target>/<variant>/generations/<generation>/
devel/<package>/<target>/<variant>/current
```

Do not content-address the entire native build directory on every edit. The
native build system owns fine-grained state; the workspace proves the outputs
published by the last successful action.

### Local override safety

A local package may shadow a locked export only when compatibility policy
allows it. Overriding a compiled non-leaf package must do one of:

- rebuild the complete affected reverse-dependency closure;
- prove the relevant interface/ABI is compatible;
- refuse the mixed graph.

Fallback to the locked package is allowed only when the local package is
unselected or absent. It must never happen because a selected local build
failed; that would make the developer test a different graph than requested.

Every plan and execution summary reports provenance such as `LOCAL dirty`,
`LOCAL commit`, or `LOCKED revision/output`.

### Execution engine boundary

The static `ws` core owns parsing, graph resolution, planning, provenance, and
artifact validation. It should initially execute the selected closure through
devenv 2.1 tasks and native tools rather than inventing another generic task
engine.

Devenv already provides task DAG edges, dependency states (`started`, `ready`,
`succeeded`, and `completed`), traversal modes, typed task inputs, status
checks, process/task edges, and tracing. CogniPilot adds the missing artifact
proof, variants, resource locks, provenance, and package-oriented diagnostics.

Bound parallelism explicitly. Native Cargo, CMake, npm, and Nix jobs can each
consume all available cores; scheduling several without CPU/memory/exclusive
resource declarations can be slower than sequential execution. At minimum:

- `ws build --jobs N`;
- a memory-aware default;
- coordinate locks for shared build/output/worktree resources;
- no publication after interruption or partial failure;
- deterministic per-node logs and a final graph summary.

## Package/resource index

Adopt the useful semantics of the ament resource index without copying its
install-prefix implementation. The generated static index should map qualified
package resources to local or release locations:

- packages and versions;
- targets and artifacts;
- executables;
- launch descriptions;
- configuration, model, schema, and data resources;
- plugins and named capabilities;
- development/release provenance.

Commands:

```sh
ws package list
ws package show electrode_web
ws package path electrode_web
ws package validate
ws resource electrode_web/config/default
ws run electrode_web/ground-station
ws env electrode_web --explain
```

Do not put every resource directory on global search paths. Resolve by
qualified name through the index and let adapters construct only the selected
task environment.

## Launch system direction

Raw devenv/process-compose modules are a good backend, but they are too dynamic
and underspecified as the public package launch contract. The public layer must
be declarative enough to validate, complete, plan, and reproduce without
starting processes or evaluating arbitrary code.

### Required launch semantics

A package launch can declare:

- typed parameters and defaults;
- package launch includes with argument forwarding;
- explicit process groups/namespaces and conditions;
- exact artifact/resource requirements;
- argv, working directory, and structured environment mapping;
- provided and consumed endpoints/capabilities;
- resource claims such as ports, devices, writable paths, GUI, and secrets;
- startup edges and readiness probes;
- restart, backoff, timeout, and shutdown policy;
- responses to exit, failure, and readiness loss;
- public URLs and other values to report after startup.

Start with the events the current products need. Do not reproduce the complete
ROS launch event language preemptively.

### Typed runtime parameters

Parameters are mandatory contract data, not an optional future detail. Initial
types should include:

- string, integer, float, boolean, and enum;
- host/IP, URL, and port;
- bounded number and duration;
- path with allowed-root/existence/read/write policy;
- list/map where justified;
- secret reference;
- automatically allocated resource.

Precedence is fixed and inspectable:

```text
package default < bundle default < parameter file < command line
```

Runtime values such as host, port, vehicle ID, and log level must not trigger
Nix reevaluation. The planner validates them into a redacted session JSON/env
file consumed by generated process wrappers. Only a topology-changing choice
may require recompiling the selected launch plan, and that plan should be
cached independently of ordinary runtime values.

Commands use one qualified launch token and action-first lifecycle syntax:

```sh
ws launch show electrode_web/ground-station
ws launch plan simulation-stack \
  --set mocap.host=qtm.example \
  --set ground-station.http.port=8791

ws launch up simulation-stack \
  --name qtm-test \
  --params scenarios/qtm-test.yaml
```

Do not add the ambiguous two-token `ws launch PACKAGE LAUNCH` alias.

### Named sessions and multiple instances

Every launch creates a named session containing:

- resolved launch/include graph;
- redacted parameter values and parameter-file identity;
- assigned ports/resources;
- package revisions and artifact identities;
- process commands, working directories, PIDs, and supervisor socket;
- readiness timing, restarts, exit state, and timestamped logs.

Operations address the session:

```sh
ws launch status
ws launch status qtm-test
ws launch logs qtm-test/ground-station --follow
ws launch restart qtm-test/mocap
ws launch down qtm-test
ws launch events qtm-test
ws launch prune
```

Session runtime directories, sockets, state, process names, logs, and automatic
ports must be isolated so two copies of the same stack can run concurrently.

### Preflight

Before the process manager starts, validate all of these and return one
consolidated error report:

- repository/source availability;
- exact required artifact existence, type, mode, proof, and freshness;
- parameter types, bounds, and unknown values;
- include, wiring, and startup graph cycles;
- port/resource collisions;
- required secrets without revealing their values;
- device/path existence and permissions;
- working directories and executable paths;
- release/local provenance policy.

Example:

```text
Cannot start simulation-stack:

  electrode_web:fake_sim@profile=debug is missing
    Run: ws build electrode_web:fake_sim@profile=debug

  synapse_qualisys_bridge is not checked out
    Run: ws sync synapse_qualisys_bridge

No processes were started.
```

Launch does not build by default. A future `--build=missing` may explicitly run
only the incremental development DAG. It still does not sync, fetch, test,
package, or run CI.

### Secrets

Descriptors declare secret names and policies, never values. Secret values
must not enter Nix evaluation, derivation arguments, argv, static manifests,
completion, plans, session metadata, or ordinary logs. Runtime providers can
resolve references from protected files, environment references, keyrings,
sops/age, or an organization provider. Plans and logs remain redacted.

## Developer-facing command model

Keep the expert native tools available, but make the common path obvious:

```sh
# Works before shell activation.
./ws doctor
./ws setup cubs2

# Package discovery and execution.
ws package list
ws package show cerebri_cubs2
ws build --plan cerebri_cubs2
ws build cerebri_cubs2
ws build --why cerebri_cubs2
ws test cerebri_cubs2
ws test-result

# Useful selector subset inspired by colcon.
ws build --up-to cerebri_cubs2
ws build --dependencies-only cerebri_cubs2
ws build --reverse-dependents synapse_fbs
ws build --changed origin/main
ws build --failed

# Expert and editor integration.
ws exec electrode_web -- cargo check
ws shell electrode_web
ws env electrode_web --json
ws editor vscode electrode_web
```

Public repository selection is named `product`; devenv and launch profiles
remain separate implementation/runtime concepts.
Reserve “devenv profile” for the implementation mechanism.

`./ws doctor` should diagnose platform support, Nix daemon/features, pinned
devenv, cache trust, disk, direnv, missing repositories, manifest/evaluation
errors, merge conflicts, offline readiness, active sessions, and occupied
ports. Safe user-local repairs may be offered through `--fix`; administrator
trust changes remain explicit.

## What to copy from ROS and colcon

| ROS/colcon capability | Decision for CogniPilot |
| --- | --- |
| Package self-description and unique names | Adopt through static package manifests |
| Dependency selectors (`up-to`, reverse dependents, failed) | Adopt the useful subset |
| Package/resource index | Adopt semantics with generated local/store index |
| Package-owned launches | Adopt |
| Launch arguments, defaults, includes, conditions, namespaces | Adopt through static launch IR |
| Launch argument discovery and resolved plan | Adopt and make available without Nix evaluation |
| Process events and lifecycle responses | Adopt incrementally for ready/lost/exit/failure/shutdown |
| Isolated package outputs | Adapt to typed generation-based `devel/` artifacts |
| Per-invocation/package logs and test-result summaries | Adopt |
| Package scaffolding and lint | Adopt after schema pilot |
| Mixins/presets | Defer until core parameter and variant semantics stabilize |
| XML/YAML/Python launch format parity | Defer; semantics matter more than syntax count |
| `package.xml` for non-ROS software | Reject |
| ament wrappers for all native projects | Reject |
| recursive source-tree crawling | Reject |
| sourced install-overlay setup scripts | Reject |
| mutable install prefix as deployment identity | Reject |
| unrestricted executable launch programs as the default | Reject |

ROS-native repositories should use an adapter that reads/checks their existing
`package.xml` and launch metadata. Do not duplicate facts manually or require
non-ROS packages to acquire ROS dependencies.

## Alternative architecture decision matrix

Scores are 1–10; higher is better. These are architecture-fit judgments to be
validated by the proof of concept, not benchmark results.

| Direction | Onboarding | Hot path | Heterogeneous/polyrepo | Launch UX | DAG correctness | Release reproducibility | Maintainability | Ecosystem reuse | Migration safety | Failure isolation | Total |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Improve current central registry | 7 | 8 | 9 | 6 | 7 | 9 | 5 | 6 | 9 | 5 | 71 |
| **Static manifests + thin `ws` + lazy providers** | **9** | **9** | **9** | **9** | **9** | **9** | **8** | **8** | **7** | **9** | **86** |
| Eager package-owned Nix/devenv modules | 5 | 6 | 9 | 8 | 9 | 9 | 6 | 8 | 6 | 3 | 69 |
| Full ROS 2/colcon | 5 | 6 | 4 | 10 | 9 | 6 | 6 | 10 | 2 | 7 | 65 |
| Bazel/bzlmod workspace | 4 | 9 | 6 | 6 | 10 | 8 | 5 | 9 | 1 | 9 | 67 |

The current central registry is the safest migration scaffold and a viable
fallback at the current workspace size. It should remain authoritative while
the static model is proven. Bazel becomes worth reconsidering only if remote
execution and cross-language action caching become strategic enough to fund a
dedicated build-platform effort.

## Revised roadmap

### Phase 0: repair correctness and freeze behavior

- Resolve the existing `nix/tasks.nix` conflict without guessing between the
  two task/cache semantics.
- Add artifact tamper, executable-mode, missing-output, interruption, and
  concurrent-build regression tests.
- Make Electrode's declared outputs and launch requirements agree, including
  the fake simulator.
- Record current shell, planning, build, and launch timing separately.
- Rename existing “cold” measurements to `workspace-cold`; retain a separate
  future machine-cold category.
- Make release invocation command-scoped before adding more global state.

### Phase 1: static read-only control plane

- Define terminology: repository, package, target, variant, action, artifact,
  resource, product, launch, process, session, qualification output, and
  deployable output.
- Define the versioned static schema and JSON Schema.
- Add a typed `ws-core` for parsing, validation, graph queries, planning, and
  normalized JSON output. The existing Bash `ws` can delegate new commands;
  do not rewrite safe Git/sync behavior immediately.
- Add cached package/resource indexes and completion with no package Nix
  evaluation.
- Run the new index in read-only shadow mode beside the central registry.
- Use one authority flag per package (`legacy` or `manifest`) rather than
  silently merging facts from two sources.

### Phase 2: artifact-qualified DAG pilot

Pilot the hard shapes, not only the easy one:

- `synapse_ppm_bridge`: simple Cargo action/provider;
- `synapse_fbs`: generated multi-language artifact producer;
- `cerebri_cubs2` or `cerebri_modules`: Zephyr target/variant consumer;
- `electrode_web`: Cargo/npm monorepo with multiple executable/web artifacts.

Add target/variant identities, typed artifact manifests, atomic development
generations, locks, bounded parallelism, `--plan`, `--why`, and per-node logs.
Keep current native build directories and devenv task execution.

### Phase 3: declarative launch and sessions

- Move Electrode simulation and ground-station launch ownership into the
  Electrode package manifest.
- Define typed runtime parameters, includes, explicit capability wiring,
  resources, readiness, restart/shutdown policy, and redacted secrets.
- Add `show`, `plan`, preflight, named sessions, isolated runtime state,
  multi-instance ports, logs, events, and cleanup.
- Keep process-compose/devenv as supervisor.
- Express `simulation-stack` as a root product bundle.
- Migrate mocap ownership when its repository is available.

### Phase 4: real local overlay and resource index

- Resolve selected local/locked artifact instances explicitly.
- Implement adapter-specific environment translation.
- Rebuild/refuse unsafe reverse closures for local compiled overrides.
- Prohibit silent locked fallback after local failure.
- Add `ws resource`, `ws run`, and `ws env --explain`.
- Verify launches from release resources with source checkouts absent.

### Phase 5: release/product graph

- Expand the workspace snapshot into a committed product lock covering product
  selection, packages, versions, repository revisions/trees, manifest/compiler
  versions, variants, Nix outputs, and closure digests.
- Build deployable products through a pure top-level Nix graph.
- Treat shell-based release tasks as qualification only.
- Disable local fallback, mutable source dependencies, and non-Git external
  trees in deployable closures.
- Produce SBOM, signed provenance/attestation, and output/NAR digests.

### Phase 6: migration and governance

- Cut packages over atomically from legacy to manifest authority.
- Require the contract for new packages after the pilots pass.
- Support the current and previous schema major during a documented window.
- Warn on missing ownership/license metadata early; make policy fields hard
  gates after repositories have migration support.
- Remove compatibility aliases and dual registry paths on owned schedules.
- Keep ROS package metadata authoritative through the ROS adapter.

## Performance and usability SLOs

Use p50/p95 repeated measurements on a named reference developer host and a
separate CI class.

| Operation | Initial p95 budget |
| --- | ---: |
| `ws help`, package list, launch list, completion backend | 100 ms |
| graph/build/launch plan from cached index | 200 ms |
| warm shell activation | 0.5 s |
| cold Nix/devenv evaluation on provisioned host | 5 s |
| manifest rebuild for 100 packages | 1 s |
| framework overhead before first native task | 0.5 s |
| launch preflight | 250 ms |
| launch dispatch after preflight | 1 s |
| unchanged default-profile build graph | 8 s |
| small CUBS2 source edit | 10 s |

Existing per-target warm build budgets remain authoritative. Ordinary source
edits must not invalidate the root shell environment. Package list, graph,
completion, and runtime launch-parameter changes must not evaluate Nix.

## Falsification experiments

Do not roll the design out by conviction alone. Option B remains preferred only
if it passes these experiments.

### Scale and evaluation

Compare the legacy central registry, static manifests, and eager Nix modules at
1, 15, 50, 100, and 200 synthetic packages. Measure shell activation,
manifest compilation, list/graph/plan, selected provider activation, and no-op
task dispatch.

### Package-author usability

Give a developer unfamiliar with workspace internals a small repository and
ask them to add build, test, and a two-parameter launch. Target the first
successful build and launch within 30 minutes. A simple manifest should remain
under roughly 50 lines and a representative complex one under roughly 150.

If more than approximately 20% of ordinary packages require custom executable
providers, the static contract is too rigid and must be redesigned.

### Failure isolation

Introduce invalid YAML, unsupported schemas, missing dependencies, cycles, and
broken lazy provider modules in an unrelated optional package. Shell entry and
unrelated builds must continue, while validation identifies the bad package.

### Artifact/cache correctness

Test unchanged builds, relevant and irrelevant source edits, public interface
changes, executable mode changes, artifact truncation/tampering, missing
outputs, recipe/adapter/toolchain changes, restored outputs, interruption, and
two concurrent requests for one coordinate.

Consumers rebuild only for changed artifacts they actually consume. A launch's
recommended repair command must produce the exact missing artifact.

### Local override safety

Change a generated Synapse interface and a compiled non-leaf package. Prove
that all affected reverse dependents rebuild or that the system refuses the
mixed graph. Build two CUBS variants concurrently and select them explicitly.

### Launch behavior

Port Electrode simulation and ground station and compose the stack. Test
argument help/validation, include forwarding, quoting/injection, secret
redaction, explicit wiring, port collisions, auto ports, two concurrent
sessions, readiness loss, restart propagation, Ctrl-C, reattachment, and stale
process cleanup.

Changing only runtime values must perform zero Nix evaluation and begin launch
dispatch within the budget.

### Release contamination

Place unique dirty sentinels in source, `build/`, and `devel/`, then build the
locked product. None may appear in the release references, dependency closure,
or output bytes. Remove source checkouts and prove release resource/launch
resolution still works.

If the static/lazy design fails these tests, improve the current central
registry rather than forcing distributed package ownership prematurely.

## Adversarial acceptance scenarios

The design is incomplete until it handles these cases explicitly:

1. A generated Synapse interface changes while a downstream binary was built
   against the previous ABI.
2. Two CUBS boards/configurations build concurrently without overwriting one
   another.
3. An output exists but has changed bytes, mode, type, or symlink target.
4. An Electrode simulator artifact is absent and the suggested command actually
   recreates it.
5. A release launch runs after its source checkout is renamed or removed.
6. A runtime port change starts without Nix reevaluation.
7. Two included launches claim the same fixed port or export ambiguous router
   providers.
8. A foundational router loses readiness or restarts after dependents started.
9. A malformed optional package descriptor does not prevent shell entry or an
   unrelated build.
10. Two terminals cannot silently change one another's build mode mid-command.
11. A selected local build fails and never silently falls back to locked bytes.
12. Every build/test/launch retains exact commands, provenance, timings,
    per-node logs, and a deterministic summary without leaking secrets.

## Reviewer consensus and resolved disagreements

All six reviewers agreed on:

- retaining the hybrid ROS/devenv/Nix direction;
- rejecting eager executable Nix as the universal package manifest;
- using static metadata and lazy selected providers;
- modeling actions, artifacts, and variants rather than packages alone;
- requiring a real resource/artifact index and safe local override semantics;
- keeping runtime launch values out of Nix evaluation;
- adding typed parameters, explicit wiring, preflight, named sessions, and
  multi-instance isolation;
- preserving native incremental builds and process-compose supervision;
- separating qualification tasks from deployable Nix artifacts;
- retaining the central registry as migration scaffold and policy authority.

The main implementation questions were resolved as follows:

- **Who executes the DAG?** `ws` owns static planning and provenance; use
  devenv tasks/native tools for execution first. Build a custom executor only
  if measurement proves the backend inadequate.
- **One manifest per repository or package?** Start with one static repository
  root marker containing explicit logical package roots. Add referenced nested
  descriptor files only if real monorepo pilots demonstrate the need.
- **How strict is metadata during migration?** Enforce graph, artifact, path,
  and launch correctness immediately. Warn first for ownership/license
  completeness, then make them policy gates after migration tooling exists.
- **Can local packages fall back to locked versions?** Only when absent or
  explicitly unselected, never after a selected local action fails.
- **Should every ROS launch feature be reproduced?** No. Implement the
  statically inspectable subset required by CogniPilot products and add event
  semantics only from demonstrated scenarios.

## Final direction

The desired experience is not “every repository contains a Nix file.” It is:

> A developer can discover, plan, build, test, launch, inspect, recover, and
> reproduce any supported CogniPilot workflow without needing to understand
> Nix, devenv, or each repository's native build internals—and an expert can
> still reach the native tools directly.

Static package ownership, artifact-qualified DAGs, a runtime launch/session
model, lazy devenv providers, and a pure Nix release graph provide the clearest
path to that result.

## Primary references

- [ROS 2 package/build-system model](https://docs.ros.org/en/lyrical/Concepts/Advanced/About-Build-System.html)
- [ROS 2 launch substitutions, includes, and argument discovery](https://docs.ros.org/en/rolling/Tutorials/Intermediate/Launch/Using-Substitutions.html)
- [ROS 2 launch event model](https://design.ros2.org/articles/roslaunch.html)
- [ament resource index](https://docs.ros.org/en/rolling/p/ament_cmake_core/doc/resource_index.html)
- [colcon package selectors](https://colcon.readthedocs.io/en/released/reference/package-selection-arguments.html)
- [colcon overriding-package risks](https://colcon.readthedocs.io/en/released/user/overriding-packages.html)
- [colcon per-invocation/package logs](https://colcon.readthedocs.io/en/released/user/log-files.html)
- [colcon test-result summaries](https://colcon.readthedocs.io/en/released/reference/verb/test-result.html)
- [devenv task DAGs, inputs, states, and execution modes](https://devenv.sh/tasks/)
- [devenv process supervision, readiness, restart, and ports](https://devenv.sh/processes/)
- [devenv profiles and composition semantics](https://devenv.sh/profiles/)
- [devenv polyrepo composition](https://devenv.sh/guides/polyrepo/)
