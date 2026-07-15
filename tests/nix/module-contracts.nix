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

  presetActions = {
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
    builtins.attrNames
      (evaluateIndex (projectFixtures + "/golden/${name}.nix")).projects.example.targets.default.actions
  ) presetActions;

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
      expr = actualPresetActions;
      expected = presetActions;
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
        bootstrap.argv = [
          "nix"
          "develop"
          "--no-pure-eval"
          ".#default"
          "--command"
          "devenv-flake-tasks"
          "run"
        ];
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
