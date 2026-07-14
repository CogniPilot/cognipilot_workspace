{
  description = "Tiny immutable CogniPilot release product fixture";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
    release_provider = {
      url = "path:./provider";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    alpha_source = {
      url = "path:./sources/alpha";
      flake = false;
    };
    alpha_definition = {
      url = "path:./definitions/alpha";
      flake = false;
    };
    beta_source = {
      url = "path:./sources/beta";
      flake = false;
    };
    beta_definition = {
      url = "path:./definitions/beta";
      flake = false;
    };
  };

  outputs =
    inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        ./nix/cognipilot/flake-module.nix
        ./nix/cognipilot/product-flake-module.nix
        ./nix/cognipilot/resolution-module.nix
        ./nix/cognipilot/nixspace-module.nix
      ];

      systems = [ "x86_64-linux" ];

      cognipilot.product = {
        name = "release-fixture";
        defaultTarget = {
          packageId = "alpha";
          targetId = "default";
        };
      };

      # This fixture is a release consumer, not an editable workspace.  Its
      # concrete plan therefore selects only immutable provider outputs and
      # intentionally defines no devenv task graph.
      cognipilot.resolution = {
        workspaceRoot = "editable-checkouts";
        selectedScopes.alpha = "locked";
      };

      cognipilot.projects = {
        alpha = {
          packageId = "alpha";
          deployability = "deployable";
          repositoryId = "alpha-source";
          source = {
            input = "alpha_source";
            visibility = "public";
          };
          definition = {
            origin = "external";
            input = "alpha_definition";
          };
          preset = "cargo-v1";
          softwareVersion = {
            source = "literal";
            value = "1.2.3";
          };
          targets.default.release = {
            provider = "release_provider";
            package = "alpha-release";
          };
          targets.default.artifacts.outputs.runtime = {
            kind = "executable";
            path = "bin/alpha-runtime";
            contract = {
              name = "alpha-runtime-cli";
              version = 1;
            };
          };
          resources.config = {
            kind = "configuration";
            path = "share/alpha/config.json";
          };
          executables.runtime.from = "alpha:default:runtime";
          launches.release = {
            description = "Run the immutable alpha runtime.";
            requiredArtifacts = [ "alpha:default:runtime" ];
            requiredResources = [ "alpha:config" ];
            processes.runtime = {
              executable = "alpha:runtime";
              argv = [ { literal = "--from-release-launch"; } ];
              readiness.kind = "started";
            };
          };
        };

        beta = {
          packageId = "beta";
          deployability = "deployable";
          repositoryId = "beta-source";
          source = {
            input = "beta_source";
            visibility = "private";
          };
          definition = {
            origin = "external";
            input = "beta_definition";
          };
          preset = "cmake-v1";
          softwareVersion = {
            source = "literal";
            value = "4.5.6";
          };
          targets.firmware.release = {
            provider = "release_provider";
            package = "beta-firmware";
          };
        };

        observer = {
          packageId = "observer";
          deployability = "qualification";
          repositoryId = "observer-source";
          source = {
            input = "observer_source";
            visibility = "private";
          };
          preset = "npm-v1";
        };
      };
    };
}
