{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    customActions = {
      package = {
        kind = "other";
        argv = [ "package-project" ];
        dependsOn = [ "publish" ];
      };
      publish = {
        kind = "other";
        argv = [ "publish-project" ];
        dependsOn = [ "package" ];
      };
    };
  };
}
