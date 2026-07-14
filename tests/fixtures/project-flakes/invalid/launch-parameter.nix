{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    launches.demo = {
      description = "Invalid parameter default.";
      parameters.port = {
        type = "port";
        default = "not-a-port";
      };
    };
  };
}
