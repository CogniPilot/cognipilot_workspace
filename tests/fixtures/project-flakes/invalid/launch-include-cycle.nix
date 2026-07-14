{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    launches = {
      first = {
        description = "First launch.";
        includes.second.launch = "example:second";
      };
      second = {
        description = "Second launch.";
        includes.first.launch = "example:first";
      };
    };
  };
}
