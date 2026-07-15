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
        ./project-module.nix
      ];

      systems = [ "x86_64-linux" ];
    };
}
