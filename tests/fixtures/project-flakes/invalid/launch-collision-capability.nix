{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    launches = {
      child.description = "Child launch.";
      parent = {
        description = "Namespace and capability collisions.";
        processes.child.executable = "missing:worker";
        includes.child.launch = "example:child";
        capabilities = {
          provides = [ "router" ];
          requires = [ "router" ];
        };
      };
    };
  };
}
