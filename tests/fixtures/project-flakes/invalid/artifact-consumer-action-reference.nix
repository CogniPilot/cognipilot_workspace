{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects = {
    producer = {
      repositoryId = "producer";
      source.input = "producer-source";
      preset = "cargo-v1";
      targets.default.artifacts.outputs.result = {
        kind = "file";
        path = "dist/result";
        contract = {
          name = "result-data";
          version = 1;
        };
      };
    };
    consumer = {
      repositoryId = "consumer";
      source.input = "consumer-source";
      preset = "cmake-v1";
      targets.default.artifacts.inputs.result = {
        consumedBy = [ "publish" ];
        from = "producer:default:result";
        contract = {
          name = "result-data";
          version = 1;
        };
      };
    };
  };
}
