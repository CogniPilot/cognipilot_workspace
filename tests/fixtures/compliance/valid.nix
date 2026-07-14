{
  imports = [
    ../../../nix/cognipilot/flake-module.nix
    ../../../nix/cognipilot/compliance-flake-module.nix
  ];

  cognipilot.projects.flight-control = {
    lifecycle = "stable";
    deployability = "deployable";
    owner = "CogniPilot Foundation";
    license.spdx = "Apache-2.0";
    repositoryId = "flight-control";
    source.input = "flight-control-source";
    preset = "cargo-v1";
    targets.default.release = {
      provider = "flight-control-release";
      package = "flight-control";
    };
  };
}
