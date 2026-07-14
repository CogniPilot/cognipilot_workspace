{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.flight-integration = {
    packageId = "flight-control";
    aliases = [
      "autopilot"
      "fc"
    ];
    lifecycle = "stable";
    softwareVersion = {
      source = "file";
      file = "VERSION";
    };
    deployability = "deployable";
    owner = "CogniPilot Foundation";
    license.spdx = "(Apache-2.0 OR MIT) AND BSD-3-Clause";

    repositoryId = "flight-control";
    source.input = "flight-source";
    preset = "west-v1";
    targets.default.release = {
      provider = "flight-release";
      package = "flight-control";
    };
  };
}
