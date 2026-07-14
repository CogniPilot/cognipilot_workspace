{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    resources.config = {
      kind = "configuration";
      path = "../config.json";
    };
  };
}
