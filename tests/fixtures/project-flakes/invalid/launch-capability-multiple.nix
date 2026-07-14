{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    launches = {
      first = {
        description = "First router provider.";
        capabilities.provides = [ "router" ];
      };
      second = {
        description = "Second router provider.";
        capabilities.provides = [ "router" ];
      };
      consumer = {
        description = "Select two conflicting router providers.";
        includes = {
          first.launch = "example:first";
          second.launch = "example:second";
        };
        capabilities.requires = [ "router" ];
      };
    };
  };
}
