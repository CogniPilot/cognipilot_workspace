{
  config,
  lib,
  python,
  root,
}:

let
  components = config.workspace.components;
  workspaceProfiles =
    (import ./profiles.nix {
      componentNames = builtins.attrNames components;
    }).resolved;
  defaultComponentNames = workspaceProfiles.default;
  hasTask =
    mode: kind: name:
    components.${name}.${mode}.${kind} != null;
  taskComponents =
    mode: kind: lib.filterAttrs (_: component: component.${mode}.${kind} != null) components;

  mkBuildTasks =
    mode:
    lib.mapAttrs' (
      name: component:
      lib.nameValuePair "${mode}:build:${name}" {
        description = "${mode} build: ${component.displayName}";
        cwd = "${root}/${component.path}";
        exec = component.${mode}.build;
        after =
          lib.optionals (mode == "local") (
            map (dependency: "local:build:${dependency}") component.dependencies
          )
          ++ lib.optionals component.needsWest [ "workspace:west:ensure" ];
      }
    ) (taskComponents mode "build");

  mkTestTasks =
    mode:
    lib.mapAttrs' (
      name: component:
      lib.nameValuePair "${mode}:test:${name}" {
        description = "${mode} test: ${component.displayName}";
        cwd = "${root}/${component.path}";
        exec = component.${mode}.test;
        after = lib.optional (component.${mode}.build != null) "${mode}:build:${name}";
      }
    ) (taskComponents mode "test");

  buildLeaves =
    mode:
    map (name: "${mode}:build:${name}") (builtins.filter (hasTask mode "build") defaultComponentNames);
  testLeaves =
    mode:
    map (name: "${mode}:test:${name}") (builtins.filter (hasTask mode "test") defaultComponentNames);
in
(mkBuildTasks "local")
// (mkBuildTasks "release")
// (mkTestTasks "local")
// (mkTestTasks "release")
// {
  "workspace:west:validate" = {
    description = "Validate that available app manifests have a conflict-free pinned union";
    cwd = root;
    exec = ''
      set -euo pipefail
      ${python}/bin/python "${root}/scripts/workspace-west.py" validate
    '';
  };

  "workspace:west:ensure" = {
    description = "Materialize the shared west workspace if its pins are not ready";
    cwd = root;
    exec = ''
      set -euo pipefail
      ${python}/bin/python "${root}/scripts/workspace-west.py" ensure
    '';
    after = [ "workspace:west:validate" ];
  };

  "workspace:build:local" = {
    description = "Build the complete local dependency graph";
    exec = "echo 'local workspace graph built'";
    after = buildLeaves "local";
  };

  "workspace:build:release" = {
    description = "Build every component with its published/pinned dependencies";
    exec = "echo 'release workspace graph built'";
    after = buildLeaves "release";
  };

  "workspace:test:local" = {
    description = "Test the complete local dependency graph";
    exec = "echo 'local workspace graph tested'";
    after = testLeaves "local";
  };

  "workspace:test:release" = {
    description = "Test every component in release dependency mode";
    exec = "echo 'release workspace graph tested'";
    after = testLeaves "release";
  };
}
