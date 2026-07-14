{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    owner = "   ";
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
  };
}
