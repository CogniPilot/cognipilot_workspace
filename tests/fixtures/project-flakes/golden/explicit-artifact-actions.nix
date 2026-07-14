{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects = {
    producer = {
      repositoryId = "producer";
      source.input = "producer-source";
      preset = "cargo-v1";
      customActions.docs = {
        kind = "other";
        argv = [ "build-docs" ];
      };
      targets.default.artifacts.outputs = {
        api = {
          producedBy = "build";
          kind = "directory";
          path = "generated/api";
          contract = {
            name = "generated-api";
            version = 1;
          };
        };
        documentation = {
          producedBy = "docs";
          kind = "directory";
          path = "target/doc";
          contract = {
            name = "api-documentation";
            version = 1;
          };
        };
      };
    };

    consumer = {
      repositoryId = "consumer";
      source.input = "consumer-source";
      preset = "cmake-v1";
      customActions.docs = {
        kind = "other";
        argv = [ "build-consumer-docs" ];
      };
      targets.default.artifacts.inputs.api = {
        consumedBy = [ "build" ];
        from = "producer:default:api";
        contract = {
          name = "generated-api";
          version = 1;
        };
      };
    };
  };
}
