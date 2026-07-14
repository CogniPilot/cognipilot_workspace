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
      customActions.package = {
        kind = "other";
        argv = [ "package" ];
        environment.RESULT_PATH = "static";
      };
      targets.default.artifacts.inputs.result = {
        from = "producer:default:result";
        consumedBy = [ "package" ];
        environment = "RESULT_PATH";
        contract = {
          name = "result-data";
          version = 1;
        };
      };
    };
  };
}
