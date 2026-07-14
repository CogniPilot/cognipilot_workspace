{
  imports = [
    ../../../nix/cognipilot/flake-module.nix
    ../../../nix/cognipilot/compliance-flake-module.nix
  ];

  cognipilot = {
    compliancePolicy.approvedBespokeActions = [
      "not-a-coordinate"
      "flight-control:default:missing"
      "flight-control:default:missing"
    ];

    projects.flight-control = {
      lifecycle = "stable";
      deployability = "qualification";
      owner = "CogniPilot Foundation";
      license.spdx = "Apache-2.0";
      repositoryId = "flight-control";
      source.input = "flight-control-source";
      preset = "cargo-v1";
    };
  };
}
