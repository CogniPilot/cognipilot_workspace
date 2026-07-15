{
  pkgs,
  lib ? pkgs.lib,
  nixspace,
  devenvSource,
}:

let
  apiVersion = "nixspace/v1";
  workspaceRoot = ".";
  client = lib.getExe nixspace;
  cognipilotModuleRoot = ../cognipilot;
  actionGenerationLayout = import ../nixspace/action-generation-layout.nix;

  command =
    argv:
    {
      inherit argv;
      cwd = workspaceRoot;
      environment = { };
      inheritEnvironment = false;
      timeoutMilliseconds = 15000;
      expectedExitCodes = [ 0 ];
    };

  benchmarkCase =
    {
      description,
      context,
      setup ? [ ],
      beforeEach ? [ ],
      measure,
      afterEach ? [ ],
      teardown ? [ ],
      gates ? { },
    }:
    {
      inherit
        description
        context
        setup
        beforeEach
        measure
        afterEach
        teardown
        gates
        ;
      warmupSamples = 1;
      measuredSamples = 7;
    };

  # A small dependency-free crate makes the benchmark about the editable
  # ActionTask path, not registry access or an unrelated dependency closure.
  implementationFixture = ../../tests/fixtures/sccache-pilot;
  implementationState = ".nixspace/state/benchmarks/fixtures/implementation-edit";
  implementationSource = "${implementationState}/source";
  implementationTarget = "${implementationState}/target";
  implementationOutput = "${implementationTarget}/debug/nixspace-sccache-pilot";
  implementationEditedSource = pkgs.writeText "nixspace-benchmark-edited-main.rs" ''
    fn main() {
        println!("nixspace implementation edit");
    }
  '';
  implementationIdentity = {
    packageId = "nixspace-sccache-pilot";
    targetId = "default";
    actionId = "build";
    adapter = "cargo-v1";
    argv = [
      "cargo"
      "build"
      "--offline"
      "--locked"
      "--target-dir"
      "../target"
    ];
    environment = { };
    environmentPaths.CARGO_HOME = "${implementationState}/cargo-home";
    pathPrefixes = map (package: "${package}/bin") [
      pkgs.cargo
      pkgs.nix
      pkgs.rustc
      pkgs.stdenv.cc
    ];
    locks = [ "${implementationState}/locks/cargo.lock" ];
    outputs = [
      {
        coordinate = "nixspace-sccache-pilot:default:executable";
        path = implementationOutput;
        kind = "executable";
        contract = {
          name = "nixspace-sccache-pilot";
          version = 1;
        };
        proof = {
          kind = "nix-nar-sha256";
          argvPrefix = [
            "nix"
            "hash"
            "path"
            "--type"
            "sha256"
            "--sri"
            "--"
          ];
        };
      }
    ];
  };
  implementationActionPlan = {
    inherit apiVersion;
    kind = "ActionTask";
    interfaceVersion = 3;
    cwd = implementationSource;
    inherit (implementationIdentity)
      argv
      environment
      environmentPaths
      pathPrefixes
      locks
      outputs
      ;
    generation = {
      inherit apiVersion;
      kind = "ActionGenerationStore";
      interfaceVersion = 2;
      root = "${implementationState}/generations";
      layout = actionGenerationLayout;
      identity = {
        inherit apiVersion;
        kind = "ActionTaskIdentity";
        interfaceVersion = 1;
        declaration = implementationIdentity;
      };
    };
    result = {
      task = "nixspace-sccache-pilot:default:build";
      packageId = "nixspace-sccache-pilot";
      targetId = "default";
      actionId = "build";
      artifacts.executable = {
        kind = "executable";
        path = implementationOutput;
        contract = {
          name = "nixspace-sccache-pilot";
          version = 1;
        };
      };
    };
  };
  implementationActionPlanPath = pkgs.writeText "implementation-edit-action-plan.json" (
    builtins.toJSON implementationActionPlan
  );
  runImplementation = outputFile:
    (command [
      client
      "_run-task"
      "--plan-file"
      (toString implementationActionPlanPath)
    ])
    // {
      environment.DEVENV_TASK_OUTPUT_FILE = outputFile;
      timeoutMilliseconds = 30000;
    };

  schemaModule =
    version:
    pkgs.writeText "benchmark-schema-v${toString version}.nix" ''
      {
        cognipilot.projects.benchmark_schema = {
          preset = "cargo-v1";
          targets.default.artifacts.outputs.library = {
            kind = "file";
            path = "lib/libbenchmark_schema.rlib";
            contract = {
              name = "benchmark-schema";
              version = ${toString version};
            };
          };
        };
      }
    '';
  schemaBeforeModule = schemaModule 1;
  schemaAfterModule = schemaModule 2;
  schemaEvaluator = pkgs.writeText "benchmark-schema-evaluator.nix" ''
    { modulePath, expectedVersion }:
    let
      pkgs = import ${pkgs.path} { };
      evaluated = pkgs.lib.evalModules {
        modules = [
          ${cognipilotModuleRoot}/flake-module.nix
          (builtins.toPath modulePath)
        ];
      };
      index = evaluated.config.cognipilot.validatedIndex;
      version =
        index.projects.benchmark_schema.targets.default.artifacts.outputs.library.contract.version;
    in
    if version == expectedVersion then
      builtins.hashString "sha256" (builtins.toJSON index)
    else
      throw "expected benchmark-schema contract version ''${toString expectedVersion}, got ''${toString version}"
  '';
  evaluateSchema = modulePath: version:
    command [
      (lib.getExe pkgs.nix)
      "eval"
      "--offline"
      "--impure"
      "--raw"
      "--file"
      (toString schemaEvaluator)
      "--apply"
      ''evaluator: evaluator {
        modulePath = ${builtins.toJSON (toString modulePath)};
        expectedVersion = ${toString version};
      }''
    ];

  variantModule =
    board:
    pkgs.writeText "benchmark-variant-${lib.replaceStrings [ "/" ] [ "_" ] board}.nix" ''
      {
        cognipilot.projects.benchmark_variant = {
          repositoryId = "benchmark-variant";
          source.input = "benchmark-variant-source";
          preset = "zephyr-native-sim-v1";
          targets.default.variants.dimensions.board = {
            values = [ "native_sim" "native_sim/native/64" ];
            default = ${builtins.toJSON board};
          };
        };
      }
    '';
  variantBeforeBoard = "native_sim";
  variantAfterBoard = "native_sim/native/64";
  variantBeforeModule = variantModule variantBeforeBoard;
  variantAfterModule = variantModule variantAfterBoard;
  variantEvaluator = pkgs.writeText "benchmark-variant-evaluator.nix" ''
    { modulePath, expectedBoard, expectedBuildDirectory }:
    let
      pkgs = import ${pkgs.path} { };
      evaluated = pkgs.lib.evalModules {
        modules = [
          ${cognipilotModuleRoot}/flake-module.nix
          (builtins.toPath modulePath)
        ];
      };
      index = evaluated.config.cognipilot.validatedIndex;
      target = index.projects.benchmark_variant.targets.default;
      action = target.actions.build;
      board = target.variants.dimensions.board.default;
      argv = builtins.toJSON action.argv;
    in
    if board != expectedBoard then
      throw "expected board ''${expectedBoard}, got ''${board}"
    else if !(pkgs.lib.hasInfix expectedBoard argv) then
      throw "normalized build action does not contain the selected board"
    else if !(pkgs.lib.hasInfix expectedBuildDirectory argv) then
      throw "normalized build action does not contain the selected build directory"
    else
      builtins.hashString "sha256" (builtins.toJSON index)
  '';
  evaluateVariant = modulePath: board: buildDirectory:
    command [
      (lib.getExe pkgs.nix)
      "eval"
      "--offline"
      "--impure"
      "--raw"
      "--file"
      (toString variantEvaluator)
      "--apply"
      ''evaluator: evaluator {
        modulePath = ${builtins.toJSON (toString modulePath)};
        expectedBoard = ${builtins.toJSON board};
        expectedBuildDirectory = ${builtins.toJSON buildDirectory};
      }''
    ];

  launchCoordinate = "benchmark_launch:http";
  launchWorkspaceCoordinate = "benchmark_launch/http";
  launchOutputName = "launch-benchmark_launch--http";
  # Keep the session socket below the portable Unix-domain path limit even
  # when the checkout itself has a moderately long absolute path.
  launchState = ".nixspace/bench-launch";
  launchProjectModule = {
    cognipilot.projects.benchmark_launch = {
      packageId = "benchmark_launch";
      repositoryId = "benchmark-launch";
      source.input = "benchmark-launch-source";
      preset = "cargo-v1";
      targets.default.artifacts.outputs.sleeper = {
        kind = "executable";
        path = "bin/sleep";
        contract = {
          name = "benchmark-sleeper";
          version = 1;
        };
      };
      executables.sleeper = {
        from = "benchmark_launch:default:sleeper";
        argv = [ ];
      };
      launches.http = {
        description = "Benchmark generated process supervision and readiness.";
        processes.server = {
          executable = "benchmark_launch:sleeper";
          argv = [ { literal = "30"; } ];
          readiness = {
            kind = "started";
            timeoutMs = 5000;
          };
          restart.policy = "never";
          shutdown = {
            signal = "SIGTERM";
            timeoutMs = 2000;
            killSignal = "SIGKILL";
          };
        };
      };
    };
  };
  launchIndexEvaluation = lib.evalModules {
    modules = [
      ../cognipilot/flake-module.nix
      launchProjectModule
    ];
  };
  launchIndex = launchIndexEvaluation.config.cognipilot.validatedIndex;
  launchIndexPackage = pkgs.writeTextDir "share/nixspace/index.json" (builtins.toJSON launchIndex);
  launchIndexPath = "${launchIndexPackage}/share/nixspace/index.json";
  renderLaunch = import ../cognipilot/devenv-launch-renderer.nix { inherit lib; };
  renderedLaunch = renderLaunch {
    index = launchIndex;
    launch = launchCoordinate;
    executableBindings."benchmark_launch:sleeper" = lib.getExe' pkgs.coreutils "sleep";
    workspaceRoot = workspaceRoot;
    runtimeClient = client;
  };

  devenvSupport =
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
          default = pkgs.writeText "benchmark-empty-tasks.json" "{}";
        };
      };
    };
  devenvModules = devenvSource + "/src/modules";
  processComposeEvaluation = lib.evalModules {
    specialArgs = { inherit pkgs; };
    modules = [
      devenvSupport
      (devenvModules + "/process-managers/process-compose.nix")
      (devenvModules + "/processes.nix")
      {
        process.manager.implementation = "process-compose";
        process.managers.process-compose.settings.log_location =
          "$NIXSPACE_SESSION_DIR/processes.log";
        # This fixture has no process task dependencies. Devenv's documented
        # interactive-process escape hatch keeps the generated command as the
        # direct process-compose child instead of requiring the separate
        # devenv-tasks runner merely to time supervision and readiness.
        processes = lib.mapAttrs (
          _:
          process:
          process
          // {
            process-compose = (process.process-compose or { }) // {
              is_interactive = true;
            };
          }
        ) renderedLaunch.processes;
      }
    ];
  };
  processComposeManager = processComposeEvaluation.config.process.managers.process-compose;
  processComposePackage = processComposeManager.package;
  processComposeConfig = processComposeManager.configFile;

  launchRootEvaluation = lib.evalModules {
    modules = [
      ../cognipilot/flake-module.nix
      ../cognipilot/devenv-launch-module.nix
      (
        { lib, ... }:
        {
          options.perSystem = lib.mkOption { type = lib.types.raw; };
          config = lib.mkMerge [
            launchProjectModule
            {
              cognipilot.devenvLaunches = {
                enable = true;
                launches = [ launchCoordinate ];
                workspaceRoot = workspaceRoot;
                executableBindings."benchmark_launch:sleeper" = lib.getExe' pkgs.coreutils "sleep";
                runtimeClient = client;
                sessionStateRoot = "${launchState}/sessions";
              };
            }
          ];
        }
      )
    ];
  };
  launchModuleFragment = launchRootEvaluation.config.perSystem {
    inherit pkgs;
    config.devenv.shells.${launchOutputName} = processComposeEvaluation.config // {
      procfileScript = lib.getExe processComposePackage;
    };
  };
  launchExecutionPlan = launchModuleFragment.packages.nixspace-launch-plan;
  launchExecutionPlanPath = "${launchExecutionPlan}/share/nixspace/launch-plan.json";
  launchClientPrefix = [
    client
    "--workspace-root"
    workspaceRoot
    "--index"
    launchIndexPath
    "--launch-plan"
    launchExecutionPlanPath
  ];
  launchUp =
    (command (
      launchClientPrefix
      ++ [
        "launch"
        "up"
        launchWorkspaceCoordinate
        "--name"
        "measured"
        "--detach"
      ]
    ))
    // {
      timeoutMilliseconds = 30000;
    };
  launchReady = command [
    (lib.getExe processComposePackage)
    "--unix-socket"
    "${launchState}/sessions/measured/process-compose.sock"
    "project"
    "is-ready"
    "--wait"
  ];
  launchDown = command (
    launchClientPrefix
    ++ [
      "launch"
      "down"
      "measured"
    ]
  );

  portableCases = {
    implementation-edit-cargo = benchmarkCase {
      description = "Rebuild one dependency-free Cargo executable after an implementation edit.";
      context = {
        category = "editable-action";
        ecosystem = "cargo";
      };
      gates = {
        p50Milliseconds = 1000;
        p95Milliseconds = 1000;
      };
      setup = [
        (command [ (lib.getExe' pkgs.coreutils "rm") "-rf" implementationState ])
      ];
      beforeEach = [
        (command [ (lib.getExe' pkgs.coreutils "mkdir") "-p" implementationSource ])
        (command [
          (lib.getExe' pkgs.coreutils "cp")
          "-R"
          "--no-preserve=mode,ownership,timestamps"
          "${implementationFixture}/."
          implementationSource
        ])
        (runImplementation "${implementationState}/baseline-result.json")
        (command [
          (lib.getExe' pkgs.coreutils "cp")
          (toString implementationEditedSource)
          "${implementationSource}/src/main.rs"
        ])
      ];
      measure = [ (runImplementation "${implementationState}/edited-result.json") ];
      teardown = [
        (command [ (lib.getExe' pkgs.coreutils "rm") "-rf" implementationState ])
      ];
    };

    interface-schema-edit = benchmarkCase {
      description = "Re-evaluate a project index after an artifact interface-major edit.";
      context = {
        category = "nix-evaluation";
        change = "artifact-contract-major";
      };
      gates = {
        p50Milliseconds = 2000;
        p95Milliseconds = 2000;
      };
      beforeEach = [ (evaluateSchema schemaBeforeModule 1) ];
      measure = [ (evaluateSchema schemaAfterModule 2) ];
    };

    variant-plan-switch = benchmarkCase {
      description = "Re-evaluate the Zephyr action plan after switching the selected board variant.";
      context = {
        category = "nix-evaluation";
        ecosystem = "zephyr";
      };
      gates = {
        p50Milliseconds = 2000;
        p95Milliseconds = 2000;
      };
      beforeEach = [
        (evaluateVariant variantBeforeModule variantBeforeBoard "benchmark-variant/build-native_sim_sil")
      ];
      measure = [
        (evaluateVariant
          variantAfterModule
          variantAfterBoard
          "benchmark-variant/build-native_sim_native_64_sil"
        )
      ];
    };

  };
  linuxCases = lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
    launch-start-readiness = benchmarkCase {
      description = "Start a generated process-compose launch and wait for its supervised readiness proof.";
      context = {
        category = "launch";
        readiness = "started";
        supervisor = "process-compose";
      };
      gates = {
        p50Milliseconds = 2000;
        p95Milliseconds = 2000;
      };
      setup = [
        (command [ (lib.getExe' pkgs.coreutils "rm") "-rf" launchState ])
        (command [ (lib.getExe' pkgs.coreutils "mkdir") "-p" launchState ])
      ];
      beforeEach = [
        (command [ (lib.getExe' pkgs.coreutils "rm") "-rf" "${launchState}/sessions/measured" ])
      ];
      measure = [ launchUp launchReady ];
      afterEach = [
        launchDown
      ];
      teardown = [
        (command [ (lib.getExe' pkgs.coreutils "rm") "-rf" launchState ])
      ];
    };
  };
  cases = portableCases // linuxCases;

  checks = {
    implementation-plan-v3 =
      implementationActionPlan.kind == "ActionTask"
      && implementationActionPlan.interfaceVersion == 3
      && implementationActionPlan.outputs != [ ];
    implementation-generation-is-typed =
      implementationActionPlan.generation.kind == "ActionGenerationStore"
      && implementationActionPlan.generation.identity.kind == "ActionTaskIdentity"
      && implementationActionPlan.generation.identity.declaration != { };
    schema-snapshots-change-contract =
      schemaBeforeModule != schemaAfterModule;
    variant-snapshots-change-default =
      variantBeforeModule != variantAfterModule
      && variantBeforeBoard != variantAfterBoard;
    launch-is-pinned-process-compose =
      processComposeEvaluation.config.process.manager.implementation == "process-compose"
      && processComposePackage == pkgs.process-compose
      && launchIndex.projects.benchmark_launch.launches.http.processes.server.readiness.kind
        == "started";
    launch-plan-is-generated =
      builtins.hasAttr "nixspace-launch-plan" launchModuleFragment.packages
      && builtins.hasAttr launchOutputName launchModuleFragment.devenv.shells
      && launchModuleFragment.devenv.shells.${launchOutputName}.process.manager.implementation
        == "process-compose";
    cases-have-measurements =
      lib.all (case: case.measure != [ ]) (builtins.attrValues cases);
    cases-have-explicit-lifecycle =
      portableCases.implementation-edit-cargo.setup != [ ]
      && portableCases.implementation-edit-cargo.teardown != [ ]
      && (!pkgs.stdenv.hostPlatform.isLinux || linuxCases.launch-start-readiness.afterEach != [ ]);
  };
  failedChecks = builtins.attrNames (lib.filterAttrs (_: passed: !passed) checks);
in
if failedChecks != [ ] then
  throw "CogniPilot benchmark fixture checks failed: ${lib.concatStringsSep ", " failedChecks}"
else
  {
    inherit apiVersion cases checks;
    kind = "BenchmarkFixtures";
    interfaceVersion = 1;
    integration = {
      implementationEdit = {
        actionPlan = implementationActionPlan;
        actionPlanPath = implementationActionPlanPath;
        editedSource = implementationEditedSource;
        source = implementationFixture;
      };
      schemaEdit = {
        beforeModule = schemaBeforeModule;
        afterModule = schemaAfterModule;
        evaluator = schemaEvaluator;
      };
      variantSwitch = {
        beforeModule = variantBeforeModule;
        afterModule = variantAfterModule;
        evaluator = variantEvaluator;
      };
      launch = {
        index = launchIndex;
        indexPath = launchIndexPath;
        executionPlan = launchExecutionPlan;
        executionPlanPath = launchExecutionPlanPath;
        inherit processComposeConfig processComposePackage;
      };
    };
  }
