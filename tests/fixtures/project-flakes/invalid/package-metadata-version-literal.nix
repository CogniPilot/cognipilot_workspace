{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    softwareVersion.source = "literal";
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
  };
}
