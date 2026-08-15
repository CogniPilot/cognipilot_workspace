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
    util-linux
  ];

  env = {
    COGNIPILOT_PROFILE = "base";
    COGNIPILOT_PROFILES = lib.concatStringsSep " " (
      builtins.filter (
        profile:
        !(builtins.elem profile [
          "hostname"
          "user"
        ])
      ) (builtins.attrNames config.profiles)
    );
    # Compiler caches are deliberately shared by every profile. Profile state
    # remains isolated under DEVENV_STATE, while identical compiles are reused.
    CCACHE_DIR = config.git.root + "/.devenv/cache/ccache";
    SCCACHE_DIR = config.git.root + "/.devenv/cache/sccache";
    # The interactive renderer retains only a short tail of failed task output,
    # which often hides the diagnostic at the beginning. Keep complete,
    # chronological logs and use the terminal's configurable ANSI colors.
    DEVENV_TUI = "false";
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
    if test "$COGNIPILOT_PROFILE" = base || \
      test "''${COGNIPILOT_LIST_PROFILES:-}" = true; then
      echo
      if test "$COGNIPILOT_PROFILE" = base; then
        echo "No development profile is active. Start one directly:"
      else
        echo "The current local profile is $COGNIPILOT_PROFILE. Select another profile:"
      fi
      echo "  ./setup rdd2                      # select RDD2 locally and enter its shell"
      echo "  devenv -P rdd2 shell              # enter the RDD2 shell once"
      echo "  devenv -P <profile> shell         # enter another profile once"
      echo
      echo "Available profiles:"
      for profile in $COGNIPILOT_PROFILES; do
        echo "  $profile"
      done
      echo
      if test "$COGNIPILOT_PROFILE" = base; then
        echo "To make one the default, create devenv.local.yaml containing:"
        echo "  profile: rdd2"
        echo "Then use:"
        echo "  devenv shell"
        echo
        echo "Base tasks: devenv tasks list"
      else
        echo "Local selection: devenv.local.yaml"
        echo "Current tasks:   devenv tasks list"
      fi
      echo "Profile details: README.md#pick-a-profile"
    else
      echo "Tasks: devenv -P $COGNIPILOT_PROFILE tasks list"
      echo "Guide: README.md#pick-a-profile"
    fi
  '';
}
