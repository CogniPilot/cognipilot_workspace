# CogniPilot tasks generated from the nixspace index

`nix/cognipilot/devenv-task-module.nix` is the isolated integration seam from
the validated package index to ordinary `devenv.shells.default.tasks`.
`nix/cognipilot/devenv-workspace-module.nix` is the root policy layer: it adds
the generic, independently locked `nixspace` client and Git to every generated
shell and exports normalized launch shells. Enabling the workspace always
enables its generated editable tasks; there is no second registry or migration
switch.

The root composition imports the exact devenv 2.1.2 flake-parts module, the
CogniPilot modules, and the shared workspace module:

```nix
{
  cognipilot.devenvWorkspace = {
    enable = true;
    workspaceRoot = ".";

    # Only exceptions to WORKSPACE/src/REPOSITORY_ID belong here.
    sourceBindings.synapse_fbs_source = "/checkout/synapse_fbs";

  };
}
```

Project definitions must be complete before the product imports them. The root
then has exactly one task graph, generated from the normalized Nix index.

Local checkout locations are root-owned runtime policy. Project definition
modules declare only the normalized source input and package-relative root;
they do not repeat workspace paths. An unbound source defaults to
`WORKSPACE/src/REPOSITORY_ID`, followed by the declared package source root.

## Generated contract

Each normalized action becomes `PACKAGE:TARGET:ACTION`. Its devenv `input` is
an attribute set, which pinned devenv encodes as JSON, containing canonical
package, target, action, adapter, variant, artifact, requirement, and source
coordinate data. Local action dependencies become `after` edges. Artifact
edges are action-qualified: every normalized output has one `producedBy`
action, and every normalized input has a non-empty `consumedBy` action list.
Only a consuming action gets an `after` edge to the exact producer task.

The shared presets keep the ordinary case terse. With no custom actions, an
artifact declaration may omit ownership when the preset has a conventional
`build` action, or when the preset has only one action. The normalized index
still records the resolved action IDs. Adding a custom action makes ownership
ambiguous, so every artifact in that project must declare `producedBy` or
`consumedBy` explicitly. Misspelled action IDs, repeated consumers, duplicate
consumption aliases, contract mismatches, and cycles in the fully qualified
`PACKAGE:TARGET:ACTION` graph fail module evaluation.

Shared commands are intentionally small and conventional:

- Cargo: `cargo build --workspace`, `cargo test --workspace`
- npm: `npm run build`, `npm test`
- CMake: `cmake --build build`, `ctest --test-dir build`
- west: `west build`
- Twister: `west twister`

Bespoke actions use normalized argv and environment values; shell command
strings are not accepted. The generated `exec` is exactly one escaped call to
the absolute Nix-built `nixspace _run-task` executable. Its versioned
`ActionTask` JSON contains the exact cwd, argv, environment overlay, and result.
`nixspace` invokes the child directly and atomically writes the declared result
to `DEVENV_TASK_OUTPUT_FILE` only after success; it does not schedule tasks or
reinterpret the Nix plan.

Each task input contains only the artifact inputs it consumes and outputs it
produces. An artifact input can bind its producer's resolved local output path
to one validated environment name for its consuming actions. Literal values
remain in `environment`; paths remain in `environmentPaths`, so `nixspace`
resolves relative artifact paths against the explicit workspace root before
changing to the consumer cwd. Absolute external source bindings stay absolute.
Nix rejects duplicate names and collisions with the action's static
environment. A docs or test action therefore does not acquire producer edges,
environment bindings, or artifact declarations belonging to a build action.
CPU, memory, and exclusive-lock values remain task input metadata. This layer
does not claim that devenv enforces those requirements.

## Authority boundary

Shared presets cover conventional task shapes. Packages using an xtask,
custom build directory, board/product-specific west arguments, generated
environment, or other semantic delta use an explicit typed argv declaration
and focused execution tests. Native project behavior is not redirected to a
generic command merely because its broad tool family matches.

Targets and their finite variants own literal output paths. Variants that need
different build locations are represented by distinct explicit target/variant
declarations with non-overlapping paths; this layer does not invent a path
template or selector language.

The atomic cutover deleted `devenv.nix`, `nix/tasks.nix`,
`nix/components/default.nix`, the old launch registry, orchestration scripts,
and their focused tests. The generated Nix graph is now the sole authority.
Release qualification and performance results are tracked separately; neither
creates a dormant editable-task fallback.
