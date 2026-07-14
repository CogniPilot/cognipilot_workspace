{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    aliases = [ "Invalid Alias" ];
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
  };
}
