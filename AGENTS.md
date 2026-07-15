# Workspace development rules

- Do not implement workspace discovery, schema validation, graph planning,
  build orchestration, launch orchestration, compliance, or release tooling in
  Python.
- Do not write workspace tests, fixtures, or test harnesses in Python. Use Nix
  checks for static semantics and Rust tests for `nixspace` behavior.
- Nix modules and flake outputs are the sole static semantic authority.
- Use generated devenv tasks for editable actions, west for Zephyr workspace
  operations, and the devenv process manager for supervision. Use the
  Nix-built Rust client only as a typed cross-platform client over
  Nix-generated data and those established tools.
- Checked-in shell is limited to the smallest bootstrap needed before the
  Nix-built Rust client is available. Do not implement orchestration in Bash or
  hide substantial Bash programs inside Nix strings.
- The Rust client is the reusable `nixspace` Cargo package and `nixspace`
  binary. It must remain independently locked, publishable, and installable
  with `cargo install nixspace`; `ws` is only a CogniPilot convenience alias.
  Rust must not hard-code CogniPilot package semantics, environment names,
  state paths, or flake output names. Those enter through a versioned generic
  Nix workspace interface and explicit CLI configuration.
- `nixspace` must not link workspace project crates, use path/Git dependencies
  into `src/`, import project-generated code, or join a project Cargo
  workspace. It may invoke project-exported apps and external tools through
  versioned data and process boundaries, like a typed replacement for shell
  orchestration.
- Project-native Python is allowed under `src/` when an upstream package or
  ecosystem requires it; the workspace may provision and invoke that tool but
  must not move its own control plane into Python.
- Do not add compatibility fallbacks during a cutover. Delete the replaced
  implementation and its tests in the same change.
