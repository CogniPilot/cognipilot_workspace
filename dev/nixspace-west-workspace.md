# Nixspace West workspace boundary

West remains the semantic authority for a Zephyr manifest. Nixspace does not
parse `west.yml`, flatten imports, restate project URLs or revisions, or build a
second project graph.

The generic flake-parts module is
[`nix/nixspace/west-workspace-module.nix`](../nix/nixspace/west-workspace-module.nix).
An importing product supplies only:

- its product/interface identity;
- one exact Nix input and source-relative `west.yml` resource binding;
- optional editable checkout bindings keyed by native West project name; and
- cache/view policy.

Nix resolves the input to an immutable store path and hashes both the selected
manifest bytes and its exact source identity. The combined content key therefore
covers native West imports and self command files without interpreting them.
Nix emits this as versioned `flake.nixspaceWestPlan` data. The per-system
`packages.nixspace-west-plan` adds the exact native West executable plus
one versioned, exact argv contract that atomically ingests the checkout and
installs its generation GC root.
This plan is self-identifying as `apiVersion = "nixspace/v1"`,
`kind = "WestWorkspace"`, `interfaceVersion = 3`, and contains no West project
list. Nixspace rejects any other transport identity; there is no compatibility
decoder.

The standalone Rust client consumes that plan. It asks native `west list` for
resolved project paths and invokes native `west update --narrow` with West's
own `--path-cache` in a private staging directory. After native validation, it
runs that plan command, which does not return until the resulting immutable
tree has its final generation GC root. Non-overridden projects in the editable
view point directly to that canonical store tree. Immutable checkouts are
addressed by the Nix-emitted manifest hash. By root policy, editable views live
below `.nixspace/state/west/views`, never below `src/.west`, and use only the
local overrides already declared by Nix. These commands and paths are
Nix-emitted data, not constants in the Rust client.

`west exec`, `west run`, and `west path --mode local` hold a shared publication
lease for the selected generation and a distinct exclusive lease for its
editable view. Before executing or exposing that local path, nixspace verifies
the generated West configuration, every managed project link, and the exact
override projection; a changed projection is rebuilt from the sealed tree.
Thus an arbitrary developer command cannot
mutate or race the release checkout, while ordinary build output in the local
view remains persistent. The plan also defines an indexed project-path
environment protocol. Nixspace renders it only for the canonical sealed
project and repository-metadata paths, so Git-based tools can read multi-user
Nix store checkouts without changing user-global configuration or trusting
unrelated repositories.

All persistent relative paths are plan data: immutable generation root,
generation GC-root component, current pointer, publication lock, local
generation root, and local execution lock. The cache namespace is part of each
cache-relative path even when the runtime cache root is overridden. Rust adds
only runtime generation IDs and rejects equal or ancestor-overlapping plan
paths.

The plan also declares a positive retained-generation count. Cleanup is
deliberately explicit: `nixspace west gc` takes the publication lease, retains
the selected generation plus the newest plan-selected history, proves every
candidate is owned by this exact West plan, and only then removes old local
views and generation GC roots. A sync never invalidates a path merely because
another command previously printed it.

The runtime protocol is intentionally small and has no legacy names:

- `NIXSPACE_WEST_PLAN` selects the Nix-generated plan;
- `NIXSPACE_WORKSPACE_ROOT` binds its runtime workspace root;
- `NIXSPACE_WEST_CACHE` optionally relocates the immutable cache; and
- `NIXSPACE_WEST_VIEW_ROOT` optionally relocates editable views.

Pure release builds are a separate concern. They may consume a reviewed,
hash-pinned `west2nix` frozen manifest. Nixspace does not use `west2nix` as a
replacement for native West's editable developer workspace.
