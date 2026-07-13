{
  config,
  lib,
  pkgs,
  ...
}:

let
  root = config.git.root;
  components = import ./nix/components.nix { inherit root; };
  workspaceProfileData = import ./nix/profiles.nix;
  workspaceProfiles = workspaceProfileData.resolved;
  repositoryManifest = pkgs.writeText "cognipilot_workspace-repositories.json" (
    builtins.toJSON {
      schema = 1;
      profiles = workspaceProfiles;
      profileDefinitions = workspaceProfileData.definitions;
      components = lib.mapAttrs (name: component: {
        inherit (component) path dependencies repo;
        optional = !(builtins.elem name workspaceProfiles.default);
      }) components;
    }
  );
  python = pkgs.python3.withPackages (
    ps: with ps; [
      pip
      pyyaml
      west
    ]
  );
in
{
  name = "cognipilot_workspace";

  # Keep the always-on shell deliberately small. Each component's flake owns
  # its language/compiler toolchain, so entering this workspace does not fetch
  # Rust, Node, Zephyr SDKs, ROS, or native build stacks pre-emptively.
  packages = with pkgs; [
    bashInteractive
    coreutils
    curl
    findutils
    git
    gnugrep
    gnused
    jq
    python
    util-linux
    which
  ];

  env = {
    COGNIPILOT_WORKSPACE_ROOT = root;
    COGNIPILOT_COMPONENTS_ROOT = "${root}/src";
    COGNIPILOT_REPO_MANIFEST = repositoryManifest;
  };

  # FastDyn is private and expensive to provision. Its compiler/QEMU stack is
  # only added when `ws build FastDyn` or `ws shell FastDyn` requests it.
  profiles.fastdyn.module = {
    env.COGNIPILOT_FASTDYN_PROFILE = "1";
    packages = with pkgs; [
      bison
      cargo
      cmake
      dtc
      expat
      flex
      gcc
      git
      glib
      gnumake
      libcjson
      libfdt
      meson
      ninja
      pixman
      pkg-config
      python3
      rustc
      zlib
    ];
  };

  scripts.ws = {
    description = "CogniPilot workspace mode, build, test, and status frontend";
    exec = ''exec ${pkgs.bash}/bin/bash "${root}/scripts/ws" "$@"'';
  };

  scripts.workspace-west = {
    description = "Validate and materialize the shared pinned west workspace";
    exec = ''exec ${python}/bin/python "${root}/scripts/workspace-west.py" "$@"'';
  };

  tasks = import ./nix/tasks.nix {
    inherit
      config
      lib
      pkgs
      python
      root
      ;
  };

  enterShell = ''
    mode="local"
    profile="default"
    if [ -r "$DEVENV_STATE/workspace-mode" ]; then
      mode="$(cat "$DEVENV_STATE/workspace-mode")"
    fi
    if [ -r "$DEVENV_STATE/workspace-profile" ]; then
      profile="$(cat "$DEVENV_STATE/workspace-profile")"
    fi
    echo "cognipilot_workspace ($mode mode, $profile profile) — run: ws help"
  '';
}
