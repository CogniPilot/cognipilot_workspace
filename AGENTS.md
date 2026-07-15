# Workspace development rules

- This repository is a canonical Devenv project. Use `devenv.nix`, ordinary
  imported Devenv modules, profiles, tasks, processes, outputs, tests, and
  Cachix integration directly.
- Do not add a workspace client, alternate command wrapper, generated workspace
  index, package schema, custom task runner, launch/session protocol, or second
  dependency graph.
- Devenv owns development task dependencies and process supervision. West owns
  repository and Zephyr workspace operations. Cargo, npm, CMake, Meson, colcon,
  Nix project flakes, and native package managers own project behavior.
- Project repositories remain editable below `src/`. Do not copy editable
  workspaces or mutable `target`, build, or node-module directories into Nix
  derivations.
- Project flakes may provide immutable packages and project-owned applications.
  The root workspace may invoke them, but must not duplicate their build logic.
- Each Zephyr application owns its West manifest and an isolated West
  workspace. Do not create shared root `zephyr/`, `modules/`, or `models/`
  trees, or add a root West manifest or `.west/` workspace.
- Do not implement workspace control, tests, fixtures, or release orchestration
  in Python. Project-native Python remains allowed inside project repositories.
- Keep shell in `setup` and Devenv task bodies short and declarative. Do not hide
  a substantial program in Bash or a Nix string.
- Do not add compatibility aliases or retain replaced implementation. Complete
  cutovers delete the old code, tests, state paths, environment variables, and
  documentation in the same change.
- Create commits with DCO sign-off (`git commit -s`). Do not add AI co-author
  attribution or `Co-authored-by` trailers naming an AI tool or model.
