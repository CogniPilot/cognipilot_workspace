{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    softwareVersion = {
      source = "file";
      file = "../VERSION";
    };
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
  };
}
