{ config, lib, ... }:

let
  rootConfig = config;
  cfg = config.cognipilot.devenvWorkspace;
  westEnabled = lib.attrByPath [
    "nixspace"
    "west"
    "enable"
  ] false rootConfig;
  portableRelativePath = value:
    builtins.match "[A-Za-z0-9._-]+(/[A-Za-z0-9._-]+)*" value != null
    && builtins.all (segment: segment != "." && segment != "..") (lib.splitString "/" value);
in
{
  options.cognipilot.devenvWorkspace = {
    enable = lib.mkEnableOption "the shared CogniPilot root devenv shells";

    name = lib.mkOption {
      type = lib.types.strMatching "[a-z][a-z0-9-]*";
      default = "cognipilot-workspace";
      description = "Name of the generated default devenv shell.";
    };

    workspaceRoot = lib.mkOption {
      type = lib.types.str;
      default = ".";
      description = ''
        Runtime workspace root used by generated editable tasks and launches.
        The conventional relative value keeps mutable checkout paths out of the
        Nix store; devenv runs tasks and processes from its workspace root.
      '';
    };

    sourceBindings = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = { };
      description = ''
        Root-owned exceptions to the WORKSPACE/src/REPOSITORY_ID checkout
        convention, keyed by normalized source input ID.
      '';
    };

    taskStateRoot = lib.mkOption {
      type = lib.types.addCheck lib.types.str portableRelativePath;
      default = ".nixspace/state/tasks";
      description = ''
        Portable workspace-relative runtime state directory for action locks
        and atomic artifact generations. This is data passed to the generic
        nixspace ActionTask interface, not a path selected by the client.
      '';
    };

    launches = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Whether to export one devenv/process-compose shell and app per normalized launch.";
      };

      selected = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [ ];
        description = ''
          Internal package:launch coordinates to export. An empty list exports
          every launch. Public CLI coordinates remain package/launch.
        '';
      };
    };

  };

  config = lib.mkIf cfg.enable {
    # The shared root module owns the editable task and launch surfaces.
    # Enabling the workspace always enables its Nix-generated task graph.
    cognipilot.devenvTasks = {
      enable = true;
      inherit (cfg) sourceBindings taskStateRoot workspaceRoot;
    };
    cognipilot.devenvLaunches = {
      enable = cfg.launches.enable;
      launches = cfg.launches.selected;
      inherit (cfg) sourceBindings workspaceRoot;
    };

    perSystem =
      { config, pkgs, ... }:
      let
        nixspace = config.packages.nixspace;
        nixspaceIndex = config.packages.nixspace-index;
        completions = lib.attrByPath [ "packages" "nixspace-completions" ] null config;
        hostPlan = lib.attrByPath [ "packages" "nixspace-host-plan" ] null config;
        launchPlan =
          if cfg.launches.enable then config.packages.nixspace-launch-plan else null;
        sourcePlan = lib.attrByPath [ "packages" "nixspace-source-plan" ] null config;
        resolutionPlan = lib.attrByPath [ "packages" "nixspace-resolution-plan" ] null config;
        westPlan = if westEnabled then config.packages.nixspace-west-plan else null;
      in
      {
        # Devenv applies these settings to every generated shell, including
        # launch shells. Keep this deliberately small: Nix supplies the typed
        # client, its completions and plans; Git is the only universal
        # mutable-worktree tool.
        devenv.modules = [
          (
            { config, ... }:
            {
              cachix.pull = [ "cognipilot" ];
              env = {
                # Devenv's flake integration normally duplicates every task in
                # one DEVENV_TASKS string as well as task.config. A realistic
                # workspace crosses Linux's per-string exec limit long before
                # ARG_MAX. The upstream runner treats an empty value as absent
                # and reads DEVENV_TASK_FILE, so keep one Nix-generated static
                # authority without constraining the workspace graph size.
                DEVENV_TASKS = lib.mkForce "";
                NIXSPACE_INDEX = "${nixspaceIndex}/share/nixspace/index.json";
                NIXSPACE_WORKSPACE_ROOT = config.devenv.root;
              }
              // lib.optionalAttrs (hostPlan != null) {
                NIXSPACE_HOST_PLAN = "${hostPlan}/share/nixspace/host-plan.json";
              }
              // lib.optionalAttrs (launchPlan != null) {
                NIXSPACE_LAUNCH_PLAN = "${launchPlan}/share/nixspace/launch-plan.json";
              }
              // lib.optionalAttrs (sourcePlan != null) {
                NIXSPACE_SOURCE_PLAN = "${sourcePlan}/share/nixspace/source-plan.json";
              }
              // lib.optionalAttrs (resolutionPlan != null) {
                NIXSPACE_RESOLUTION_PLAN = "${resolutionPlan}/share/nixspace/resolution-plan.json";
              }
              // lib.optionalAttrs westEnabled {
                NIXSPACE_WEST_PLAN = "${westPlan}/share/nixspace/west-plan.json";
              };
              packages = [
                nixspace
                pkgs.git
              ]
              ++ lib.optional (completions != null) completions;
            }
          )
        ];

        # In official flake integration `devenv.cli.version` must remain null:
        # that is the upstream signal to provide the Nix-built task runner used
        # by `nix develop`. Host setup pins the optional standalone CLI version
        # independently through the generated Host plan.
        devenv.shells.default.name = cfg.name;
      };
  };
}
