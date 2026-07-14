{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    launches.demo = {
      description = "Invalid automatic non-port parameter.";
      parameters.host = {
        type = "host";
        default = "127.0.0.1";
        allocation = "automatic";
      };
    };
  };
}
