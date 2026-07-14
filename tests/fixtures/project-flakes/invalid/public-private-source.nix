{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects = {
    private = {
      repositoryId = "private";
      source.input = "private-source";
      preset = "cargo-v1";
    };

    public = {
      repositoryId = "public";
      source = {
        input = "public-source";
        visibility = "public";
        dependencies = [ "private" ];
      };
      preset = "cargo-v1";
    };
  };
}
