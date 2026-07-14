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
      preset = "cargo-v1";
      customActions.docs = {
        kind = "other";
        argv = [ "build-docs" ];
      };
      targets.default.artifacts = {
        outputs.report = {
          kind = "file";
          path = "docs/report.json";
          contract = {
            name = "documentation-report";
            version = 1;
          };
        };
        inputs.api = {
          from = "producer:default:api";
          contract = {
            name = "generated-api";
            version = 1;
          };
        };
      };
    };
  };
}
