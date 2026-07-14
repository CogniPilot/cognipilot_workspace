{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.firmware = {
    repositoryId = "firmware";
    source.input = "firmware-source";
    preset = "west-v1";

    targets = {
      native-sim = {
        variants.dimensions.board = {
          values = [ "native-sim" ];
          default = "native-sim";
        };
        artifacts.outputs.image = {
          kind = "file";
          path = "build/native-sim/zephyr/zephyr.bin";
          contract = {
            name = "firmware-image";
            version = 1;
          };
        };
      };
      native-sim-64 = {
        variants.dimensions.board = {
          values = [ "native-sim-64" ];
          default = "native-sim-64";
        };
        artifacts.outputs.image = {
          kind = "file";
          path = "build/native-sim-64/zephyr/zephyr.bin";
          contract = {
            name = "firmware-image";
            version = 1;
          };
        };
      };
    };
  };
}
