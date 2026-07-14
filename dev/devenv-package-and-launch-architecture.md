# Package-owned devenv architecture

Status: initial proposal, materially revised by
`dev/adversarial-devenv-direction-review.md`, 2026-07-13. Do not implement this
proposal unchanged: the six-agent review replaces eager executable package Nix
with static manifests and lazy providers, and replaces package-level build and
launch requirements with artifact/variant-aware plans.

This document proposes ROS-like package semantics for the CogniPilot workspace
without forcing every repository into a ROS 2 filesystem layout or deployment
model. The central idea is:

> Enforce a small, typed package interface; let each package keep the internal
> layout and native build system that fit it.

The recommended direction is **ROS package semantics with devenv/Nix
mechanics**, not conversion of the whole workspace to ROS 2.

## Decision summary

- Every participating repository has one required root marker,
  `cognipilot.nix`.
- That marker is a constrained Nix descriptor implementing a versioned, typed
  package contract. It may declare one or more logical packages in the
  repository.
- Package source, build files, tests, launch modules, generated code, and
  executables may live anywhere below that repository. Their locations are
  explicit data in the contract; no `src/`, `include/`, `launch/`, or test tree
  is imposed.
- Packages own their local build/test recipes and package-specific launches.
- The workspace owns only repository acquisition and pinning, product/profile
  composition, cross-package launch bundles, policy, and contract validation.
- `ws build` continues to use fast native incremental paths. It never runs CI,
  release packaging, or launches implicitly.
- `ws launch` supervises already-built development artifacts. It never builds
  implicitly; `--build` may be added later as an explicit opt-in convenience.
- `devel/` remains the automatically activated mutable local overlay. There is
  no workspace `install/` directory. Deployments consume locked Nix store
  outputs and never consume `devel/`.
- Developers enter the workspace through `devenv shell` or automatic direnv
  activation. They do not source a generated setup script.

## Why this boundary fits this workspace

The current workspace has genuine layout diversity:

- Zephyr/CMake/west firmware (`cerebri_cubs2`, `cerebri_modules`, `csyn`)
- Rust and npm monorepos (`electrode_web`, `rumoca`)
- generated multi-language interfaces (`synapse_fbs`)
- Modelica packages (`modelica_models`)
- Rust bridges with simple Cargo layouts
- Meson, Python, QEMU, and project-specific setup (`FastDyn`)

Those are not accidental inconsistencies that a single directory convention
would fix. They are different kinds of software. A ROS-style static manifest
is valuable, but forcing all of them into ament conventions would add wrappers
and duplicated metadata without improving their native development loops.

The current central `nix/components/default.nix`, however, knows too much about
the internals of every repository: native commands, generated paths, CMake and
west details, npm behavior, and release commands. The root `launch/` directory
also owns launches that naturally belong to Electrode and the Qualisys bridge.
That makes the workspace root a coordination bottleneck and makes package
changes easier to forget or duplicate.

The proposed contract moves that knowledge to its owner while retaining a
central view of the complete product.

## Package and repository are different concepts

A **repository** is a source-control and pinning unit. The root workspace must
know enough about a repository to clone or select it before any local package
manifest exists.

A **package** is a logical build, dependency, launch, and release unit. One
repository usually declares one package, but monorepos may declare several.
Unlike colcon, packages do not need to be recursively discovered as independent
directory trees and may be nested when explicitly declared.

For example:

```text
src/electrode_web/
├── cognipilot.nix       # required repository marker and package declarations
├── Cargo.toml           # existing native layout remains intact
├── package.json
├── apps/
├── crates/
├── packages/
└── nix/                 # optional location chosen by this repository
    └── launch/
        ├── ground-station.nix
        └── simulation.nix
```

Only `cognipilot.nix` has a fixed name and location. Even the `nix/launch/`
location above is an example, not a rule.

## Responsibilities

| Concern | Package/repository owns | Workspace owns |
| --- | --- | --- |
| Source acquisition | Repository identity | URL, branch/revision policy, private access, profile selection |
| Package identity | Name, metadata, schema version | Global uniqueness and naming policy |
| Native layout | All internal paths | Nothing beyond the root marker |
| Build/test | Native commands, inputs, required outputs, toolchain adapter | Dependency ordering, caching framework, policy checks |
| Dependencies | Declared package relationships by kind | Resolution, cycle detection, selected-source closure |
| Launch | Package-specific processes and requirements | Namespacing, CLI, product-wide launch bundles |
| Local artifacts | Declared development outputs | Automatic `devel/` overlay and cache state |
| Release | Exact Nix output or release builder | Clean/pinned policy, promotion and deployment metadata |
| CI | Package-specific exhaustive checks | Required organization-wide gates |

The repository catalog cannot disappear entirely: the workspace must know how
to acquire a repository that has not yet been checked out. It should become a
small source catalog rather than a second package build system.

## Proposed package contract

The shared workspace defines a custom devenv/Nix module with typed options
under `cognipilot.packages`. Each repository's `cognipilot.nix` returns a
constrained definition that the shared module validates and compiles into those
options. The root should not import the descriptor wholesale as arbitrary
devenv configuration; referenced development and launch modules activate only
when their package/task/launch is selected. A representative descriptor would
look like this:

```nix
{ ... }:
let
  repositoryRoot = ./.;
in
{
  cognipilot.packages.electrode_web = {
    schemaVersion = 1;
    root = repositoryRoot;

    metadata = {
      displayName = "Electrode";
      description = "CogniPilot ground station and simulation services";
      license = "Apache-2.0";
      maintainers = [ "flight-software" ];
    };

    dependencies = {
      source = [ "synapse_fbs" ];
      build = [ "synapse_fbs" ];
      runtime = [ ];
      test = [ ];
    };

    development = {
      adapter = "cargo-npm";
      module = ./nix/development.nix;

      tasks.build = {
        cwd = repositoryRoot;
        exec = "cargo xtask workspace-build";
        inputs = [ "Cargo.toml" "Cargo.lock" "package.json" "apps" "crates" ];
        outputs = [ "target/debug/electrode-ground-station" ];
      };

      tasks.test = {
        cwd = repositoryRoot;
        exec = "cargo test --workspace";
      };
    };

    launches = {
      ground-station = {
        description = "Run the local Electrode ground station";
        requiresBuild = [ "electrode_web" ];
        module = ./nix/launch/ground-station.nix;
      };

      simulation = {
        description = "Run the Electrode fake simulation";
        requiresBuild = [ "electrode_web" ];
        module = ./nix/launch/simulation.nix;
      };
    };

    release = {
      deployable = true;
      kind = "flake-output";
      output = "electrode_web";
    };
  };
}
```

This is illustrative syntax; Phase 1 should finalize the option names in tests
before repositories adopt it. The essential properties are more important than
the spelling:

- the contract is typed and versioned;
- paths are relative to an explicit package/repository root;
- native commands and output knowledge live with the package;
- launches are ordinary devenv modules referenced by the package;
- local development and deployable release outputs are separate fields;
- no package is forced to have its own flake, CMake project, Cargo workspace,
  `src/`, `include/`, or `launch/` directory.

### Required fields

Every package should declare:

- `schemaVersion`
- stable package name (the attribute name)
- `root`
- display name, description, license, and owning team/maintainers
- dependency sets, even when empty
- a development build task or an explicit `buildable = false`
- a focused test task or an explicit reason it is unavailable
- release status: a deployable Nix output, `libraryOnly`, or `notDeployable`

Package IDs should use lowercase letters, digits, and underscores, begin with a
letter, and be globally unique. This matches most current target names and is
familiar to ROS users. `FastDyn` should gain the canonical ID `fastdyn`, with a
temporary `FastDyn` CLI alias during migration.

### Dependency kinds

One undifferentiated dependency list is not sufficient:

- `source`: another local repository's source is read or composed directly;
- `build`: another local package must produce development artifacts first;
- `runtime`: a released package must be present in the deployed closure;
- `test`: required only for focused package testing.

Toolchain dependencies from nixpkgs are declared in the package's development
module rather than pretending that they are workspace packages. Product
profile membership is workspace policy and is not self-selected by packages.

### Build adapters without layout enforcement

An `adapter` is a typed behavior preset, not a directory convention. Initial
adapters can cover `cargo`, `cargo-npm`, `cmake`, `zephyr`, `nix`, and `custom`.
Adapters provide defaults and stronger checks while permitting explicit
overrides.

For example, the Zephyr adapter can require a board/configuration declaration
and can select the guarded CMake/Ninja hot path after west configuration. It
must not require every Zephyr project to place its boards or modules in the
same directories. The custom adapter is important for projects such as
FastDyn; it should require more explicit inputs and outputs rather than being a
validation escape hatch.

### Standalone and workspace development

The same descriptor must work in two contexts:

- the root workspace compiles all selected package descriptors into one graph;
- a repository-local optional `devenv.nix` compiles only that repository's
  descriptor and its available dependencies for standalone development.

That prevents the package contract from becoming useful only inside this one
super-workspace. A package may pin/import the shared CogniPilot contract module
for standalone use, while the root workspace supplies its own pinned module
when composing repositories. Both paths validate the same schema and expose
the same package tasks. Packages without a standalone `devenv.nix` still work
normally inside the root workspace.

## Package-owned launches

A package launch is a devenv module that describes processes, readiness,
restart behavior, environment, and dependencies. Package launches are
qualified globally as `<package>/<launch>`:

```sh
ws launch electrode_web/ground-station
ws launch electrode_web/simulation
ws launch synapse_qualisys_bridge/mocap
```

For ROS-like ergonomics, the CLI may also accept the two-argument form:

```sh
ws launch electrode_web ground-station
```

Internally, process identifiers must be namespaced, for example
`electrode_web.ground-station`, so independently developed packages cannot
silently replace one another's processes.

Package launch modules are dormant until selected. The contract compiler turns
them into generated devenv profiles, and `ws launch` selects the corresponding
profile before invoking devenv's process supervisor. This preserves native
devenv process readiness, restart, dependency, log, and lifecycle behavior.

Launch definitions may also declare typed arguments, includes, and defaults.
For example, a mocap launch can expose `host` and `port` without hard-coding a
shell interface. `ws launch PACKAGE/LAUNCH --set host=...` validates values and
maps them to the module options or environment expected by its processes.
Bundles can supply or forward arguments when including a package launch.
Secrets remain references to the workspace secret mechanism, not defaults in
the descriptor.

### Package launches versus product bundles

Packages own processes they can meaningfully run and test. The root workspace
owns compositions whose purpose is a complete product or integration scenario.

The current launch definitions should move as follows:

| Current launch | New owner | Reason |
| --- | --- | --- |
| `simulation` | `electrode_web` package | Runs an Electrode binary and uses its artifacts |
| `ground-station` | `electrode_web` package | Runs Electrode binary and web assets |
| `mocap` | `synapse_qualisys_bridge` package | Runs the package's bridge binary |
| `simulation-stack` | root workspace bundle | Composes launches from multiple packages |

The root bundle becomes declarative rather than reimplementing package
processes:

```nix
cognipilot.launchBundles.simulation-stack = {
  includes = [
    "electrode_web/simulation"
    "electrode_web/ground-station"
    "synapse_qualisys_bridge/mocap"
  ];
};
```

Inclusion order alone must not define startup order. Package launch modules
declare explicit readiness and process dependencies; the contract compiler
validates all referenced process names after composition.

### Launch/build boundary

Launch remains out of the build hot path and build remains out of launch:

- `ws build PACKAGE` builds the package and its declared build dependencies;
- `ws launch PACKAGE/LAUNCH` checks `requiresBuild` outputs and starts them;
- a missing artifact produces the exact build command and exits;
- launch does not fetch repositories, build, test, package, or run CI;
- `ws launch --build ...`, if added, is visibly explicit and still uses the
  incremental development graph.

This keeps failures understandable and prevents a routine launch from turning
into a long, surprising rebuild.

## Discovery, validation, and generated views

Discovery should be deterministic. The workspace walks its selected repository
catalog, checks each expected repository root for `cognipilot.nix`, and imports
that module. It should not recursively import arbitrary Nix files from `src/`.

The Nix module system should reject invalid package graphs during evaluation.
`ws package validate` exposes the same checks directly and in CI.

Required validation includes:

- supported schema version;
- globally unique, policy-compliant package names and aliases;
- repository catalog expectations match declared package names;
- descriptor, source, and referenced module paths remain within the declaring
  repository and exist when required;
- all dependency references resolve and the build graph is acyclic;
- selected profiles include the required source closure;
- all buildable packages declare commands, inputs, and required outputs;
- development outputs stay below approved mutable state roots;
- deployable releases resolve to Nix outputs and never reference `devel/`;
- package launch names and qualified process names are unique;
- launch modules and included launches exist;
- cross-process dependencies resolve after bundle composition.

Useful package-oriented commands are:

```sh
ws package list
ws package info electrode_web
ws package validate
ws graph cerebri_cubs2
ws build --packages-up-to cerebri_cubs2
ws test electrode_web
ws launch list
ws launch list electrode_web
```

The evaluated module graph can emit a JSON manifest for fast CLI listing,
completion, benchmarks, and diagnostics. Nix evaluation remains the source of
truth; generated JSON is a cache/view, never hand-edited metadata.

## Local development, overlays, and deployment

The word “install” is especially easy to misunderstand here, so the layers are
deliberately distinct:

| Layer | Mutable? | Automatically visible? | Deployment input? | Purpose |
| --- | --- | --- | --- | --- |
| Package source | Yes | Yes | Only through a clean locked release build | Developer edits |
| `build/` | Yes/disposable | Build-system-specific | No | Compiler and generator intermediates |
| `devel/` | Yes | Yes, inside the workspace shell | Never | Local package overlay and generated development artifacts |
| Nix store release output | No/content-addressed | Only when explicitly selected | Yes | Fixed, reproducible product artifact |
| `release-results/` | Symlinks only | No | No; convenience view only | Human-accessible links to selected store outputs |

Development dependency resolution behaves like an automatically managed
overlay:

```text
selected local package/devel output
            ↓ shadows the same package ID
locked CogniPilot package set in the Nix store
            ↓
locked nixpkgs/toolchain inputs
```

In local mode, a selected and checked-out package is built from its working
tree and shadows the fixed package of the same ID. Its declared local build
dependencies do the same recursively. A dependency that is intentionally not
local may come from the locked package set when the package contract permits
that fallback. The resolution and provenance must be inspectable with
`ws package info`/`ws doctor` so a developer never has to guess which copy won.

Release mode has no local-shadowing rule. It resolves every package from the
release lock/pin set, constructs immutable Nix outputs, and records package
revisions and output digests in the promoted product manifest. Any reference to
`devel/`, a working-tree artifact, or an unapproved dirty source is an error.
This is how developers can freely overlay local packages without making a
locally built artifact look deployable.

Modern colcon normally creates `build/`, `install/`, and `log/`; it does not
use the ROS 1 catkin `devel/` model. `--symlink-install` makes iteration faster
by placing symlinks in the install prefix, and an overlay is activated by
sourcing its generated setup script. In this workspace, `devel/` represents
the useful mutable overlay idea, while the Nix store provides the actual
installed/release identity. Creating another workspace `install/` tree would
duplicate concepts and invite accidental deployment of local state.

Developers should only need:

```sh
direnv allow                 # once, if using direnv
ws profile default
ws sync all
ws build cerebri_cubs2
ws launch electrode_web/ground-station
```

No generated shell script is sourced. The devenv shell exports the active
overlay paths deterministically. Package-specific shells remain available via
`ws shell PACKAGE` for unusual toolchains, but ordinary package builds select
their required development module automatically.

## Current layout versus a fully ROS 2 layout

“Fully ROS 2” here means converting workspace components into discoverable ROS
packages with `package.xml`, supported ament build types, colcon package
discovery/build/test, installed launch files, and sourced install overlays. ROS
2 itself permits more than one build type, but the standard package contract
and installation/discovery conventions still shape the repository.

| Dimension | Current CogniPilot layout | Fully ROS 2 layout |
| --- | --- | --- |
| Package boundary | Mostly one root component per Git repository | One or more recursively discovered ROS packages, each with `package.xml` |
| Internal structure | Native and highly varied | Conventional package roots; ament CMake/Python have expected files and install rules |
| Metadata | Central Nix component registry plus native manifests | Standard `package.xml` metadata and dependency tags |
| Build orchestration | `ws`, devenv tasks, native incremental tools, Nix release mode | colcon selects/orders packages and invokes declared build types |
| Dependency graph | Central source/build declarations, plus native manifests | Package manifests form a standardized package graph |
| Launch ownership | Currently centralized at workspace root | Launch descriptions normally installed and discovered with their package |
| Discovery | Explicit central component catalog | Recursive package discovery with ignore markers |
| Development overlay | Automatic mutable `devel/` view in devenv shell | `install/` overlay, often symlinked, activated by sourcing setup files |
| Release/deployment | Pinned content-addressed Nix outputs | Installed package prefixes; reproducible deployment/version promotion needs additional release policy/tooling |
| Monorepo flexibility | High; arbitrary nested language ecosystems | Multiple packages are supported, but nested ROS package discovery is restricted and each logical package needs a manifest |
| Hot path | Can use native Cargo/npm/CMake/Ninja/west behavior directly | colcon adds a uniform orchestration layer; symlink install helps interpreted/static resources |
| Ecosystem integration | Custom CogniPilot tools | Mature ROS package, index, launch, dependency, release, and community tooling |
| Non-ROS components | First-class | Usually require ament wrappers, package manifests, install rules, or separation from the ROS workspace |

### Strengths of the current layout

- It treats firmware, generated interfaces, web applications, Modelica, Rust,
  and simulation infrastructure as first-class native projects.
- Nix locks and immutable outputs provide a strong development/release boundary
  and a precise deployment identity.
- The optimized hot paths can call native incremental systems directly; current
  unchanged targets complete in seconds rather than rerunning CI workflows.
- Developers receive overlay paths automatically from devenv/direnv rather
  than managing sourced overlay order.
- Existing monorepos do not need to be split or wrapped merely to satisfy a
  workspace tool.

### Weaknesses of the current layout

- Package ownership is weak: root configuration contains package-specific
  commands, paths, outputs, and launch details.
- Adding or changing a component often requires coordinated edits in the root
  repository and its native repository.
- There is no uniform package manifest at the repository boundary and no
  package-local launch discovery.
- Dependency metadata can drift between native manifests, flakes, and the
  central registry.
- The component abstraction is usually repository-sized, which is too coarse
  for some monorepos and product capabilities.
- Validation and developer introspection are workspace-specific and currently
  less mature than colcon/ament tooling.

### Strengths of a fully ROS 2 layout

- A widely understood `package.xml` contract makes package metadata and
  dependencies discoverable by standard tools.
- colcon supports package selection, dependency ordering, parallel builds,
  isolated install prefixes, and package-level testing.
- Package-owned installed launches are standard, discoverable, composable, and
  supported by XML, YAML, and Python launch descriptions.
- The ROS/ament index, rosdep, release tooling, and community conventions make
  ROS interoperability much easier.
- Isolated install layouts are useful for exposing undeclared dependencies and
  deleting one package's artifacts independently.

### Weaknesses of a fully ROS 2 conversion here

- Many components are not ROS middleware packages. Requiring ament manifests,
  install rules, resource-index entries, and wrappers would add ceremony with
  little local value.
- Electrode, Rumoca, Synapse, Zephyr projects, Modelica models, and FastDyn do
  not naturally share one canonical package tree. Some would need artificial
  splitting or wrapper packages.
- colcon's recursive package model does not allow nested packages, which is a
  poor fit for arbitrary existing monorepo nesting.
- Install overlay ordering and sourced setup scripts reintroduce shell-state
  behavior that the current devenv shell deliberately avoids.
- A mutable colcon install prefix is not equivalent to a locked,
  content-addressed Nix deployment. Additional release controls would still be
  necessary.
- Standardizing orchestration does not automatically optimize language-specific
  hot paths; an ament/colcon wrapper can obscure or duplicate native caching if
  designed poorly.
- It would make ROS an architectural dependency of tools, firmware, generated
  libraries, and web projects that do not need it.

## Recommended hybrid: strengths and tradeoffs

The proposed package contract preserves the strongest parts of both models:

- ROS-like package self-description, dependency graphs, package-owned
  launches, validation, names, and discovery;
- devenv composition, profiles, typed custom modules, automatic environments,
  and native process supervision;
- Nix pinning and immutable release/deployment outputs;
- native build systems and layouts for fast local iteration.

Its cost is that CogniPilot owns a small ecosystem: schema evolution, adapters,
CLI views, validation, and launch namespacing. Nix modules are also more
powerful and less portable than static XML. This should be managed by keeping
the contract small, versioned, well tested, and capable of emitting a static
JSON view. ROS-native repositories should use an adapter that reads or checks
their `package.xml` rather than duplicating its facts manually.

## Migration roadmap

The migration should be additive and preserve current command behavior until a
package has been verified against the new contract.

### Phase 0: establish a clean implementation base

- Resolve the existing `nix/tasks.nix` merge conflict before changing the task
  graph. This design document deliberately does not choose either side.
- Preserve the current benchmark records and warm-build budgets as regression
  gates.

### Phase 1: define and test the shared contract

- Add `nix/modules/cognipilot-package.nix` with typed options and assertions.
- Define schema version 1 and adapter interfaces.
- Add fixture-based Nix tests for valid and invalid descriptors.
- Add `ws package list`, `info`, and `validate` backed by an evaluated JSON
  manifest.
- Keep `nix/components/default.nix` authoritative during this phase.

Acceptance criteria:

- duplicate names, bad dependency references, cycles, invalid output paths,
  process collisions, and release references to `devel/` fail evaluation;
- evaluating the package view does not run builds, tests, network operations,
  or CI;
- workspace shell entry and existing warm-build budgets do not regress.

### Phase 2: pilot package ownership

- Add a descriptor to `electrode_web` and move its `simulation` and
  `ground-station` modules into that repository without changing their process
  behavior.
- Add a descriptor to one small Cargo bridge to validate the simple path.
- When the Qualisys repository is available, add its descriptor and move
  `mocap` there.
- Import descriptors alongside legacy definitions and assert that dependency,
  task, and output facts agree. Do not maintain two silent sources of truth.

Acceptance criteria:

- old launch aliases still work;
- new qualified package launches work;
- missing launch artifacts still produce exact `ws build` guidance;
- cold/warm results stay within current target budgets.

### Phase 3: migrate build/test ownership

- Move native development build/test recipes and required-output declarations
  from the root registry to package descriptors one package at a time.
- Implement adapters in this order: Cargo, Cargo/npm, Zephyr/CMake, Nix, custom.
- Retain the existing task cache and guarded CMake/Ninja paths as shared
  framework behavior.
- Keep CI/release commands explicitly separate from development tasks.

Acceptance criteria:

- `ws build TARGET` has the same or smaller dependency closure and no hidden CI;
- the package descriptor is the only source of package-internal paths and
  commands after that package migrates;
- every migrated target passes repeated-build budget checks.

### Phase 4: make product launches declarative

- Replace root process implementations with includes of qualified package
  launches.
- Retain root-owned bundles such as `simulation-stack`.
- Validate readiness and dependency edges after composition.
- Add launch evaluation tests that do not start processes.

### Phase 5: reduce the central registry to policy

The final central entry for a repository should contain only facts needed
before checkout or facts that are intentionally workspace policy:

```nix
electrode_web = {
  path = "src/electrode_web";
  github = "CogniPilot/electrode_web";
  branch = "main";
  private = false;
  expectedPackages = [ "electrode_web" ];
};
```

Product profiles continue to choose package/repository sets centrally. All
package-internal build, test, artifact, and launch definitions live with their
packages.

### Phase 6: ROS interoperability where it is useful

- Add a `ros2` adapter for actual ROS packages such as the ROS bridge.
- Treat `package.xml` as authoritative for ROS dependency metadata and check it
  against the CogniPilot contract rather than copying fields by hand.
- Allow ROS package launch descriptions to be wrapped as package launches when
  needed, without requiring non-ROS packages to adopt ROS.

## Guardrails against repeating the current problems

- A new package is not accepted only because it has a descriptor; it must pass
  contract validation and declare local versus release behavior explicitly.
- The root workspace must not regain package-specific source paths or native
  commands through “temporary” launch or task helpers.
- A package launch must not trigger `ws build`, `ws sync`, network access, or
  tests as a side effect.
- Package descriptors must not add their whole toolchain to the default shell.
  Toolchains activate through selected development profiles/tasks.
- Release evaluation must reject mutable overlay paths and dirty/unpinned
  package inputs according to release policy.
- Schema changes require compatibility fixtures and a documented migration;
  packages never silently reinterpret old fields.
- Performance benchmarks remain package-contract acceptance tests, not just
  informal observations.

## References

- [ROS 2 build system and package manifests](https://docs.ros.org/en/lyrical/Concepts/Advanced/About-Build-System.html)
- [ROS 2 package structure tutorial](https://docs.ros.org/en/rolling/Tutorials/Beginner-Client-Libraries/Creating-Your-First-ROS2-Package.html)
- [ROS 2 launch formats](https://docs.ros.org/en/rolling/How-To-Guides/Launch-file-different-formats.html)
- [ROS 2 package development and installing launch files](https://docs.ros.org/en/rolling/How-To-Guides/Developing-a-ROS-2-Package.html)
- [colcon workspace layout](https://colcon.readthedocs.io/en/released/user/what-is-a-workspace.html)
- [colcon isolated versus merged workspaces](https://colcon.readthedocs.io/en/released/user/isolated-vs-merged-workspaces.html)
- [ROS 2 colcon tutorial and symlink install](https://docs.ros.org/en/rolling/Tutorials/Beginner-Client-Libraries/Colcon-Tutorial.html)
- [devenv tasks](https://devenv.sh/tasks/)
- [devenv processes](https://devenv.sh/processes/)
- [devenv composition through imports](https://devenv.sh/composing-using-imports/)
- [devenv custom modules](https://devenv.sh/extending/)
- [devenv profiles](https://devenv.sh/profiles/)
