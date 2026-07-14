{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    deployability = "deployable";
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
  };
}
