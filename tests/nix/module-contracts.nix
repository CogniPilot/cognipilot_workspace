{ pkgs }:

let
  inherit (pkgs) lib;
  fixtures = ../fixtures;
  projectFixtures = fixtures + /project-flakes;

  nixFiles =
    directory:
    builtins.filter (path: lib.hasSuffix ".nix" (toString path)) (
      lib.filesystem.listFilesRecursive directory
    );
  evaluateIndex = path: (lib.evalModules { modules = [ path ]; }).config.cognipilot.validatedIndex;
  forceIndex = path: builtins.tryEval (builtins.deepSeq (evaluateIndex path) true);

  goldenPaths = nixFiles (projectFixtures + /golden);
  invalidPaths = nixFiles (projectFixtures + /invalid);
  goldenResults = map forceIndex goldenPaths;
  invalidResults = map forceIndex invalidPaths;

  cargo = evaluateIndex (projectFixtures + /golden/cargo.nix);
  externalCargo = evaluateIndex (projectFixtures + /golden/external-cargo.nix);
  semanticDag = evaluateIndex (projectFixtures + /golden/semantic-dag.nix);
  artifactArgv = evaluateIndex (projectFixtures + /golden/artifact-argv.nix);
  metadata = evaluateIndex (projectFixtures + /golden/compliance-metadata.nix);
  resources = evaluateIndex (projectFixtures + /golden/resources-actions.nix);
  launch = evaluateIndex (projectFixtures + /golden/launch-ir.nix);
  locked = evaluateIndex (projectFixtures + /golden/resolution-locked.nix);
  launchClosure = evaluateIndex (projectFixtures + /golden/resolution-launch-closure.nix);
  variantPaths = evaluateIndex (projectFixtures + /golden/variant-output-paths.nix);

  presetActionNames = {
    cargo = [
      "build"
      "test"
    ];
    cargo-npm = [
      "cargo-build"
      "cargo-test"
      "npm-build"
      "npm-install"
      "npm-test"
    ];
    cmake = [
      "build"
      "configure"
      "test"
    ];
    npm = [
      "build"
      "test"
    ];
    rumoca = [
      "compiler-build"
      "javascript-build"
      "python-build"
      "test"
    ];
    twister = [
      "build"
      "test"
    ];
    west = [ "build" ];
    zephyr-native-sim = [
      "build"
      "test"
    ];
  };
  actualPresetActions = lib.mapAttrs (
    name: _:
    (evaluateIndex (projectFixtures + "/golden/${name}.nix")).projects.example.targets.default.actions
  ) presetActionNames;
  emptyRequirements = {
    cpu = null;
    memoryMiB = null;
    exclusiveLocks = [ ];
  };
  expectedAction =
    {
      adapter,
      argv,
      dependsOn ? [ ],
      environment ? { },
      kind,
      toolProfile ? null,
    }:
    {
      inherit
        adapter
        argv
        dependsOn
        environment
        kind
        toolProfile
        ;
      requirements = emptyRequirements;
    };
  inProjectShell =
    argv:
    [
      "nix"
      "develop"
      "--no-pure-eval"
      "."
      "-c"
    ]
    ++ argv;
  westTwister =
    output: extra:
    [
      "nixspace"
      "west"
      "run"
      "--cwd"
      "."
      "--"
      "nix"
      "develop"
      "--no-pure-eval"
      "./manifest#default"
      "-c"
      "west"
      "twister"
      "-T"
      "modules/lib/example/tests"
      "-p"
      "native_sim/native/64"
      "--force-platform"
      "--outdir"
      "modules/lib/example/build/twister/${output}"
      "--no-clean"
    ]
    ++ extra;
  expectedPresetActions = {
    cargo = {
      build = expectedAction {
        kind = "build";
        adapter = "cargo-v1";
        argv = inProjectShell [
          "cargo"
          "build"
          "--workspace"
        ];
      };
      test = expectedAction {
        kind = "test";
        adapter = "cargo-v1";
        dependsOn = [ "build" ];
        argv = inProjectShell [
          "cargo"
          "test"
          "--workspace"
        ];
      };
    };
    cargo-npm = {
      npm-install = expectedAction {
        kind = "build";
        adapter = "npm-v1";
        argv = inProjectShell [
          "npm"
          "ci"
        ];
      };
      cargo-build = expectedAction {
        kind = "build";
        adapter = "cargo-v1";
        dependsOn = [ "npm-install" ];
        argv = inProjectShell [
          "cargo"
          "build"
          "--locked"
          "--workspace"
        ];
      };
      npm-build = expectedAction {
        kind = "build";
        adapter = "npm-v1";
        dependsOn = [ "npm-install" ];
        argv = inProjectShell [
          "npm"
          "run"
          "build"
        ];
      };
      npm-test = expectedAction {
        kind = "test";
        adapter = "npm-v1";
        dependsOn = [ "npm-build" ];
        argv = inProjectShell [
          "npm"
          "run"
          "ci"
        ];
      };
      cargo-test = expectedAction {
        kind = "test";
        adapter = "cargo-v1";
        dependsOn = [
          "cargo-build"
          "npm-test"
        ];
        argv = inProjectShell [
          "cargo"
          "test"
          "--locked"
          "--workspace"
        ];
      };
    };
    cmake = {
      configure = expectedAction {
        kind = "generate";
        adapter = "cmake-v1";
        toolProfile = "cmake-v1";
        argv = [
          "cmake"
          "-S"
          "."
          "-B"
          "build"
          "-G"
          "Ninja"
        ];
      };
      build = expectedAction {
        kind = "build";
        adapter = "cmake-v1";
        toolProfile = "cmake-v1";
        dependsOn = [ "configure" ];
        argv = [
          "cmake"
          "--build"
          "build"
        ];
      };
      test = expectedAction {
        kind = "test";
        adapter = "cmake-v1";
        toolProfile = "cmake-v1";
        dependsOn = [ "build" ];
        argv = [
          "ctest"
          "--test-dir"
          "build"
          "--output-on-failure"
        ];
      };
    };
    npm = {
      build = expectedAction {
        kind = "build";
        adapter = "npm-v1";
        argv = inProjectShell [
          "npm"
          "run"
          "build"
        ];
      };
      test = expectedAction {
        kind = "test";
        adapter = "npm-v1";
        dependsOn = [ "build" ];
        argv = inProjectShell [
          "npm"
          "test"
        ];
      };
    };
    rumoca = {
      compiler-build = expectedAction {
        kind = "build";
        adapter = "nix-flake-package-v1";
        argv = [
          "nix"
          "build"
          "--no-pure-eval"
          ".#rumoca"
          "--out-link"
          "result-rumoca"
        ];
      };
      python-build = expectedAction {
        kind = "build";
        adapter = "nix-flake-package-v1";
        argv = [
          "nix"
          "build"
          "--no-pure-eval"
          ".#rumoca-python-env"
          "--out-link"
          "result-rumoca-python"
        ];
      };
      javascript-build = expectedAction {
        kind = "build";
        adapter = "npm-v1";
        argv = inProjectShell [
          "npm"
          "--prefix"
          "packages/rumoca"
          "run"
          "build:dev"
        ];
      };
      test = expectedAction {
        kind = "test";
        adapter = "nix-flake-check-v1";
        argv = [
          "nix"
          "flake"
          "check"
          "--no-pure-eval"
          "."
        ];
      };
    };
    twister = {
      build = expectedAction {
        kind = "build";
        adapter = "twister-v1";
        environment = {
          NIX_HARDENING_ENABLE = "";
          ZEPHYR_TOOLCHAIN_VARIANT = "host";
        };
        argv = westTwister "build" [ "--build-only" ];
      };
      test = expectedAction {
        kind = "test";
        adapter = "twister-v1";
        dependsOn = [ "build" ];
        environment = {
          NIX_HARDENING_ENABLE = "";
          ZEPHYR_TOOLCHAIN_VARIANT = "host";
        };
        argv = westTwister "test" [ ];
      };
    };
    west.build = expectedAction {
      kind = "build";
      adapter = "west-v1";
      argv = inProjectShell [
        "west"
        "build"
      ];
    };
    zephyr-native-sim = {
      build = expectedAction {
        kind = "build";
        adapter = "nixspace-west-v1";
        environment.ZEPHYR_TOOLCHAIN_VARIANT = "host";
        argv = [
          "nixspace"
          "west"
          "exec"
          "build"
          "-b"
          "native_sim/native/64"
          "-d"
          "example/build-native_sim_native_64_sil"
          "example"
          "-p"
          "auto"
          "--"
          "-DEXTRA_CONF_FILE=example/boards/native_sim_sil.conf"
        ];
      };
      test = expectedAction {
        kind = "test";
        adapter = "nixspace-west-app-v1";
        dependsOn = [ "build" ];
        argv = [
          "nixspace"
          "west"
          "run"
          "--"
          "env"
          "CUBS2_WORKSPACE_ROOT=."
          "nix"
          "run"
          "--no-pure-eval"
          "./example#native-sim-64-sil-test"
        ];
      };
    };
  };

  support = { lib, ... }: {
    options.perSystem = lib.mkOption {
      type = lib.types.raw;
      default = { };
    };
  };
  evaluateCompliance =
    path:
    (lib.evalModules {
      modules = [
        support
        path
      ];
    }).config.cognipilot.complianceReport;
  validCompliance = evaluateCompliance (fixtures + /compliance/valid.nix);
  approvedCompliance = evaluateCompliance (fixtures + /compliance/approved-bespoke.nix);
  privateCompliance = evaluateCompliance (fixtures + /compliance/private-excluded.nix);
  invalidCompliance = path: builtins.tryEval (builtins.deepSeq (evaluateCompliance path) true);

  tests = {
    testGoldenFixturesEvaluate = {
      expr = builtins.all (result: result.success) goldenResults;
      expected = true;
    };
    testInvalidFixturesFailClosed = {
      expr = builtins.all (result: !result.success) invalidResults;
      expected = true;
    };
    testPresetActionSurface = {
      expr = lib.mapAttrs (_: actions: builtins.attrNames actions) actualPresetActions;
      expected = presetActionNames;
    };
    testPresetActionSemantics = {
      expr = actualPresetActions;
      expected = expectedPresetActions;
    };
    testExternalDefinitionMatchesInTreeDefinition = {
      expr = externalCargo;
      expected = cargo;
    };
    testActionPlansArePrecomputedTaskRoots = {
      expr = semanticDag.actionPlans.runner;
      expected = {
        kind = "devenv-task";
        direct = {
          argv = [
            "devenv-flake-tasks"
            "run"
          ];
          requiredEnvironment = [
            "DEVENV_TASK_FILE"
            "NIXSPACE_INDEX"
            "NIXSPACE_WORKSPACE_ROOT"
          ];
        };
        bootstrap = {
          argv = [
            "nix"
            "develop"
            "--no-pure-eval"
            ".#default"
            "--command"
            "devenv-flake-tasks"
            "run"
          ];
          environment.NIXSPACE_WORKSPACE_ROOT = "workspace-root";
        };
      };
    };
    testTypedArtifactArgument = {
      expr = builtins.elemAt artifactArgv.projects.consumer.targets.default.actions.bind.argv 1;
      expected = {
        artifactInput = "api";
        prefix = "--api=";
        suffix = "/schema";
      };
    };
    testVariantOutputPathsDoNotCollide = {
      expr = map (target: variantPaths.projects.firmware.targets.${target}.artifacts.outputs.image.path) [
        "native-sim"
        "native-sim-64"
      ];
      expected = [
        "build/native-sim/zephyr/zephyr.bin"
        "build/native-sim-64/zephyr/zephyr.bin"
      ];
    };
    testComplianceMetadataNormalizes = {
      expr = {
        inherit (metadata.projects.flight-integration)
          aliases
          deployability
          lifecycle
          owner
          packageId
          softwareVersion
          ;
        inherit (metadata.projects.flight-integration.license) spdx;
        warnings = metadata.projects.flight-integration.compliance.warnings;
      };
      expected = {
        aliases = [
          "autopilot"
          "fc"
        ];
        deployability = "deployable";
        lifecycle = "stable";
        owner = "CogniPilot Foundation";
        packageId = "flight-control";
        softwareVersion = {
          source = "file";
          file = "VERSION";
          value = null;
        };
        spdx = "(Apache-2.0 OR MIT) AND BSD-3-Clause";
        warnings = [ ];
      };
    };
    testResourceAndExecutableContractsNormalize = {
      expr = {
        requirements = resources.projects.app.targets.default.actions.build.requirements;
        resource = resources.projects.app.resources.default-config;
        executable = resources.projects.app.executables.app;
      };
      expected = {
        requirements = {
          cpu = 2;
          memoryMiB = 1024;
          exclusiveLocks = [
            "cargo-target"
            "usb-device"
          ];
        };
        resource = {
          kind = "configuration";
          path = "config/default.json";
        };
        executable = {
          from = "app:default:cli";
          argv = [
            "--config"
            "config/default.json"
          ];
        };
      };
    };
    testLaunchEndpointMetadataSurvivesNormalization = {
      expr = launch.projects.app.launches.router.processes.router.endpoints.http;
      expected = {
        protocol = "http";
        hostParameter = "host";
        portParameter = "port";
        path = "/ready";
        expectedStatus = 204;
      };
    };
    testResolutionTemplateIsTyped = {
      expr = {
        inherit (semanticDag.resolutionTemplate) apiVersion interfaceVersion kind;
        flightClosure = semanticDag.resolutionTemplate.packagePlans.flight.dependencyClosure;
        producerTask =
          semanticDag.resolutionTemplate.artifacts."codegen:schema:headers".candidates.local.generation.producerTask;
      };
      expected = {
        apiVersion = "nixspace/v1";
        kind = "WorkspaceResolutionTemplate";
        interfaceVersion = 1;
        flightClosure = [
          "codegen"
          "flight"
        ];
        producerTask = "codegen:schema:build";
      };
    };
    testLockedCandidatesUseReleaseOutputs = {
      expr = locked.resolutionTemplate.packages.runtime.candidates.locked;
      expected = {
        kind = "nix-output-reference";
        installable = ".#target-runtime--default";
        provider = "runtime_release";
        package = "runtime";
        relativePath = ".";
        targetId = "default";
        provenance = {
          kind = "locked-output";
          label = "LOCKED";
          provider = "runtime_release";
          package = "runtime";
        };
      };
    };
    testLaunchChildrenEnterSelectionClosure = {
      expr = launchClosure.resolutionTemplate.packagePlans.app.dependencyClosure;
      expected = [
        "app"
        "worker"
      ];
    };
    testValidComplianceReport = {
      expr = {
        inherit (validCompliance) compliant schemaVersion;
        inherit (validCompliance.summary) bespokeAdapterCount selectedPackageCount warningCount;
      };
      expected = {
        compliant = true;
        schemaVersion = 1;
        bespokeAdapterCount = 0;
        selectedPackageCount = 1;
        warningCount = 0;
      };
    };
    testBespokeApprovalIsExact = {
      expr = approvedCompliance.bespoke.approvals;
      expected = [ "flight-control:default:package" ];
    };
    testPrivatePackagesCanBeExcluded = {
      expr = {
        compliant = privateCompliance.compliant;
        enforced = privateCompliance.packages.legacy-flight.enforced;
        visibilities = privateCompliance.policy.enforcedVisibilities;
      };
      expected = {
        compliant = true;
        enforced = false;
        visibilities = [ "public" ];
      };
    };
    testInvalidComplianceFailsClosed = {
      expr = builtins.all (result: !result.success) (
        map invalidCompliance [
          (fixtures + /compliance/invalid.nix)
          (fixtures + /compliance/invalid-approvals.nix)
        ]
      );
      expected = true;
    };
  };

  failures = lib.runTests tests;
in
if failures == [ ] then
  {
    suite = "module-contracts";
    goldenFixtureCount = builtins.length goldenPaths;
    invalidFixtureCount = builtins.length invalidPaths;
    assertionCount = builtins.length (builtins.attrNames tests);
  }
else
  throw "Nix module contract failures: ${builtins.toJSON failures}"
