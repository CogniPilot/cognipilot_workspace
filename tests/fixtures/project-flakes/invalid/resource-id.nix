{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    resources."Invalid Resource" = {
      kind = "data";
      path = "data/example.json";
    };
  };
}
