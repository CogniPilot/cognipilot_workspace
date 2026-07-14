{
  description = "Standalone CogniPilot project-output proof";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
    fake_source = {
      url = "path:./source";
      flake = false;
    };
    fake_definition.url = "path:./definition";
  };

  outputs =
    inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        ./nix/cognipilot/flake-module.nix
        ./nix/cognipilot/project-flake-module.nix
        inputs.fake_definition.flakeModules.default
      ];

      systems = [ "x86_64-linux" ];

      flake.flakeModules.default = inputs.fake_definition.flakeModules.default;
    };
}
