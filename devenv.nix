{
  config,
  lib,
  pkgs,
  ...
}:

let
  taskModules = import ./devenv/tasks.nix { inherit config lib; };
in
{
  imports = [
    taskModules.common
    ./devenv/profiles.nix
  ];

  _module.args = { inherit taskModules; };

  name = "cognipilot-workspace";

  # Devenv's own cache is enabled automatically. These are the additional
  # public caches used by CogniPilot and the ROS profile.
  cachix.pull = [
    "cognipilot"
    "ros"
  ];

  # Keep the unprofiled shell intentionally small. Project compilers and SDKs
  # are selected with ordinary Devenv profiles.
  packages = with pkgs; [
    git
    jq
    vcs2l
  ];

  env = {
    COGNIPILOT_PROFILE = "base";
    # Compiler caches are deliberately shared by every profile. Profile state
    # remains isolated under DEVENV_STATE, while identical compiles are reused.
    CCACHE_DIR = config.git.root + "/.devenv/cache/ccache";
    SCCACHE_DIR = config.git.root + "/.devenv/cache/sccache";
  };

  git-hooks.hooks = {
    actionlint.enable = true;
    deadnix.enable = true;
    nixfmt.enable = true;
    shellcheck.enable = true;
    statix.enable = true;
  };

  enterShell = ''
    echo "CogniPilot workspace ($COGNIPILOT_PROFILE profile)"
    if test "$COGNIPILOT_PROFILE" = base; then
      echo "  devenv tasks list"
    else
      echo "  devenv --profile $COGNIPILOT_PROFILE tasks list"
    fi
    echo "  docs: README.md"
  '';
}
