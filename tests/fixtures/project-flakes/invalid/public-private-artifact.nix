{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects = {
    private = {
      repositoryId = "private";
      source.input = "private-source";
      preset = "cargo-v1";
      targets.default.artifacts.outputs.library = {
        kind = "directory";
        path = "target/library";
        contract = {
          name = "private-library";
          version = 1;
        };
      };
    };

    public = {
      repositoryId = "public";
      source = {
        input = "public-source";
        visibility = "public";
      };
      preset = "cargo-v1";
      targets.default.artifacts.inputs.library = {
        from = "private:default:library";
        contract = {
          name = "private-library";
          version = 1;
        };
      };
    };
  };
}
