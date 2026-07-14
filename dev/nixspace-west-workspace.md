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
`packages.nixspace-west-plan` adds the exact native West executable. This plan
is self-identifying as `apiVersion = "nixspace/v1"`,
`kind = "WestWorkspace"`, `interfaceVersion = 1`, and contains no West project
list. Nixspace rejects any other transport identity; there is no compatibility
decoder.

The standalone Rust client consumes that plan. It asks native `west list` for
resolved project paths, invokes native `west update --narrow` with West's own
`--path-cache`, and creates a product-local editable view. Immutable checkouts
are addressed by the Nix-emitted manifest hash. By root policy, editable views
live below `.devenv/state/nixspace/west/views`, never below `src/.west`, and use
only the local overrides already declared by Nix. These are Nix-emitted paths,
not constants in the Rust client.

The runtime protocol is intentionally small and has no legacy names:

- `NIXSPACE_WEST_PLAN` selects the Nix-generated plan;
- `NIXSPACE_WORKSPACE_ROOT` binds its runtime workspace root;
- `NIXSPACE_WEST_CACHE` optionally relocates the immutable cache; and
- `NIXSPACE_WEST_VIEW_ROOT` optionally relocates editable views.

Pure release builds are a separate concern. They may consume a reviewed,
hash-pinned `west2nix` frozen manifest. Nixspace does not use `west2nix` as a
replacement for native West's editable developer workspace.
