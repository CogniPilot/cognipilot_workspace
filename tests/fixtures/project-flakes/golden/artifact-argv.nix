{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects = {
    producer = {
      repositoryId = "producer";
      source.input = "producer-source";
      preset = "cargo-v1";
      targets.default.artifacts.outputs.api = {
        kind = "directory";
        path = "target/generated/api";
        contract = {
          name = "generated-api";
          version = 1;
        };
      };
    };

    consumer = {
      repositoryId = "consumer";
      source.input = "consumer-source";
      preset = "cmake-v1";
      customActions.bind = {
        kind = "generate";
        argv = [
          "bind-api"
          {
            artifactInput = "api";
            prefix = "--api=";
            suffix = "/schema";
          }
        ];
      };
      targets.default.artifacts.inputs.api = {
        from = "producer:default:api";
        consumedBy = [ "bind" ];
        contract = {
          name = "generated-api";
          version = 1;
        };
      };
    };
  };
}
