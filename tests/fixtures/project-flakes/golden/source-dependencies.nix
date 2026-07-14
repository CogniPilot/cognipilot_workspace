{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects = {
    schema = {
      repositoryId = "schema";
      source.input = "schema-source";
      preset = "cargo-v1";
      targets.schema = { };
    };

    middleware = {
      repositoryId = "middleware";
      source = {
        input = "middleware-source";
        dependencies = [ "schema" ];
      };
      preset = "cargo-v1";
    };

    application = {
      repositoryId = "application";
      source = {
        input = "application-source";
        dependencies = [ "middleware" ];
      };
      preset = "cargo-v1";
    };
  };
}
