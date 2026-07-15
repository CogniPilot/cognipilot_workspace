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
    actionlint
    deadnix
    git
    jq
    nixfmt-rfc-style
    shellcheck
    statix
    python3Packages.west
  ];

  env = {
    CCACHE_DIR = config.env.DEVENV_STATE + "/ccache";
    SCCACHE_DIR = config.env.DEVENV_STATE + "/sccache";
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
