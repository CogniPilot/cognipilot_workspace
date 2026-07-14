{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects = {
    producer = {
      repositoryId = "producer";
      source.input = "producer-source";
      preset = "cargo-v1";
      targets.default.artifacts.outputs.api = {
        kind = "directory";
        path = "target/api";
        contract = {
          name = "api";
          version = 1;
        };
      };
    };
    consumer = {
      repositoryId = "consumer";
      source.input = "consumer-source";
      preset = "cmake-v1";
      customActions.invalid = {
        kind = "other";
        argv = [
          "consume"
          { artifactInput = "api"; }
        ];
      };
      targets.default.artifacts.inputs.api = {
        from = "producer:default:api";
        consumedBy = [ "build" ];
        contract = {
          name = "api";
          version = 1;
        };
      };
    };
  };
}
