{ pkgs }:

let
  inherit (pkgs) lib;
  definitionFlake = (import ./definition/flake.nix).outputs { };
  definitionModule = definitionFlake.flakeModules.default;
  lock = builtins.fromJSON (builtins.readFile ./flake.lock);

  rootSupport = { lib, ... }: {
    options = {
      flake = lib.mkOption {
        type = lib.types.attrs;
        default = { };
      };
      perSystem = lib.mkOption { type = lib.types.deferredModule; };
    };
  };
  systemSupport = { lib, ... }: {
    options = {
      apps = lib.mkOption {
        type = lib.types.lazyAttrsOf lib.types.attrs;
        default = { };
      };
      checks = lib.mkOption {
        type = lib.types.lazyAttrsOf lib.types.package;
        default = { };
      };
      devShells = lib.mkOption {
        type = lib.types.lazyAttrsOf lib.types.package;
        default = { };
      };
      formatter = lib.mkOption {
        type = lib.types.nullOr lib.types.package;
        default = null;
      };
      packages = lib.mkOption {
        type = lib.types.lazyAttrsOf lib.types.package;
        default = { };
      };
    };
  };

  root = lib.evalModules {
    modules = [
      rootSupport
      ../../../nix/cognipilot/flake-module.nix
      ../../../nix/cognipilot/project-flake-module.nix
      definitionModule
    ];
  };
  direct = lib.evalModules {
    modules = [
      ../../../nix/cognipilot/flake-module.nix
      definitionModule
    ];
  };
  systemConfig =
    (lib.evalModules {
      specialArgs = { inherit pkgs; };
      modules = [
        systemSupport
        root.config.perSystem
      ];
    }).config;

  providerIndex = root.config.flake.cognipilotIndex;
  genericIndex = root.config.flake.nixspaceIndex;
  tests = {
    externalDefinitionExportsOneDefaultModule = {
      expr = {
        projects = builtins.attrNames definitionModule.cognipilot.projects;
        definition = definitionModule.cognipilot.projects.fake-app.definition;
      };
      expected = {
        projects = [ "fake-app" ];
        definition = {
          origin = "external";
          input = "fake_definition";
        };
      };
    };
    standaloneAndDirectAuthoritiesNormalizeIdentically = {
      expr = {
        equal = providerIndex == direct.config.cognipilot.validatedIndex;
        inherit (providerIndex) apiVersion interfaceVersion kind;
        source = providerIndex.projects.fake-app.source.input;
        definition = providerIndex.projects.fake-app.definition;
      };
      expected = {
        equal = true;
        apiVersion = "nixspace/v1";
        interfaceVersion = 1;
        kind = "Workspace";
        source = "fake_source";
        definition = {
          origin = "external";
          input = "fake_definition";
        };
      };
    };
    genericStandaloneProjectionIsExact = {
      expr = {
        inherit (genericIndex) apiVersion interfaceVersion kind;
        packageIds = map (package: package.id) genericIndex.catalog.packages;
        providerExtension = builtins.attrNames (builtins.head genericIndex.catalog.packages).extensions;
        packageDocument = systemConfig.packages.nixspace-index.passthru.document;
      };
      expected = {
        apiVersion = "nixspace/v1";
        interfaceVersion = 2;
        kind = "Workspace";
        packageIds = [ "fake-app" ];
        providerExtension = [ "org.cognipilot/package-v1" ];
        packageDocument = genericIndex;
      };
    };
    conventionalOutputSurfaceHasNoArbitraryDefault = {
      expr = {
        packages = builtins.attrNames systemConfig.packages;
        checks = builtins.attrNames systemConfig.checks;
        devShells = builtins.attrNames systemConfig.devShells;
        apps = builtins.attrNames systemConfig.apps;
        showIndexType = systemConfig.apps.show-index.type;
        formatter = systemConfig.formatter.type;
        hasDefault = systemConfig.packages ? default;
      };
      expected = {
        packages = [
          "nixspace"
          "nixspace-completions"
          "nixspace-index"
        ];
        checks = [
          "nixspace-interface"
          "nixspace-standalone"
        ];
        devShells = [ "default" ];
        apps = [
          "nixspace"
          "show-index"
        ];
        showIndexType = "app";
        formatter = "derivation";
        hasDefault = false;
      };
    };
    sourceAndLockRemainConventional = {
      expr = {
        sourceHasFlake = builtins.pathExists ./source/flake.nix;
        lockVersion = lock.version;
        sourceFlake = lock.nodes.fake_source.flake;
        sourceType = lock.nodes.fake_source.locked.type;
        definitionType = lock.nodes.fake_definition.locked.type;
      };
      expected = {
        sourceHasFlake = false;
        lockVersion = 7;
        sourceFlake = false;
        sourceType = "path";
        definitionType = "path";
      };
    };
  };
  failures = lib.runTests tests;

  runtime =
    pkgs.runCommand "project-output-contract"
      {
        interfaceCheck = systemConfig.checks.nixspace-interface;
        nativeBuildInputs = [ pkgs.jq ];
      }
      ''
        ${systemConfig.apps.show-index.program} \
          | jq -e '
              .apiVersion == "nixspace/v1" and
              .kind == "Workspace" and
              .interfaceVersion == 2 and
              [.catalog.packages[].id] == ["fake-app"]
            ' >/dev/null
        test -e "$interfaceCheck"
        touch "$out"
      '';
in
if failures == [ ] then
  {
    report = {
      suite = "project-output";
      assertionCount = builtins.length (builtins.attrNames tests);
      packageId = providerIndex.projects.fake-app.packageId;
    };
    checks.project-output-runtime = runtime;
  }
else
  throw "project output contract failures: ${builtins.toJSON failures}"
