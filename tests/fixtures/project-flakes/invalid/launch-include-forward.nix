{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    launches = {
      child = {
        description = "Child launch.";
        parameters.port = {
          type = "port";
          required = true;
        };
      };
      parent = {
        description = "Parent launch missing required forwarding.";
        parameters.host = {
          type = "host";
          default = "127.0.0.1";
        };
        includes.child.launch = "example:child";
      };
    };
  };
}
