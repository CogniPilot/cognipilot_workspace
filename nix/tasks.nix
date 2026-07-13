{
  config,
  lib,
  pkgs,
  python,
  root,
}:

let
  components = import ./components.nix { inherit root; };
  workspaceProfiles = (import ./profiles.nix).resolved;
  componentNames = builtins.attrNames components;
  defaultComponentNames = workspaceProfiles.default;
  profileNames = builtins.filter (profile: profile != "default") (
    builtins.attrNames workspaceProfiles
  );
  profileComponentNames = profile: workspaceProfiles.${profile};

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
          ++ lib.optionals (component.needsWest or false) [ "workspace:west:ensure" ];
      }
    ) components;

  mkTestTasks =
    mode:
    lib.mapAttrs' (
      name: component:
      lib.nameValuePair "${mode}:test:${name}" {
        description = "${mode} test: ${component.displayName}";
        cwd = "${root}/${component.path}";
        exec = component.${mode}.test;
        after = [ "${mode}:build:${name}" ];
      }
    ) components;

  buildLeaves = mode: map (name: "${mode}:build:${name}") defaultComponentNames;
  testLeaves = mode: map (name: "${mode}:test:${name}") defaultComponentNames;
  buildProfileLeaves =
    mode: profile: map (name: "${mode}:build:${name}") (profileComponentNames profile);
  testProfileLeaves =
    mode: profile: map (name: "${mode}:test:${name}") (profileComponentNames profile);
  mkProfileTasks = profile: {
    "workspace:build:local:${profile}" = {
      description = "Build the local ${profile} profile graph";
      exec = "echo 'local ${profile} workspace graph built'";
      after = buildProfileLeaves "local" profile;
    };
    "workspace:build:release:${profile}" = {
      description = "Build the release ${profile} profile graph";
      exec = "echo 'release ${profile} workspace graph built'";
      after = buildProfileLeaves "release" profile;
    };
    "workspace:test:local:${profile}" = {
      description = "Test the local ${profile} profile graph";
      exec = "echo 'local ${profile} workspace graph tested'";
      after = testProfileLeaves "local" profile;
    };
    "workspace:test:release:${profile}" = {
      description = "Test the release ${profile} profile graph";
      exec = "echo 'release ${profile} workspace graph tested'";
      after = testProfileLeaves "release" profile;
    };
  };
  profileTasks = lib.foldl' (tasks: profile: tasks // mkProfileTasks profile) { } profileNames;
in
(mkBuildTasks "local")
// (mkBuildTasks "release")
// (mkTestTasks "local")
// (mkTestTasks "release")
// profileTasks
// {
  "workspace:west:validate" = {
    description = "Validate that available app manifests have a conflict-free pinned union";
    cwd = root;
    exec = ''
      set -euo pipefail
      ${python}/bin/python ${../scripts/workspace-west.py} validate
    '';
  };

  "workspace:west:ensure" = {
    description = "Materialize the shared west workspace if its pins are not ready";
    cwd = root;
    exec = ''
      set -euo pipefail
      ${python}/bin/python ${../scripts/workspace-west.py} ensure
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
