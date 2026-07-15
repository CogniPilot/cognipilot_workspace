{ pkgs }:

let
  inherit (pkgs) lib;
  index = import ../fixtures/devenv-tasks/index.nix;
  generate = import ../../nix/cognipilot/devenv-task-generator.nix { inherit lib; };
  actionGenerationLayout = import ../../nix/nixspace/action-generation-layout.nix;
  nixspaceExecutable = "/nix/store/00000000000000000000000000000000-nixspace/bin/nixspace";

  tasksFor =
    overrides:
    generate (
      {
        inherit index nixspaceExecutable;
        taskStateRoot = ".state/tasks";
        workspaceRoot = "/workspace";
        sourceBindings.alpha-source = "/checkouts/alpha";
      }
      // overrides
    );
  tasks = tasksFor { };
  presetIndex =
    (lib.evalModules {
      modules = [ ../fixtures/project-flakes/golden/cargo.nix ];
    }).config.cognipilot.validatedIndex;
  presetTasks = generate {
    index = presetIndex;
    inherit nixspaceExecutable;
    taskStateRoot = ".state/tasks";
    workspaceRoot = "/workspace";
    sourceBindings.example_source = "/checkouts/example";
  };

  alpha = index.projects.alpha-definition;
  alphaTarget = alpha.targets.default;
  withAlphaActions =
    actions:
    index
    // {
      projects = index.projects // {
        alpha-definition = alpha // {
          targets = alpha.targets // {
            default = alphaTarget // {
              inherit actions;
            };
          };
        };
      };
    };
  docsChanged = withAlphaActions (
    alphaTarget.actions
    // {
      docs = alphaTarget.actions.docs // {
        argv = [ "changed-doc-command" ];
      };
    }
  );
  testChanged = withAlphaActions (
    alphaTarget.actions
    // {
      test = alphaTarget.actions.test // {
        requirements = alphaTarget.actions.test.requirements // {
          memoryMiB = 2048;
        };
      };
    }
  );
  generateForIndex =
    selectedIndex:
    generate {
      index = selectedIndex;
      inherit nixspaceExecutable;
      taskStateRoot = ".state/tasks";
      workspaceRoot = "/workspace";
      sourceBindings.alpha-source = "/checkouts/alpha";
    };
  docsTasks = generateForIndex docsChanged;
  changedTestTasks = generateForIndex testChanged;

  unboundTasks = generate {
    inherit index nixspaceExecutable;
    taskStateRoot = ".nixspace/state/tasks";
    workspaceRoot = ".";
  };
  unsafeStateRoot = builtins.tryEval (
    builtins.deepSeq ((generate {
      inherit index nixspaceExecutable;
      taskStateRoot = "/absolute/state";
      workspaceRoot = "/workspace";
    })."alpha:default:build".exec
    ) true
  );

  profileIndex = lib.recursiveUpdate index {
    projects.alpha-definition.targets.default.actions.build.toolProfile = "fixture-v1";
  };
  profileTasks = generate {
    index = profileIndex;
    inherit nixspaceExecutable;
    taskStateRoot = ".state/tasks";
    workspaceRoot = "/workspace";
    sourceBindings.alpha-source = "/checkouts/alpha";
    toolProfiles.fixture-v1 = {
      pathPrefixes = [
        "/nix/store/00000000000000000000000000000000-compiler/bin"
        "/nix/store/11111111111111111111111111111111-formatter/bin"
      ];
      environment.PKG_CONFIG_PATH = "/nix/store/22222222222222222222222222222222-library/lib/pkgconfig";
      environmentPaths.SCCACHE_DIR = ".state/sccache";
    };
  };

  expectedBuildArtifacts = {
    inputs = { };
    outputs.bundle = {
      producedBy = "build";
      kind = "directory";
      path = "dist";
      contract = {
        name = "alpha-api";
        version = 1;
      };
    };
  };
  expectedBuildArgv = [
    "nix"
    "develop"
    "--no-pure-eval"
    "."
    "-c"
    "cargo"
    "build"
    "--workspace"
  ];
  expectedBuildLocks = [
    ".state/tasks/locks/cargo-target.lock"
    ".state/tasks/locks/shared-cache.lock"
  ];
  expectedBuildOutputs = [
    {
      coordinate = "alpha:default:bundle";
      path = "/checkouts/alpha/dist";
      kind = "directory";
      contract = {
        name = "alpha-api";
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
  expectedBuildDeclaration = {
    actionId = "build";
    adapter = "cargo-v1";
    argv = expectedBuildArgv;
    artifacts = expectedBuildArtifacts;
    environment = { };
    environmentPaths = { };
    locks = expectedBuildLocks;
    outputs = expectedBuildOutputs;
    packageId = "alpha";
    pathPrefixes = [ ];
    requirements = {
      cpu = 2;
      memoryMiB = 1024;
      exclusiveLocks = [
        "shared-cache"
        "cargo-target"
      ];
    };
    source = {
      coordinate = "alpha-source:.";
      input = "alpha-source";
      localPath = "/checkouts/alpha";
      root = ".";
      visibility = "private";
    };
    targetId = "default";
    toolProfileId = null;
    variants = {
      allowedCombinations = [ ];
      dimensions.mode = {
        default = "debug";
        values = [
          "debug"
          "release"
        ];
      };
    };
  };
  expectedBuildGeneration = {
    apiVersion = "nixspace/v1";
    kind = "ActionGenerationStore";
    interfaceVersion = 2;
    root = ".state/tasks/devel/${builtins.hashString "sha256" "alpha:default:build"}";
    layout = actionGenerationLayout;
    identity = {
      apiVersion = "nixspace/v1";
      kind = "ActionTaskIdentity";
      interfaceVersion = 1;
      declaration = expectedBuildDeclaration;
    };
  };
  expectedBuildResult = {
    packageId = "alpha";
    targetId = "default";
    actionId = "build";
    task = "alpha:default:build";
    artifacts = expectedBuildArtifacts.outputs;
  };
  expectedBuildPlan = {
    apiVersion = "nixspace/v1";
    kind = "ActionTask";
    interfaceVersion = 3;
    cwd = "/checkouts/alpha";
    argv = expectedBuildArgv;
    environment = { };
    environmentPaths = { };
    generation = expectedBuildGeneration;
    locks = expectedBuildLocks;
    outputs = expectedBuildOutputs;
    pathPrefixes = [ ];
    result = expectedBuildResult;
  };

  tests = {
    testStableTaskNames = {
      expr = builtins.attrNames tasks;
      expected = [
        "alpha:default:build"
        "alpha:default:docs"
        "alpha:default:generate"
        "alpha:default:test"
        "beta:default:build"
        "beta:default:test"
        "firmware:default:build"
        "qualification:default:test"
        "web:default:build"
        "web:default:test"
      ];
    };
    testDependencyEdges = {
      expr = {
        generate = tasks."alpha:default:generate".after;
        betaBuild = tasks."beta:default:build".after;
        betaTest = tasks."beta:default:test".after;
      };
      expected = {
        generate = [ "alpha:default:build" ];
        betaBuild = [ "alpha:default:build" ];
        betaTest = [ "beta:default:build" ];
      };
    };
    testCanonicalTaskInput = {
      expr = {
        inherit (tasks."alpha:default:build".input)
          actionId
          adapter
          artifacts
          environment
          environmentPaths
          locks
          packageId
          requirements
          targetId
          ;
        source = tasks."alpha:default:build".input.source;
        cwd = tasks."alpha:default:build".cwd;
        modified = tasks."alpha:default:build".execIfModified;
      };
      expected = {
        actionId = "build";
        adapter = "cargo-v1";
        artifacts = {
          inputs = { };
          outputs.bundle = {
            producedBy = "build";
            kind = "directory";
            path = "dist";
            contract = {
              name = "alpha-api";
              version = 1;
            };
          };
        };
        environment = { };
        environmentPaths = { };
        locks = [
          ".state/tasks/locks/cargo-target.lock"
          ".state/tasks/locks/shared-cache.lock"
        ];
        packageId = "alpha";
        requirements = {
          cpu = 2;
          memoryMiB = 1024;
          exclusiveLocks = [
            "shared-cache"
            "cargo-target"
          ];
        };
        targetId = "default";
        source = {
          coordinate = "alpha-source:.";
          input = "alpha-source";
          localPath = "/checkouts/alpha";
          root = ".";
          visibility = "private";
        };
        cwd = "/workspace";
        modified = [ "/checkouts/alpha" ];
      };
    };
    testCanonicalActionTaskPlanAndExec = {
      expr = {
        input = tasks."alpha:default:build".input;
        exec = tasks."alpha:default:build".exec;
      };
      expected = {
        input = expectedBuildDeclaration // {
          generation = expectedBuildGeneration;
        };
        exec = lib.escapeShellArgs [
          nixspaceExecutable
          "_run-task"
          "--plan-json"
          (builtins.toJSON expectedBuildPlan)
        ];
      };
    };
    testRealPresetFlowsIntoTaskProducer = {
      expr = {
        build = {
          inherit (presetTasks."example:default:build") after cwd;
          inherit (presetTasks."example:default:build".input)
            adapter
            argv
            environment
            toolProfileId
            ;
        };
        test = {
          inherit (presetTasks."example:default:test") after cwd;
          inherit (presetTasks."example:default:test".input)
            adapter
            argv
            environment
            toolProfileId
            ;
        };
      };
      expected = {
        build = {
          after = [ ];
          cwd = "/workspace";
          adapter = "cargo-v1";
          argv = [
            "nix"
            "develop"
            "--no-pure-eval"
            "."
            "-c"
            "cargo"
            "build"
            "--workspace"
          ];
          environment = { };
          toolProfileId = null;
        };
        test = {
          after = [ "example:default:build" ];
          cwd = "/workspace";
          adapter = "cargo-v1";
          argv = [
            "nix"
            "develop"
            "--no-pure-eval"
            "."
            "-c"
            "cargo"
            "test"
            "--workspace"
          ];
          environment = { };
          toolProfileId = null;
        };
      };
    };
    testArtifactConsumptionIsScoped = {
      expr = {
        betaBuildAfter = tasks."beta:default:build".after;
        betaBuildInputs = tasks."beta:default:build".input.artifacts.inputs;
        betaBuildPaths = tasks."beta:default:build".input.environmentPaths;
        betaTestArtifacts = tasks."beta:default:test".input.artifacts;
        alphaTestArtifacts = tasks."alpha:default:test".input.artifacts;
        docsArtifacts = tasks."alpha:default:docs".input.artifacts;
      };
      expected = {
        betaBuildAfter = [ "alpha:default:build" ];
        betaBuildInputs.alpha = {
          consumedBy = [ "build" ];
          from = "alpha:default:bundle";
          environment = "ALPHA_BUNDLE";
          contract = {
            name = "alpha-api";
            version = 1;
          };
        };
        betaBuildPaths.ALPHA_BUNDLE = "/checkouts/alpha/dist";
        betaTestArtifacts = {
          inputs = { };
          outputs = { };
        };
        alphaTestArtifacts = {
          inputs = { };
          outputs = { };
        };
        docsArtifacts = {
          inputs = { };
          outputs = { };
        };
      };
    };
    testUnrelatedActionsDoNotInvalidateBuilds = {
      expr = {
        buildUnaffectedByDocs = tasks."alpha:default:build" == docsTasks."alpha:default:build";
        consumerUnaffectedByDocs = tasks."beta:default:build" == docsTasks."beta:default:build";
        buildUnaffectedByTest = tasks."alpha:default:build" == changedTestTasks."alpha:default:build";
        consumerUnaffectedByTest = tasks."beta:default:build" == changedTestTasks."beta:default:build";
        docsChanged = tasks."alpha:default:docs" != docsTasks."alpha:default:docs";
        testChanged = tasks."alpha:default:test" != changedTestTasks."alpha:default:test";
      };
      expected = {
        buildUnaffectedByDocs = true;
        consumerUnaffectedByDocs = true;
        buildUnaffectedByTest = true;
        consumerUnaffectedByTest = true;
        docsChanged = true;
        testChanged = true;
      };
    };
    testLiteralArgumentsAreData = {
      expr = {
        argv = tasks."alpha:default:generate".input.argv;
        environment = tasks."alpha:default:generate".input.environment;
        oneLine = !(lib.hasInfix "\n" tasks."alpha:default:generate".exec);
      };
      expected = {
        argv = [
          "cargo"
          "xtask"
          "generate; touch /tmp/not-allowed"
          "$(not-allowed)"
        ];
        environment.DANGEROUS_LITERAL = "$(not-expanded); still literal";
        oneLine = true;
      };
    };
    testUnboundPathsRemainWorkspaceRelative = {
      expr = {
        cwd = unboundTasks."beta:default:build".cwd;
        environmentPaths = unboundTasks."beta:default:build".input.environmentPaths;
        outputs = unboundTasks."beta:default:build".input.outputs;
      };
      expected = {
        cwd = ".";
        environmentPaths.ALPHA_BUNDLE = "src/alpha-repo/dist";
        outputs = [ ];
      };
    };
    testUnsafeStateRootIsRejected = {
      expr = unsafeStateRoot.success;
      expected = false;
    };
    testResolvedToolProfileIsEmbeddedAsData = {
      expr = {
        inherit (profileTasks."alpha:default:build".input)
          environment
          environmentPaths
          pathPrefixes
          toolProfileId
          ;
      };
      expected = {
        toolProfileId = "fixture-v1";
        pathPrefixes = [
          "/nix/store/00000000000000000000000000000000-compiler/bin"
          "/nix/store/11111111111111111111111111111111-formatter/bin"
        ];
        environment.PKG_CONFIG_PATH = "/nix/store/22222222222222222222222222222222-library/lib/pkgconfig";
        environmentPaths.SCCACHE_DIR = ".state/sccache";
      };
    };
  };

  failures = lib.runTests tests;
in
if failures == [ ] then
  {
    suite = "task-contracts";
    taskCount = builtins.length (builtins.attrNames tasks);
    assertionCount = builtins.length (builtins.attrNames tests);
  }
else
  throw "Nix task contract failures: ${builtins.toJSON failures}"
