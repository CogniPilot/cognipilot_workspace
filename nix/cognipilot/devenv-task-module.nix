{ config, lib, ... }:

let
  rootConfig = config;
  cfg = config.cognipilot.devenvTasks;
  generateTasks = import ./devenv-task-generator.nix { inherit lib; };
  actionToolProfileProviders = import ./action-tool-profiles.nix { inherit lib; };
  portableRelativePath = value:
    builtins.match "[A-Za-z0-9._-]+(/[A-Za-z0-9._-]+)*" value != null
    && builtins.all (segment: segment != "." && segment != "..") (lib.splitString "/" value);
in
{
  options.cognipilot.devenvTasks = {
    enable = lib.mkEnableOption "devenv tasks generated from the normalized CogniPilot index";

    workspaceRoot = lib.mkOption {
      type = lib.types.str;
      default = ".";
      description = ''
        Runtime workspace root used by the default local-source convention.
        A package source defaults to `WORKSPACE/src/REPOSITORY_ID`.
      '';
    };

    sourceBindings = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = { };
      description = ''
        Root-owned local checkout paths keyed by normalized source input ID.
        Bind only exceptions to the `WORKSPACE/src/REPOSITORY_ID` convention;
        project definition modules must not embed workspace checkout paths.
      '';
    };

    taskStateRoot = lib.mkOption {
      type = lib.types.addCheck lib.types.str portableRelativePath;
      default = ".nixspace/state/tasks";
      description = ''
        Portable workspace-relative runtime state directory for generated
        action locks and atomic artifact generations. Empty, dot, parent, and
        platform-specific path components are rejected before task emission.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    perSystem =
      { config, pkgs, ... }:
      {
        devenv.shells.default.tasks = generateTasks {
          index = rootConfig.cognipilot.validatedIndex;
          nixspaceExecutable = lib.getExe' config.packages.nixspace "nixspace";
          toolProfiles = lib.mapAttrs (_: provider: provider pkgs) actionToolProfileProviders;
          inherit (cfg) sourceBindings taskStateRoot workspaceRoot;
        };
      };
  };
}
