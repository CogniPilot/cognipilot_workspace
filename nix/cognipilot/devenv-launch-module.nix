{ config, lib, ... }:

let
  rootConfig = config;
  cfg = config.cognipilot.devenvLaunches;
  renderLaunch = import ./devenv-launch-renderer.nix { inherit lib; };
  projects = builtins.attrValues rootConfig.cognipilot.validatedIndex.projects;
  allLaunches = lib.concatMap (
    project: map (launchId: "${project.packageId}:${launchId}") (builtins.attrNames project.launches)
  ) projects;
  selectedLaunches = if cfg.launches == [ ] then allLaunches else cfg.launches;
  unknownLaunches = builtins.filter (
    coordinate: !(builtins.elem coordinate allLaunches)
  ) selectedLaunches;
  safeRelativePath = value:
    value != ""
    && !(lib.hasPrefix "/" value)
    && builtins.all (segment: segment != "" && segment != "." && segment != "..") (
      lib.splitString "/" value
    );
in
{
  options.cognipilot.devenvLaunches = {
    enable = lib.mkEnableOption "devenv/process-compose shells generated from CogniPilot launch IR";

    launches = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      description = ''
        Exact package:launch coordinates to export. An empty list exports every
        normalized launch. Selection is root-owned; project modules need no
        devenv shell or process-manager boilerplate.
      '';
    };

    workspaceRoot = lib.mkOption {
      type = lib.types.str;
      default = ".";
      description = "Runtime workspace root used by the default WORKSPACE/src/REPOSITORY_ID source convention.";
    };

    sourceBindings = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = { };
      description = "Root-owned local checkout paths keyed by normalized source input ID.";
    };

    executableBindings = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = { };
      description = ''
        Optional immutable runtime command paths keyed by exact
        package:executable coordinate. Unbound executables are resolved by the
        standalone runtime client via `run package/executable -- ...`.
      '';
    };

    environmentPrefix = lib.mkOption {
      type = lib.types.strMatching "[A-Z_][A-Z0-9_]*";
      default = "COGNIPILOT_PARAM";
      description = "Prefix for generated runtime-parameter environment references.";
    };

    runtimeClient = lib.mkOption {
      type = lib.types.str;
      default = "nixspace";
      description = ''
        Standalone runtime client used for endpoint readiness probes. It must
        support `run package/executable -- ...` and
        `_probe tcp --host HOST --port PORT` plus argv-safe HTTP path/status
        probes, and must not link workspace project implementations.
      '';
    };

    sessionStateRoot = lib.mkOption {
      type = lib.types.addCheck lib.types.str safeRelativePath;
      default = ".devenv/state/nixspace/sessions";
      description = ''
        Workspace-relative root for runtime-only launch session records,
        sockets, input files, and logs. The Rust client receives this path in
        the generated execution plan and does not choose workspace state
        conventions itself.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    perSystem =
      { config, pkgs, ... }:
      if builtins.length selectedLaunches != builtins.length (lib.unique selectedLaunches) then
        throw "cognipilot.devenvLaunches.launches must not contain duplicate coordinates"
      else if unknownLaunches != [ ] then
        throw "cognipilot.devenvLaunches.launches contains unknown coordinates: ${lib.concatStringsSep ", " unknownLaunches}"
      else
        let
          renderedLaunches = builtins.listToAttrs (
            map (
              coordinate:
              let
                rendered = renderLaunch {
                  index = rootConfig.cognipilot.validatedIndex;
                  launch = coordinate;
                  inherit (cfg)
                    environmentPrefix
                    executableBindings
                    runtimeClient
                    sourceBindings
                    workspaceRoot
                    ;
                };
              in
              lib.nameValuePair rendered.outputName rendered
            ) selectedLaunches
          );
          launchExecutionPlan = pkgs.writeTextDir "share/nixspace/launch-plan.json" (
            builtins.toJSON {
              apiVersion = "nixspace/v1";
              kind = "LaunchExecution";
              interfaceVersion = 3;
              stateRoot = cfg.sessionStateRoot;
              launches = builtins.listToAttrs (
                map (
                  rendered:
                  let
                    shell = config.devenv.shells.${rendered.outputName};
                    manager = shell.process.managers.process-compose;
                    processCompose = lib.getExe manager.package;
                    socket = { runtime = "sessionSocket"; };
                    sessionLog = { runtime = "sessionLog"; };
                    clientPrefix = [
                      processCompose
                      "--unix-socket"
                      socket
                    ];
                    upPrefix = [
                      processCompose
                      "--config"
                      (toString manager.configFile)
                      "--disable-dotenv"
                      "--unix-socket"
                      socket
                      "--log-file"
                      sessionLog
                      "--ordered-shutdown"
                    ];
                  in
                  lib.nameValuePair rendered.workspaceCoordinate {
                    coordinate = rendered.coordinate;
                    workspaceLaunch = rendered.workspaceCoordinate;
                    parameters = rendered.parameters;
                    declaredPorts = rendered.runtime.declaredPorts;
                    processPolicies = rendered.runtime.processPolicies;
                    sessionEnvironment = rendered.runtime.sessionEnvironment;
                    runner = {
                      kind = "devenv-process-compose";
                      workingDirectory = cfg.workspaceRoot;
                      commands = {
                        up.argv = upPrefix ++ [
                          "-t=true"
                          "up"
                        ];
                        start.argv = upPrefix ++ [
                          "--detached"
                          "-t=false"
                          "up"
                        ];
                        attach.argv = clientPrefix ++ [ "attach" ];
                        status.argv = clientPrefix ++ [
                          "project"
                          "state"
                        ];
                        logs.argv = clientPrefix ++ [
                          "process"
                          "logs"
                        ];
                        down.argv = clientPrefix ++ [ "down" ];
                      };
                    };
                  }
                ) (builtins.attrValues renderedLaunches)
              );
            }
          );
        in
        {
          devenv.shells = lib.mapAttrs (_: rendered: {
            process.manager.implementation = "process-compose";
            process.managers.process-compose.settings.log_location =
              "$NIXSPACE_SESSION_DIR/processes.log";
            processes = rendered.processes;
          }) renderedLaunches;

          packages =
            {
              nixspace-launch-plan = launchExecutionPlan;
            }
            // lib.concatMapAttrs (
              outputName: _:
              let
                shell = config.devenv.shells.${outputName};
              in
              {
                "${outputName}-config" = shell.process.managers.process-compose.configFile;
              }
            ) renderedLaunches;

          checks = lib.mapAttrs' (
            outputName: _:
            lib.nameValuePair "${outputName}-config"
              config.devenv.shells.${outputName}.process.managers.process-compose.configFile
          ) renderedLaunches;

          apps = lib.mapAttrs (outputName: _: {
            type = "app";
            # This is devenv's own process-manager launcher. CogniPilot does not
            # implement a second supervisor or session runtime.
            program = "${config.devenv.shells.${outputName}.procfileScript}";
          }) renderedLaunches;
        };
  };
}
