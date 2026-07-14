let
  pkgs = import <nixpkgs> { };
  lib = pkgs.lib;
  lock = builtins.fromJSON (builtins.readFile ../../../flake.lock);
  pinnedSource = (builtins.fetchTree lock.nodes.devenv.locked).outPath;
  pinnedModules = pinnedSource + "/src/modules";
  index =
    (lib.evalModules {
      modules = [ ../project-flakes/golden/launch-ir.nix ];
    }).config.cognipilot.validatedIndex;
  rendered = (import ../../../nix/cognipilot/devenv-launch-renderer.nix { inherit lib; }) {
    inherit index;
    launch = "app:router";
    workspaceRoot = "/workspace";
    runtimeClient = "/nix/store/nixspace-test/bin/nixspace";
  };
  support =
    { lib, ... }:
    {
      options = {
        assertions = lib.mkOption {
          type = lib.types.listOf lib.types.anything;
          default = [ ];
        };
        env = lib.mkOption {
          type = lib.types.attrsOf lib.types.anything;
          default = { };
        };
        packages = lib.mkOption {
          type = lib.types.listOf lib.types.package;
          default = [ ];
        };
        tasks = lib.mkOption {
          type = lib.types.attrsOf lib.types.anything;
          default = { };
        };
        ci = lib.mkOption {
          type = lib.types.listOf lib.types.package;
          default = [ ];
        };
        infoSections = lib.mkOption {
          type = lib.types.attrsOf lib.types.anything;
          default = { };
        };
        devenv.cli.version = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = "2.1.2";
        };
        devenv.state = lib.mkOption {
          type = lib.types.str;
          default = "/tmp/devenv-state";
        };
        devenv.runtime = lib.mkOption {
          type = lib.types.str;
          default = "/tmp/devenv-runtime";
        };
        devenv.dotfile = lib.mkOption {
          type = lib.types.str;
          default = ".devenv";
        };
        devenv.debug = lib.mkOption {
          type = lib.types.bool;
          default = false;
        };
        task.package = lib.mkOption {
          type = lib.types.package;
          default = pkgs.hello;
        };
        task.config = lib.mkOption {
          type = lib.types.path;
          default = pkgs.writeText "cognipilot-test-tasks.json" "{}";
        };
      };
    };
  evaluated = lib.evalModules {
    specialArgs = { inherit pkgs; };
    modules = [
      support
      (pinnedModules + "/process-managers/process-compose.nix")
      (pinnedModules + "/processes.nix")
      {
        process.manager.implementation = "process-compose";
        processes = rendered.processes;
      }
    ];
  };
  processes = evaluated.config.process.managers.process-compose.settings.processes;
  checks = {
    pinnedVersion = evaluated.config.devenv.cli.version == "2.1.2";
    processOptionsAccepted =
      builtins.attrNames evaluated.config.processes == [
        "monitor"
        "router"
      ];
    directExecPreserved = lib.hasPrefix "exec /nix/store/nixspace-test/bin/nixspace run --selection-root app app/router --" evaluated.config.processes.router.exec;
    dependencyTranslated = processes.monitor.depends_on.router.condition == "process_healthy";
    runtimeEnvironmentPreserved = builtins.elem "ROUTER_TOKEN=$COGNIPILOT_PARAM_APP_ROUTER_TOKEN" processes.router.environment;
    readinessTranslated = lib.hasInfix "_probe http --host \"$COGNIPILOT_PARAM_APP_ROUTER_HOST\" --port \"$COGNIPILOT_PARAM_APP_ROUTER_PORT\" --path /ready --expect-status 204" processes.router.readiness_probe.exec.command;
    restartTranslated =
      processes.router.availability == {
        backoff_seconds = 1;
        max_restarts = 3;
        restart = "on_failure";
      };
    shutdownTranslated =
      processes.router.shutdown == {
        signal = 15;
        timeout_seconds = 3;
      };
    defaultShutdownTranslated =
      processes.monitor.shutdown == {
        signal = 15;
        timeout_seconds = 10;
      };
  };
  failures = lib.attrNames (lib.filterAttrs (_: passed: !passed) checks);
in
if failures == [ ] then
  checks
else
  throw "pinned devenv launch checks failed: ${lib.concatStringsSep ", " failures}"
