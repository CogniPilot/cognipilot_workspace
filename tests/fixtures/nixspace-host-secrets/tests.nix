let
  pkgs = import <nixpkgs> { };
  lib = pkgs.lib;

  basePlan = {
    apiVersion = "nixspace/v1";
    kind = "Host";
    interfaceVersion = 4;
    nix = {
      minimumVersion = "2.18";
      settings = {
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

  evaluate =
    settings:
    lib.evalModules {
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
        ../../../nix/nixspace/host-module.nix
        {
          nixspace.host.plan = basePlan // {
            nix = basePlan.nix // {
              inherit settings;
            };
          };
        }
      ];
    };

  force = settings: builtins.toJSON (evaluate settings).config.flake.nixspaceHostPlan;
  accepted = builtins.tryEval (force basePlan.nix.settings);
  rejected =
    map
      (name: builtins.tryEval (force (basePlan.nix.settings // { ${name} = "must-not-enter-store"; })))
      [
        "access-tokens"
        "netrc-file"
        "secret-key-files"
        "registry-password"
      ];
  rejectedSubstituters =
    map
      (uri:
        builtins.tryEval (
          force {
            extra-substituters = [
              "https://cache.example.test"
              uri
            ];
          }
        )
      )
      [
        "https://token@cache.example.test"
        "https://cache.example.test?token=value"
        "https://cache.example.test#credential"
        "http://cache.example.test"
      ];
in
assert accepted.success;
assert builtins.all (result: !result.success) rejected;
assert builtins.all (result: !result.success) rejectedSubstituters;
{
  success = true;
  rejectedCount = builtins.length rejected + builtins.length rejectedSubstituters;
}
