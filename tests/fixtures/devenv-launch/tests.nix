let
  pkgs = import <nixpkgs> { };
  lib = pkgs.lib;
  index =
    (lib.evalModules {
      modules = [ ../project-flakes/golden/launch-ir.nix ];
    }).config.cognipilot.validatedIndex;
  renderer = import ../../../nix/cognipilot/devenv-launch-renderer.nix { inherit lib; };
  bindings = {
    "app:router" = "/nix/store/cognipilot-test-router/bin/router";
    "app:monitor" = "/nix/store/cognipilot-test-monitor/bin/monitor";
  };
  render =
    launch:
    renderer {
      inherit index launch;
      executableBindings = bindings;
      workspaceRoot = "/workspace";
      runtimeClient = "/nix/store/nixspace-test/bin/nixspace";
    };
  router = render "app:router";
  stack = render "app:stack";
  unbound = renderer {
    inherit index;
    launch = "app:router";
    workspaceRoot = "/workspace";
    runtimeClient = "/nix/store/nixspace-test/bin/nixspace";
  };
  succeededIndex = lib.recursiveUpdate index {
    projects.app.launches.router.processes.monitor.dependencies.router = "succeeded";
  };
  automaticPortIndex = lib.recursiveUpdate index {
    projects.app.launches.router.parameters.port.allocation = "automatic";
  };
  automaticPort = renderer {
    index = automaticPortIndex;
    launch = "app:router";
    executableBindings = bindings;
    workspaceRoot = "/workspace";
  };
  succeeded = renderer {
    index = succeededIndex;
    launch = "app:router";
    executableBindings = bindings;
    workspaceRoot = "/workspace";
  };
  udpIndex = lib.recursiveUpdate index {
    projects.app.launches.router.processes.router.endpoints.http.protocol = "udp";
  };
  udpResult = builtins.tryEval (
    builtins.deepSeq ((renderer {
      index = udpIndex;
      launch = "app:router";
      executableBindings = bindings;
    }).processes
    ) true
  );
  finalSignalIndex = lib.recursiveUpdate index {
    projects.app.launches.router.processes.router.shutdown.killSignal = "SIGTERM";
  };
  finalSignalResult = builtins.tryEval (
    builtins.deepSeq ((renderer {
      index = finalSignalIndex;
      launch = "app:router";
      executableBindings = bindings;
    }).processes
    ) true
  );
  mutableBindingResult = builtins.tryEval (
    builtins.deepSeq (renderer {
      inherit index;
      launch = "app:router";
      executableBindings."app:router" = "/tmp/router";
    }) true
  );
  moduleEvaluation = lib.evalModules {
    modules = [
      (
        { lib, ... }:
        {
          options = {
            assertions = lib.mkOption {
              type = lib.types.listOf lib.types.anything;
              default = [ ];
            };
            cognipilot.validatedIndex = lib.mkOption { type = lib.types.attrs; };
            perSystem = lib.mkOption { type = lib.types.raw; };
          };
        }
      )
      ../../../nix/cognipilot/devenv-launch-module.nix
      {
        cognipilot.validatedIndex = index;
        cognipilot.devenvLaunches = {
          enable = true;
          executableBindings = bindings;
          workspaceRoot = "/workspace";
          runtimeClient = "/nix/store/nixspace-test/bin/nixspace";
        };
      }
    ];
  };
  upstreamConfig = pkgs.writeText "upstream-process-compose.yaml" "version: 0.5";
  upstreamLauncher = pkgs.writeShellScript "upstream-devenv-up" "exit 0";
  upstreamManager = pkgs.writeShellScriptBin "process-compose" "exit 0";
  moduleFragment = moduleEvaluation.config.perSystem {
    inherit pkgs;
    config.devenv.shells = {
      "launch-app--router" = {
        process.managers.process-compose = {
          configFile = upstreamConfig;
          package = upstreamManager;
        };
        procfileScript = upstreamLauncher;
      };
      "launch-app--stack" = {
        process.managers.process-compose = {
          configFile = upstreamConfig;
          package = upstreamManager;
        };
        procfileScript = upstreamLauncher;
      };
    };
  };
  executionPlan = builtins.fromJSON (
    builtins.unsafeDiscardStringContext (
      builtins.readFile "${moduleFragment.packages.nixspace-launch-plan}/share/nixspace/launch-plan.json"
    )
  );
  routerProcess = router.processes.router;
  monitorProcess = router.processes.monitor;
  includedRouter = stack.processes."base--router";
  secretContract = stack.parameters.COGNIPILOT_PARAM_APP_STACK_ROUTER_TOKEN;
  checks = {
    versioned = router.schemaVersion == 1;
    directArgv = lib.hasInfix "exec /nix/store/cognipilot-test-router/bin/router" routerProcess.exec;
    runtimeClientExec = lib.hasPrefix "exec /nix/store/nixspace-test/bin/nixspace run --selection-root app app/router --" unbound.processes.router.exec;
    runtimeArgv = lib.hasInfix ''"$COGNIPILOT_PARAM_APP_ROUTER_PORT"'' routerProcess.exec;
    secretOnlyInEnvironment =
      routerProcess.env.ROUTER_TOKEN == "$COGNIPILOT_PARAM_APP_ROUTER_TOKEN"
      && !(lib.hasInfix "TOKEN_VALUE" (builtins.toJSON router));
    dependency = monitorProcess.after == [ "devenv:processes:router@ready" ];
    readiness =
      routerProcess.ready.exec
      == ''exec /nix/store/nixspace-test/bin/nixspace _probe http --host "$COGNIPILOT_PARAM_APP_ROUTER_HOST" --port "$COGNIPILOT_PARAM_APP_ROUTER_PORT" --path /ready --expect-status 204'';
    sessionEnvironment =
      routerProcess.env.ROUTER_STATE == "$NIXSPACE_SESSION_DIR/router/state.json"
      &&
        router.runtime.sessionEnvironment.ROUTER_STATE == {
          base = "session";
          path = "router/state.json";
          create = "parent";
        };
    restart =
      routerProcess.restart == {
        on = "on_failure";
        max = 3;
        window = null;
      };
    restartBackoff = routerProcess.process-compose.availability.backoff_seconds == 1;
    shutdown =
      routerProcess.process-compose.shutdown == {
        signal = 15;
        timeout_seconds = 3;
      };
    defaultShutdown =
      monitorProcess.process-compose.shutdown == {
        signal = 15;
        timeout_seconds = 10;
      };
    workingDirectory = routerProcess.cwd == "/workspace/src/app";
    includedNames =
      builtins.attrNames stack.processes == [
        "base--monitor"
        "base--router"
      ];
    forwardedParameter = lib.hasInfix ''"$COGNIPILOT_PARAM_APP_STACK_ROUTER_PORT"'' includedRouter.exec;
    secretContractContainsNoValue =
      secretContract.secret
      && secretContract.default == null
      && secretContract.environment == "COGNIPILOT_PARAM_APP_STACK_ROUTER_TOKEN";
    lifecyclePolicyIsRuntimeMetadata =
      stack.runtime.processPolicies."base--router" == {
        onExit = "restart";
        onReadinessLoss = "restart";
        required = true;
      };
    declaredPortIsRuntimeMetadata =
      router.runtime.declaredPorts == [
        {
          launch = "app/router";
          process = "router";
          endpoint = "http";
          protocol = "http";
          transport = "tcp";
          host = "127.0.0.1";
          port = 7447;
          hostEnvironment = "COGNIPILOT_PARAM_APP_ROUTER_HOST";
          portEnvironment = "COGNIPILOT_PARAM_APP_ROUTER_PORT";
        }
      ];
    automaticPortIsResolvedOnlyAtRuntime =
      automaticPort.runtime.declaredPorts == [
        {
          launch = "app/router";
          process = "router";
          endpoint = "http";
          protocol = "http";
          transport = "tcp";
          host = "127.0.0.1";
          port = null;
          hostEnvironment = "COGNIPILOT_PARAM_APP_ROUTER_HOST";
          portEnvironment = "COGNIPILOT_PARAM_APP_ROUTER_PORT";
        }
      ];
    succeededDependency =
      succeeded.processes.monitor.process-compose.depends_on.router.condition
      == "process_completed_successfully";
    unsupportedUdpRejected = !udpResult.success;
    unsupportedFinalSignalRejected = !finalSignalResult.success;
    mutableExecutableBindingRejected = !mutableBindingResult.success;
    moduleMapsDirectly =
      moduleFragment.devenv.shells."launch-app--router" == {
        process.manager.implementation = "process-compose";
        process.managers.process-compose.settings.log_location = "$NIXSPACE_SESSION_DIR/processes.log";
        processes = router.processes;
      };
    moduleExportsEveryLaunch =
      builtins.attrNames moduleFragment.devenv.shells == [
        "launch-app--router"
        "launch-app--stack"
      ];
    moduleExportsUpstreamConfig =
      moduleFragment.packages."launch-app--router-config" == upstreamConfig
      && moduleFragment.checks."launch-app--router-config" == upstreamConfig;
    moduleExportsExecutionPlan =
      executionPlan.apiVersion == "nixspace/v1"
      && executionPlan.interfaceVersion == 3
      && executionPlan.kind == "LaunchExecution"
      && executionPlan.stateRoot == ".devenv/state/nixspace/sessions"
      &&
        builtins.attrNames executionPlan.launches == [
          "app/router"
          "app/stack"
        ]
      && executionPlan.launches."app/router".parameters == router.parameters
      && executionPlan.launches."app/router".declaredPorts == router.runtime.declaredPorts
      && executionPlan.launches."app/router".processPolicies == router.runtime.processPolicies
      && executionPlan.launches."app/router".sessionEnvironment == router.runtime.sessionEnvironment
      && executionPlan.launches."app/router".runner.kind == "devenv-process-compose"
      && executionPlan.launches."app/router".runner.workingDirectory == "/workspace"
      &&
        executionPlan.launches."app/router".runner.commands.up.argv == [
          "${upstreamManager}/bin/process-compose"
          "--config"
          (toString upstreamConfig)
          "--disable-dotenv"
          "--unix-socket"
          { runtime = "sessionSocket"; }
          "--log-file"
          { runtime = "sessionLog"; }
          "--ordered-shutdown"
          "-t=true"
          "up"
        ]
      &&
        executionPlan.launches."app/router".runner.commands.start.argv == [
          "${upstreamManager}/bin/process-compose"
          "--config"
          (toString upstreamConfig)
          "--disable-dotenv"
          "--unix-socket"
          { runtime = "sessionSocket"; }
          "--log-file"
          { runtime = "sessionLog"; }
          "--ordered-shutdown"
          "--detached"
          "-t=false"
          "up"
        ]
      &&
        executionPlan.launches."app/router".runner.commands.down.argv == [
          "${upstreamManager}/bin/process-compose"
          "--unix-socket"
          { runtime = "sessionSocket"; }
          "down"
        ];
    moduleExportsUpstreamApp =
      moduleFragment.apps."launch-app--router".program == toString upstreamLauncher;
  };
  failures = lib.attrNames (lib.filterAttrs (_: passed: !passed) checks);
in
if failures == [ ] then
  checks
else
  throw "devenv launch renderer checks failed: ${lib.concatStringsSep ", " failures}"
