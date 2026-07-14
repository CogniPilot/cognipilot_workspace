{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    launches.demo = {
      description = "Invalid path and secret policy.";
      parameters = {
        config = {
          type = "path";
          default = "../config";
        };
        token = {
          type = "secret";
          default = "plaintext";
        };
      };
    };
  };
}
