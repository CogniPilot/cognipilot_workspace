{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "zephyr-native-sim-v1";
    targets.default.variants.dimensions.board = {
      values = [ "native_sim/native/64" ];
      default = "native_sim/native/64";
    };
  };
}
