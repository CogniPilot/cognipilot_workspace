{ pkgs }:

let
  lib = pkgs.lib;

  root = lib.evalModules {
    modules = [
      {
        options = {
          flake = lib.mkOption {
            type = lib.types.attrs;
            default = { };
          };
          perSystem = lib.mkOption {
            type = lib.types.deferredModule;
          };
        };
      }
      ../../../nix/nixspace/index-module.nix
      ../../../nix/nixspace/host-module.nix
      {
        nixspace.index = {
          interfaceVersion = 2;
          catalog = {
            packages = [ ];
            targets = [ ];
            artifacts = [ ];
            resources = [ ];
            executables = [ ];
            launches = [ ];
          };
          graph = {
            schemaVersion = 1;
            all = {
              nodes = [ ];
              edges = [ ];
            };
            packages = { };
            reverse = { };
          };
          launchPlans = { };
        };
        nixspace.host.plan = {
          apiVersion = "nixspace/v1";
          kind = "Host";
          interfaceVersion = 4;
          nix = {
            minimumVersion = "2.18";
            settings = {
              accept-flake-config = true;
              experimental-features = [
                "nix-command"
                "flakes"
              ];
              extra-substituters = [ "https://cache.example.test" ];
            };
          };
          readiness = {
            storage = {
              path = ".";
              minimumAvailableBytes = 0;
            };
            daemon = {
              when = "never";
              probeArgv = [ ];
            };
            requiredDocuments = [ ];
            sourceSelection = "default";
            launch = {
              allowActiveSessions = true;
              requireManagerSocket = true;
              requireAvailableDeclaredPorts = true;
            };
            cache = {
              coverageMode = "union";
              storeDirectory = builtins.storeDir;
              roots = [
                {
                  name = "public-root";
                  path = "result-public-root";
                }
              ];
              stores = [
                {
                  name = "public";
                  uri = "https://cache.example.test";
                }
              ];
            };
          };
        };
      }
    ];
  };

  system = lib.evalModules {
    specialArgs = { inherit pkgs; };
    modules = [
      {
        options = {
          apps = lib.mkOption {
            type = lib.types.lazyAttrsOf lib.types.attrs;
            default = { };
          };
          checks = lib.mkOption {
            type = lib.types.lazyAttrsOf lib.types.package;
            default = { };
          };
          packages = lib.mkOption {
            type = lib.types.lazyAttrsOf lib.types.package;
            default = { };
          };
        };
      }
      root.config.perSystem
    ];
  };

  document = root.config.flake.nixspaceIndex;
in
assert document.apiVersion == "nixspace/v1";
assert document.kind == "Workspace";
assert document.interfaceVersion == 2;
assert document.catalog.packages == [ ];
assert
  builtins.attrNames system.config.packages == [
    "nixspace"
    "nixspace-completions"
    "nixspace-host"
    "nixspace-host-plan"
    "nixspace-index"
  ];
assert
  builtins.attrNames system.config.apps == [
    "nixspace"
    "nixspace-host"
  ];
assert
  builtins.attrNames system.config.checks == [
    "nixspace-interface"
    "nixspace-standalone"
  ];
assert system.config.packages.nixspace.pname == "nixspace";
assert
  system.config.apps.nixspace-host.program
  == "${system.config.packages.nixspace-host}/bin/nixspace-host";
assert system.config.apps.nixspace.type == "app";
assert root.config.flake.nixspaceHostPlan.kind == "Host";
assert root.config.flake.nixspaceHostPlan.interfaceVersion == 4;
assert root.config.flake.nixspaceHostPlan.readiness.cache.coverageMode == "union";
assert root.config.flake.nixspaceHostPlan.readiness.cache.storeDirectory == builtins.storeDir;
assert
  root.config.flake.nixspaceHostPlan.readiness.cache.stores == [
    {
      name = "public";
      uri = "https://cache.example.test";
    }
  ];
{
  success = true;
  package = system.config.packages.nixspace.pname;
  interface = document.apiVersion;
}
