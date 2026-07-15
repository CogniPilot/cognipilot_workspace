{
  inputs,
  pkgs,
  rootConfig,
  system,
  systemConfig ? null,
}:

let
  inherit (pkgs) lib;
  cachePolicy = import ../../nix/cognipilot/cache-policy.nix {
    devenvInput = inputs.devenv;
  };
  flakeNixConfig = (import ../../flake.nix).nixConfig;
  providerIndex = rootConfig.cognipilot.validatedIndex;
  genericIndex = rootConfig.flake.nixspaceIndex;
  sourcePlan = rootConfig.flake.nixspaceSourcePlan;
  hostPlan = rootConfig.flake.nixspaceHostPlan;
  workspacePolicy = rootConfig.cognipilot.workspacePolicy.report;
  benchmarkPlan = systemConfig.packages.nixspace-benchmark-plan.passthru.document;
  nativeWarmCaseNames = builtins.filter (name: lib.hasPrefix "native-warm-" name) (
    builtins.attrNames benchmarkPlan.cases
  );
  nativeWarmPackages = map (lib.removePrefix "native-warm-") nativeWarmCaseNames;
  nativeWarmBudgets = lib.genAttrs nativeWarmPackages (
    package: benchmarkPlan.cases."native-warm-${package}".gates
  );
  promotionSbom = systemConfig.packages.promotion-sbom.passthru.document;
  cacheProjection = systemConfig.packages.public-cache-root.passthru.projection;
  defaultDevenvShell = systemConfig.devenv.shells.default;

  expectedCatalogIds = [
    "cerebri_cubs2"
    "cerebri_modules"
    "cerebri_rdd2"
    "csyn"
    "csyn_ros2_bridge"
    "electrode_web"
    "fastdyn"
    "modelica_models"
    "qualisys_rust_sdk"
    "rumoca"
    "synapse_fbs"
    "synapse_ppm_bridge"
    "synapse_qualisys_bridge"
    "zros"
    "zros_drivers"
  ];
  expectedPublicPackageIds = builtins.filter (id: id != "fastdyn") expectedCatalogIds;
  expectedPublicCheckNames = [
    "cognipilot-compliance"
    "cognipilot-contract-tests"
    "cognipilot-promotion-attestation"
    "cognipilot-promotion-record"
    "cognipilot-promotion-sbom"
    "cognipilot-workspace-policy"
    "nixspace-interface"
    "nixspace-standalone"
    "nixspace-west-plan"
    "launch-electrode_web--ground-station-config"
    "launch-electrode_web--simulation-config"
    "launch-electrode_web--simulation-stack-config"
    "launch-electrode_web--simulation-stack-mocap-config"
    "launch-synapse_qualisys_bridge--mocap-config"
  ];
  expectedSelectedInputLinks = [
    "input-product-root"
  ]
  ++ lib.concatMap (id: [
    "input-${id}--source"
    "input-${id}--definition"
  ]) expectedPublicPackageIds;
  expectedPublicCacheLinks = [
    "workspace"
    "nixspace-index"
    "nixspace"
    "nixspace-completions"
    "sccache-tools"
    "ws"
    "nixspace-host"
    "nixspace-benchmark-plan"
    "nixspace-host-plan"
    "nixspace-launch-plan"
    "nixspace-resolution-plan"
    "nixspace-source-plan"
    "nixspace-west-plan"
  ]
  ++ expectedSelectedInputLinks
  ++ [ "synapse_ppm_bridge--default" ]
  ++ map (name: "check-${name}") expectedPublicCheckNames;
  expectedSourceProjection = lib.genAttrs expectedCatalogIds (
    id:
    if id == "fastdyn" then
      {
        branch = "fix/cerebri-fastdyn-lockstep";
        input = "fastdyn_source";
        packages = [ "fastdyn" ];
        path = "src/FastDyn";
        roots = [ "." ];
        visibility = "private";
      }
    else
      {
        branch = "main";
        input = "${id}_source";
        packages = [ id ];
        path = "src/${id}";
        roots = [ "." ];
        visibility = "public";
      }
  );
  expectedSubstituters = [
    "https://cache.nixos.org"
    "https://devenv.cachix.org"
    "https://cachix.cachix.org"
    "https://cognipilot.cachix.org"
    "https://ros.cachix.org"
  ];
  expectedPublicKeys = [
    "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
    "devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw="
    "cachix.cachix.org-1:eWNHQldwUO7G2VkjpnjDbWwy4KQ/HNxht7H4SSoMckM="
    "cognipilot.cachix.org-1:TSm+h3sAVYP0B+D+1uG5hvgI/4JFAS3LFvv/yhTwvK8="
    "ros.cachix.org-1:dSyZxI8geDCJrwgvCOHDoAfOm5sV1wCPjBkKL+38Rvo="
  ];

  sourceRoot = inputs.self.outPath;
  textSuffixes = [
    ".md"
    ".nix"
    ".rs"
    ".toml"
  ];
  reusableRoots = [
    (sourceRoot + "/nix/nixspace")
    (sourceRoot + "/tools/nixspace/src")
    (sourceRoot + "/tools/nixspace/tests")
  ];
  reusableFiles =
    builtins.filter (path: builtins.any (suffix: lib.hasSuffix suffix (toString path)) textSuffixes)
      (
        lib.concatMap lib.filesystem.listFilesRecursive reusableRoots
        ++ [
          (sourceRoot + "/tools/nixspace/Cargo.toml")
          (sourceRoot + "/tools/nixspace/README.md")
        ]
      );
  containsProductName =
    text:
    builtins.any (needle: lib.hasInfix needle text) [
      "cognipilot"
      "CogniPilot"
      "COGNIPILOT"
    ];
  packageShape =
    package:
    builtins.attrNames package == [
      "aliases"
      "extensions"
      "id"
    ]
    && builtins.hasAttr "org.cognipilot/package-v1" package.extensions;

  catalogIds = map (package: package.id) genericIndex.catalog.packages;
  sourceProjection = lib.mapAttrs (id: repository: {
    inherit (repository) packages path;
    inherit (repository.git) branch;
    inherit (repository.source) input roots;
    visibility = providerIndex.projects.${id}.source.visibility;
  }) sourcePlan.repositories;
  releasedTargets = lib.concatMap (
    project:
    lib.concatMap (target: lib.optional (target.release != null) "${project.packageId}:${target.id}") (
      builtins.attrValues project.targets
    )
  ) (builtins.attrValues providerIndex.projects);
  sbomPackageNames = map (package: package.name) promotionSbom.packages;
  sbomProjectIds = builtins.tail sbomPackageNames;
  sbomRelationshipIds = map (
    relationship: relationship.relatedSpdxElement
  ) promotionSbom.relationships;

  workflowFiles = builtins.filter (
    path:
    let
      value = toString path;
    in
    lib.hasSuffix ".yml" value || lib.hasSuffix ".yaml" value
  ) (lib.filesystem.listFilesRecursive (sourceRoot + "/.github/workflows"));
  workflowLines = lib.concatMap (path: lib.splitString "\n" (builtins.readFile path)) workflowFiles;
  actionLines = builtins.filter (
    line: builtins.match "[[:space:]]*(-[[:space:]]+)?uses:.*" line != null
  ) workflowLines;
  pinnedAction =
    line:
    builtins.match "[[:space:]]*(-[[:space:]]+)?uses:[[:space:]]+[^[:space:]@]+@[0-9a-f]{40}([[:space:]]+#.*)?" line
    != null;
  ci = builtins.readFile (sourceRoot + "/.github/workflows/ci.yml");
  ciLines = lib.splitString "\n" ci;
  indexedCiLines = lib.imap0 (index: line: { inherit index line; }) ciLines;
  isComment = line: builtins.match "[[:space:]]*#.*" line != null;
  ciCodeLines = builtins.filter (line: line != "" && !(isComment line)) ciLines;
  jobName =
    line:
    let
      matched = builtins.match "  ([A-Za-z0-9_-]+):[[:space:]]*" line;
    in
    if matched == null then null else builtins.head matched;
  stepName =
    line:
    let
      matched = builtins.match "      - name:[[:space:]]+(.+)" line;
    in
    if matched == null then null else builtins.head matched;
  jobStarts = builtins.filter (entry: jobName entry.line != null) indexedCiLines;
  nextBoundary =
    entries: start: fallback:
    (lib.findFirst (entry: entry.index > start) { index = fallback; } entries).index;
  jobBlock =
    name:
    let
      start = lib.findFirst (entry: jobName entry.line == name) null jobStarts;
      end = if start == null then 0 else nextBoundary jobStarts start.index (builtins.length ciLines);
    in
    if start == null then [ ] else lib.sublist start.index (end - start.index) ciLines;
  stepBlocksFor =
    job:
    let
      lines = jobBlock job;
      indexed = lib.imap0 (index: line: { inherit index line; }) lines;
      starts = builtins.filter (entry: stepName entry.line != null) indexed;
    in
    map (
      start:
      let
        end = nextBoundary starts start.index (builtins.length lines);
      in
      {
        name = stepName start.line;
        lines = lib.sublist start.index (end - start.index) lines;
      }
    ) starts;
  validateSteps = stepBlocksFor "validate";
  substituteSteps = stepBlocksFor "substitute";
  allCiSteps = lib.concatMap stepBlocksFor (map (entry: jobName entry.line) jobStarts);
  stepByName =
    steps: name:
    builtins.filter (line: line != "" && !(isComment line)) (
      (lib.findFirst (step: step.name == name) { lines = [ ]; } steps).lines
    );
  validateStep = stepByName validateSteps;
  substituteStep = stepByName substituteSteps;
  matrixProjection =
    job:
    let
      lines = jobBlock job;
      indexed = lib.imap0 (index: line: { inherit index line; }) lines;
      systems = builtins.filter (
        entry: builtins.match "          - system:[[:space:]]+([^[:space:]]+)" entry.line != null
      ) indexed;
    in
    map (
      entry:
      let
        systemMatch = builtins.match "          - system:[[:space:]]+([^[:space:]]+)" entry.line;
        runnerLine = builtins.elemAt lines (entry.index + 1);
        runnerMatch = builtins.match "            runner:[[:space:]]+([^[:space:]]+)" runnerLine;
      in
      {
        system = builtins.head systemMatch;
        runner = if runnerMatch == null then null else builtins.head runnerMatch;
      }
    ) systems;
  expectedNativeMatrix = [
    {
      system = "x86_64-linux";
      runner = "ubuntu-24.04";
    }
    {
      system = "aarch64-linux";
      runner = "ubuntu-24.04-arm";
    }
    {
      system = "aarch64-darwin";
      runner = "macos-15";
    }
  ];
  tokenLine = "          CACHIX_AUTH_TOKEN: \${{ secrets.CACHIX_AUTH_TOKEN }}";
  tokenSteps = builtins.filter (step: builtins.elem tokenLine step.lines) allCiSteps;
  setup = builtins.readFile (sourceRoot + "/setup");
  ws = builtins.readFile (sourceRoot + "/ws");

  tests = {
    testDevenvDoesNotSnapshotEditableWorkspace = {
      expr = {
        shellContainerRoots = map toString defaultDevenvShell.containers.shell.copyToRoot;
        processContainerRoots = map toString defaultDevenvShell.containers.processes.copyToRoot;
        filteredSelfExcludesEditableState = builtins.all (
          name: !(builtins.pathExists "${inputs.self.outPath}/${name}")
        ) [
          ".devenv"
          ".git"
          "src"
        ];
      };
      expected = {
        shellContainerRoots = [ (toString inputs.self.outPath) ];
        processContainerRoots = [ (toString inputs.self.outPath) ];
        filteredSelfExcludesEditableState = true;
      };
    };
    testCachePolicyIsSingleAuthority = {
      expr = {
        flake = flakeNixConfig;
        policy = cachePolicy.flakeNixConfig;
        hostSubstituters = cachePolicy.hostPlan.nix.settings.extra-substituters;
        hostKeys = cachePolicy.hostPlan.nix.settings.extra-trusted-public-keys;
        hostTrustedSubstituters = cachePolicy.hostPlan.nix.settings.extra-trusted-substituters;
        observedStores = hostPlan.readiness.cache.stores;
        observedRoots = hostPlan.readiness.cache.roots;
        trust = cachePolicy.hostPlan.nix.settings.trusted-users;
        autoOptimiseStore = cachePolicy.hostPlan.nix.settings.auto-optimise-store;
        devenvPin = cachePolicy.devenvPin;
        devenvTool = cachePolicy.hostPlan.tools.devenv;
      };
      expected = {
        flake = {
          extra-substituters = builtins.tail expectedSubstituters;
          extra-trusted-public-keys = builtins.tail expectedPublicKeys;
        };
        policy = {
          extra-substituters = builtins.tail expectedSubstituters;
          extra-trusted-public-keys = builtins.tail expectedPublicKeys;
        };
        hostSubstituters = expectedSubstituters;
        hostTrustedSubstituters = expectedSubstituters;
        hostKeys = expectedPublicKeys;
        observedStores = [
          {
            name = "nixos";
            uri = "https://cache.nixos.org";
          }
          {
            name = "cognipilot";
            uri = "https://cognipilot.cachix.org";
          }
        ];
        observedRoots = [
          {
            name = "public-cache-root";
            path = "result-public-cache-root";
          }
        ];
        trust = [ "*" ];
        autoOptimiseStore = true;
        devenvPin = {
          revision = inputs.devenv.rev;
          version =
            (builtins.fromTOML (builtins.readFile (inputs.devenv.outPath + "/Cargo.toml")))
            .workspace.package.version;
          installable = "github:cachix/devenv/${inputs.devenv.rev}";
        };
        devenvTool = {
          executable = "devenv";
          expectedVersion =
            (builtins.fromTOML (builtins.readFile (inputs.devenv.outPath + "/Cargo.toml")))
            .workspace.package.version;
          versionArgv = [
            "devenv"
            "version"
          ];
          installArgv = [
            "nix"
            "profile"
            "add"
            "--accept-flake-config"
            "github:cachix/devenv/${inputs.devenv.rev}"
          ];
        };
      };
    };
    testProviderAndGenericInterfacesCompose = {
      expr = {
        provider = {
          inherit (providerIndex) apiVersion interfaceVersion kind;
        };
        generic = {
          inherit (genericIndex) apiVersion interfaceVersion kind;
        };
        packageIds = catalogIds;
        providerPackageIds = builtins.sort builtins.lessThan (
          map (project: project.packageId) (builtins.attrValues providerIndex.projects)
        );
        packagesAreNamespaced = builtins.all packageShape genericIndex.catalog.packages;
      };
      expected = {
        provider = {
          apiVersion = "nixspace/v1";
          interfaceVersion = 1;
          kind = "Workspace";
        };
        generic = {
          apiVersion = "nixspace/v1";
          interfaceVersion = 2;
          kind = "Workspace";
        };
        packageIds = expectedCatalogIds;
        providerPackageIds = expectedCatalogIds;
        packagesAreNamespaced = true;
      };
    };
    testProductionSourceProjectionIsExact = {
      expr = {
        inherit sourceProjection;
        all = sourcePlan.plans.all;
        default = sourcePlan.plans.default;
        packageSelectors = builtins.attrNames sourcePlan.plans.packages;
      };
      expected = {
        sourceProjection = expectedSourceProjection;
        all = expectedCatalogIds;
        default = expectedPublicPackageIds;
        packageSelectors = expectedCatalogIds;
      };
    };
    testPublicReleaseProjectionIsExact = {
      expr = {
        inherit releasedTargets sbomProjectIds;
        sbomName = promotionSbom.name;
        sbomRelationshipIds = sbomRelationshipIds;
        sbomRelationshipTypes = lib.unique (
          map (relationship: relationship.relationshipType) promotionSbom.relationships
        );
      };
      expected = {
        releasedTargets = [ "synapse_ppm_bridge:default" ];
        sbomName = "cognipilot-development-${system}";
        sbomProjectIds = expectedPublicPackageIds;
        sbomRelationshipIds = map (
          id: "SPDXRef-Package-${builtins.replaceStrings [ "_" ] [ "-" ] id}"
        ) expectedPublicPackageIds;
        sbomRelationshipTypes = [ "DEPENDS_ON" ];
      };
    };
    testPublicCacheProjectionIsExact = {
      expr = cacheProjection;
      expected = {
        links = expectedPublicCacheLinks;
        selectedInputs = expectedSelectedInputLinks;
        releases = [ "synapse_ppm_bridge--default" ];
        checks = expectedPublicCheckNames;
      };
    };
    testReusableNixspaceContainsNoProductPolicy = {
      expr = {
        productNameAbsent = builtins.all (
          path: !(containsProductName (builtins.readFile path))
        ) reusableFiles;
        devenvStateAbsent = builtins.all (
          path: !(lib.hasInfix ".devenv/" (builtins.readFile path))
        ) reusableFiles;
      };
      expected = {
        productNameAbsent = true;
        devenvStateAbsent = true;
      };
    };
    testWorkspaceControlPlaneUsesNixAndRust = {
      expr = {
        compliant = workspacePolicy.compliant;
        python = workspacePolicy.policy.python;
        staticAuthority = workspacePolicy.policy.staticAuthority;
        violations = workspacePolicy.violations;
      };
      expected = {
        compliant = true;
        python = "project-native-under-src-only";
        staticAuthority = "nix";
        violations = [ ];
      };
    };
    testBenchmarkPlanIsNixOwnedAndPortable = {
      expr = {
        inherit (benchmarkPlan) apiVersion interfaceVersion kind;
        inherit system;
        defaultCases = benchmarkPlan.defaultCases;
        limits = benchmarkPlan.limits;
        inherit nativeWarmPackages;
        inherit nativeWarmBudgets;
        allNativeWarmCasesBuildable = builtins.all (
          package: genericIndex.actionPlans.actions.build.packages.${package} != [ ]
        ) nativeWarmPackages;
        hasNativeWarmCases = nativeWarmPackages != [ ];
      };
      expected = {
        apiVersion = "nixspace/v1";
        interfaceVersion = 4;
        kind = "BenchmarkPlan";
        inherit system;
        limits.maxSamplesPerPhase = 1000;
        defaultCases = [
          "help"
          "completion-backend"
          "package-list"
          "graph-plan"
          "launch-list"
          "launch-plan"
          "module-index-100"
          "ws-build-plan"
        ];
        nativeWarmPackages = lib.optionals (system == "x86_64-linux") [
          "cerebri_cubs2"
          "cerebri_modules"
          "cerebri_rdd2"
          "csyn"
          "electrode_web"
          "modelica_models"
          "qualisys_rust_sdk"
          "rumoca"
          "synapse_fbs"
          "synapse_ppm_bridge"
          "synapse_qualisys_bridge"
          "zros"
        ];
        nativeWarmBudgets = lib.optionalAttrs (system == "x86_64-linux") {
          cerebri_cubs2 = {
            p50Milliseconds = 6000;
            p95Milliseconds = 6000;
          };
          cerebri_modules = {
            p50Milliseconds = 2000;
            p95Milliseconds = 2000;
          };
          cerebri_rdd2 = {
            p50Milliseconds = 10000;
            p95Milliseconds = 10000;
          };
          csyn = {
            p50Milliseconds = 2000;
            p95Milliseconds = 2000;
          };
          electrode_web = {
            p50Milliseconds = 4000;
            p95Milliseconds = 4000;
          };
          modelica_models = {
            p50Milliseconds = 2000;
            p95Milliseconds = 2000;
          };
          qualisys_rust_sdk = {
            p50Milliseconds = 1000;
            p95Milliseconds = 1000;
          };
          rumoca = {
            p50Milliseconds = 2000;
            p95Milliseconds = 2000;
          };
          synapse_fbs = {
            p50Milliseconds = 1000;
            p95Milliseconds = 1000;
          };
          synapse_ppm_bridge = {
            p50Milliseconds = 2000;
            p95Milliseconds = 2000;
          };
          synapse_qualisys_bridge = {
            p50Milliseconds = 2000;
            p95Milliseconds = 2000;
          };
          zros = {
            p50Milliseconds = 2000;
            p95Milliseconds = 2000;
          };
        };
        allNativeWarmCasesBuildable = true;
        hasNativeWarmCases = system == "x86_64-linux";
      };
    };
    testActionsAreCommitPinned = {
      expr = builtins.all pinnedAction actionLines;
      expected = true;
    };
    testCiCachePublicationBoundary = {
      expr = {
        validateMatrix = matrixProjection "validate";
        substituteMatrix = matrixProjection "substitute";
        cacheReadStep = validateStep "Configure the public cache for reads";
        publishStep = validateStep "Publish the explicit public closure";
        retentionStep = validateStep "Retain bounded main revisions";
        substitutionStep = substituteStep "Substitute the complete public root without builders";
        tokenStepNames = map (step: step.name) tokenSteps;
        executableCachixWatchLines = builtins.filter (
          line: builtins.match ".*cachix[[:space:]]+watch.*" line != null
        ) ciCodeLines;
      };
      expected = {
        validateMatrix = expectedNativeMatrix;
        substituteMatrix = expectedNativeMatrix;
        cacheReadStep = [
          "      - name: Configure the public cache for reads"
          "        uses: cachix/cachix-action@5f2d7c5294214f71b873db4b969586b980625e71 # v17"
          "        with:"
          "          name: cognipilot"
          "          skipPush: true"
        ];
        publishStep = [
          "      - name: Publish the explicit public closure"
          "        if: github.event_name == 'push' && github.ref == 'refs/heads/main'"
          "        env:"
          tokenLine
          "        run: cachix push cognipilot ./result-public-cache-root"
        ];
        retentionStep = [
          "      - name: Retain bounded main revisions"
          "        if: github.event_name == 'push' && github.ref == 'refs/heads/main'"
          "        env:"
          tokenLine
          "        run: >-"
          "          cachix pin cognipilot \"main-\${{ matrix.system }}\""
          "          \"$(nix path-info ./result-public-cache-root)\""
          "          --keep-revisions 30"
        ];
        substitutionStep = [
          "      - name: Substitute the complete public root without builders"
          "        run: >-"
          "          nix build --accept-flake-config --print-build-logs"
          "          --max-jobs 0 --builders \"\" --option fallback false"
          "          --option substituters"
          "          \"https://cache.nixos.org https://cognipilot.cachix.org\""
          "          --out-link result-public-cache-root"
          "          \".#packages.\${{ matrix.system }}.public-cache-root\""
        ];
        tokenStepNames = [
          "Publish the explicit public closure"
          "Retain bounded main revisions"
        ];
        executableCachixWatchLines = [ ];
      };
    };
    testBootstrapHasNoOrchestrationFallback = {
      expr = {
        setupRealizesHost = lib.hasInfix ''"$root#nixspace-host"'' setup;
        setupRealizesClient = lib.hasInfix ''"$root#ws"'' setup;
        setupDelegatesDoctor = lib.hasInfix ''exec "$client" --workspace-root "$root" doctor'' setup;
        checkUsesCachedClient = lib.hasInfix ''exec "$client" --workspace-root "$root" setup --check'' setup;
        wrapperDelegates = lib.hasInfix ''exec "$client" --workspace-root "$root" "$@"'' ws;
        noPython = !(lib.hasInfix "python" setup) && !(lib.hasInfix "python" ws);
      };
      expected = {
        setupRealizesHost = true;
        setupRealizesClient = true;
        setupDelegatesDoctor = true;
        checkUsesCachedClient = true;
        wrapperDelegates = true;
        noPython = true;
      };
    };
  };

  failures = lib.runTests tests;
in
if failures == [ ] then
  {
    suite = "root-contracts";
    assertionCount = builtins.length (builtins.attrNames tests);
    actionCount = builtins.length actionLines;
    projectCount = builtins.length (builtins.attrNames providerIndex.projects);
  }
else
  throw "Nix root contract failures: ${builtins.toJSON failures}"
