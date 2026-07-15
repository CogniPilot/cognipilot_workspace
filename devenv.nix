{
  config,
  pkgs,
  ...
}:

{
  imports = [
    ./devenv/tasks.nix
    ./devenv/profiles.nix
  ];

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
    python3Packages.west
  ];

  env = {
    # Compiler caches are deliberately shared by every profile. Profile state
    # remains isolated under DEVENV_STATE, while identical compiles are reused.
    CCACHE_DIR = config.git.root + "/.devenv/cache/ccache";
    SCCACHE_DIR = config.git.root + "/.devenv/cache/sccache";
    WEST_CONFIG_LOCAL = config.git.root + "/.west/config";
  };

  git-hooks.hooks = {
    actionlint.enable = true;
    deadnix.enable = true;
    nixfmt-rfc-style.enable = true;
    shellcheck.enable = true;
    statix.enable = true;
  };

  enterShell = ''
    echo "CogniPilot workspace"
    echo "  devenv tasks list"
    echo "  devenv --profile cubs2 shell"
    echo "  devenv --profile simulation up"
  '';
}
