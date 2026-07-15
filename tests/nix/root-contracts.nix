{
  inputs,
  pkgs,
  rootConfig,
  system,
  systemConfig ? null,
}:

let
  inherit (pkgs) lib;
  cachePolicy = import ../../nix/cognipilot/cache-policy.nix;
  flakeNixConfig = (import ../../flake.nix).nixConfig;
  providerIndex = rootConfig.cognipilot.validatedIndex;
  genericIndex = rootConfig.flake.nixspaceIndex;
  workspacePolicy = rootConfig.cognipilot.workspacePolicy.report;
  benchmarkPlan = systemConfig.packages.nixspace-benchmark-plan.passthru.document;

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

  workflowFiles = builtins.filter (path: lib.hasSuffix ".yml" (toString path)) (
    lib.filesystem.listFilesRecursive (sourceRoot + "/.github/workflows")
  );
  workflowLines = lib.concatMap (path: lib.splitString "\n" (builtins.readFile path)) workflowFiles;
  actionLines = builtins.filter (
    line: builtins.match "[[:space:]]*(-[[:space:]]+)?uses:.*" line != null
  ) workflowLines;
  pinnedAction =
    line:
    builtins.match "[[:space:]]*(-[[:space:]]+)?uses:[[:space:]]+[^[:space:]@]+@[0-9a-f]{40}([[:space:]]+#.*)?" line
    != null;
  ci = builtins.readFile (sourceRoot + "/.github/workflows/ci.yml");
  setup = builtins.readFile (sourceRoot + "/setup");
  ws = builtins.readFile (sourceRoot + "/ws");

  tests = {
    testCachePolicyIsSingleAuthority = {
      expr = {
        flake = flakeNixConfig;
        policy = cachePolicy.flakeNixConfig;
        hostSubstituters = cachePolicy.hostPlan.nix.settings.extra-substituters;
        hostKeys = cachePolicy.hostPlan.nix.settings.extra-trusted-public-keys;
        trust = cachePolicy.hostPlan.nix.settings.trusted-users;
      };
      expected = {
        flake = cachePolicy.flakeNixConfig;
        policy = cachePolicy.flakeNixConfig;
        hostSubstituters = cachePolicy.substituters;
        hostKeys = cachePolicy.publicKeys;
        trust = [ "*" ];
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
        packageIds = map (package: package.id) genericIndex.catalog.packages;
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
        packageIds = builtins.sort builtins.lessThan (
          map (project: project.packageId) (builtins.attrValues providerIndex.projects)
        );
        providerPackageIds = builtins.sort builtins.lessThan (
          map (project: project.packageId) (builtins.attrValues providerIndex.projects)
        );
        packagesAreNamespaced = true;
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
        hasNativeWarmCases = builtins.any (name: lib.hasPrefix "native-warm-" name) (
          builtins.attrNames benchmarkPlan.cases
        );
      };
      expected = {
        apiVersion = "nixspace/v1";
        interfaceVersion = 3;
        kind = "BenchmarkPlan";
        inherit system;
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
        hasNativeWarmCases = system == "x86_64-linux";
      };
    };
    testActionsAreCommitPinned = {
      expr = builtins.all pinnedAction actionLines;
      expected = true;
    };
    testCiCachePublicationBoundary = {
      expr = {
        readOnlyPullRequests = lib.hasInfix "skipPush: true" ci;
        mainOnlyPush = lib.hasInfix "if: github.event_name == 'push' && github.ref == 'refs/heads/main'" ci;
        explicitRoot = lib.hasInfix "\".#packages.\${{ matrix.system }}.public-cache-root\"" ci;
        pushesRoot = lib.hasInfix "cachix push cognipilot ./result-public-cache-root" ci;
        provesNoBuilderSubstitution = lib.hasInfix ''--max-jobs 0 --builders "" --option fallback false'' ci;
        noIncidentalWatch = !(lib.hasInfix "cachix watch" ci);
      };
      expected = {
        readOnlyPullRequests = true;
        mainOnlyPush = true;
        explicitRoot = true;
        pushesRoot = true;
        provesNoBuilderSubstitution = true;
        noIncidentalWatch = true;
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
