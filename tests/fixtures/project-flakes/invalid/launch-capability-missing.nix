{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    launches.consumer = {
      description = "Require an explicitly selected router provider.";
      capabilities.requires = [ "router" ];
    };
  };
}
