{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    softwareVersion = {
      source = "native";
      value = "1.2.3";
    };
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
  };
}
