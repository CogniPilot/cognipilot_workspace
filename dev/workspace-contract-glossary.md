# CogniPilot workspace contract glossary

Status: interface v1 terminology, 2026-07-14.

These terms are normative in the Nix module, normalized index, `ws` CLI,
diagnostics, and documentation. A project integration may add domain-specific
words, but it must not redefine these.

- **Repository**: one source-control acquisition unit. Its stable
  `repositoryId` is independent of package and product names.
- **Project integration**: the selected in-tree, external, or fork-owned flake
  module that interprets a repository. Exactly one complete integration owns a
  project definition; definitions are never merged field by field.
- **Package**: an independently selectable logical ownership/version unit. Its
  public `packageId` and globally unique aliases may differ from the internal
  project-integration key.
- **Target**: an independently buildable output family within a package.
- **Variant**: a finite board, platform, feature, or configuration dimension of
  a target. A complete target coordinate includes its effective variant set.
- **Action**: a build, generation, test, or other operation in a target DAG.
  Native tools do the work; devenv supplies task scheduling and JSON task I/O.
- **Artifact**: a typed, content-proven action output consumed by another
  action, target, or launch. Artifact coordinates use
  `package:target:artifact`; normalized outputs name one producing action and
  normalized build inputs name every consuming action.
- **Resource**: named configuration, data, model, schema, or plugin content.
  Resources are looked up by identity rather than inferred from directory
  layout.
- **Executable export**: a named executable artifact plus argv-safe defaults.
  It is data for planning, not a shell command string.
- **Product**: a root-owned repository/package selection and wiring policy.
  Products are selected by the committed root flake composition, not mutable
  CLI state.
- **Profile**: reserved for a project-native or devenv environment profile. A
  profile never selects repositories and is not a synonym for a launch.
- **Launch**: a reusable declarative process description with typed inputs,
  explicit artifacts/resources, wiring, readiness, and lifecycle policy.
- **Process**: one supervised executable within a resolved launch.
- **Session**: one named running instance of a resolved launch, including its
  assigned ports/resources, logs, state, and supervisor connection.
- **Qualification output**: evidence from tests, simulation, or mutable shell
  builds. It is not deployable merely because it was produced by Nix tooling.
- **Deployable output**: an immutable Nix store product reachable from a
  committed product lock and free of mutable workspace references.
- **Local provenance**: an explicitly selected editable worktree/artifact
  generation for one command.
- **Locked provenance**: an immutable source/output selected by the committed
  product flake and lock.

Public CLI and index changes support one interface major at a time. A cutover
deletes the replaced command, state, environment, and definition paths; there
are no compatibility aliases or dual-authority fallbacks.
