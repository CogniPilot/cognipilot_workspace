{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "west-v1";
    targets.firmware.variants.dimensions.board = {
      values = [
        "native-sim"
        "mr-canhubk3"
      ];
      default = "unknown";
    };
  };
}
