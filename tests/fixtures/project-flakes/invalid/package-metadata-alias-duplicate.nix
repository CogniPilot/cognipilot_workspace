{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    aliases = [
      "demo"
      "demo"
    ];
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
  };
}
