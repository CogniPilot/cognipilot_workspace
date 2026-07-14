{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source = {
      input = "example-source";
      dependencies = [ "missing" ];
    };
    preset = "cargo-v1";
  };
}
