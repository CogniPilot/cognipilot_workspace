{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects = {
    codegen = {
      repositoryId = "codegen";
      source = {
        input = "codegen-source";
        visibility = "public";
      };
      preset = "cargo-v1";
      targets.schema = {
        variants = {
          dimensions.language = {
            values = [
              "c"
              "cpp"
            ];
            default = "c";
          };
          allowedCombinations = [
            { language = "c"; }
            { language = "cpp"; }
          ];
        };
        artifacts.outputs.headers = {
          kind = "directory";
          path = "generated/include";
          contract = {
            name = "synapse-api";
            version = 1;
          };
        };
      };
    };

    flight = {
      repositoryId = "flight";
      source = {
        input = "flight-source";
        visibility = "public";
      };
      preset = "west-v1";
      targets.firmware = {
        variants.dimensions.board = {
          values = [
            "native-sim"
            "mr-canhubk3"
          ];
          default = "native-sim";
        };
        artifacts = {
          inputs.synapse = {
            from = "codegen:schema:headers";
            contract = {
              name = "synapse-api";
              version = 1;
            };
          };
          outputs.image = {
            kind = "file";
            path = "zephyr/zephyr.bin";
            contract = {
              name = "firmware-image";
              version = 1;
            };
          };
        };
      };
    };
  };
}
