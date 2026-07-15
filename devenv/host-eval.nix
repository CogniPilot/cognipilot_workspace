# Remove this host-side evaluation when Devenv links Nix >= 2.35:
# https://github.com/cachix/devenv/issues/2729
let
  root = ../.;
  profile = builtins.getEnv "DEVENV_PROFILE";
  dotfile = if profile == "" then root + "/.devenv" else root + "/.devenv/profiles/${profile}";
  tmpdir =
    let
      value = builtins.getEnv "TMPDIR";
    in
    if value == "" then "/tmp" else value;
  evaluated = import (dotfile + "/bootstrap/default.nix") {
    version = "2.1.3";
    system = builtins.currentSystem;
    devenv_root = root;
    git_root = root;
    devenv_dotfile = dotfile;
    devenv_dotfile_path = toString dotfile;
    devenv_tmpdir = tmpdir;
    devenv_runtime = "${tmpdir}/devenv-runtime";
    devenv_direnvrc_latest_version = 5;
    active_profiles = if profile == "" then [ ] else [ profile ];
    hostname = "";
    username = "";
    cli_options = { };
  };
in
map (package: package.drvPath) evaluated.config.packages
