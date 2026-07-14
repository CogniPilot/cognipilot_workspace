{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.interfaceVersion = 2;
  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
  };
}
