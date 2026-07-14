{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects."Invalid Project" = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
  };
}
