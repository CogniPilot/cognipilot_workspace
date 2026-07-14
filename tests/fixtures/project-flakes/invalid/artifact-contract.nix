{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects = {
    producer = {
      repositoryId = "producer";
      source.input = "producer-source";
      preset = "cargo-v1";
      targets.default.artifacts.outputs.api = {
        kind = "directory";
        path = "generated/api";
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
      targets.default.artifacts.inputs.api = {
        from = "producer:default:api";
        contract = {
          name = "generated-api";
          version = 2;
        };
      };
    };
  };
}
