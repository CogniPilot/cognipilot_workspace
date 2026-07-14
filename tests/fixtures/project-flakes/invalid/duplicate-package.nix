{
  imports = [
    ../../../../nix/cognipilot/flake-module.nix
    {
      cognipilot.projects.example = {
        repositoryId = "example";
        source.input = "first-source";
        preset = "cargo-v1";
      };
    }
    {
      cognipilot.projects.example = {
        repositoryId = "example";
        source.input = "second-source";
        preset = "cargo-v1";
      };
    }
  ];
}
