{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    deployability = "qualification";
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    targets.default.release = {
      provider = "release-provider";
      package = "example";
    };
  };
}
