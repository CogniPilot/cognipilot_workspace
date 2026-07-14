{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    targets.default.artifacts.outputs.cli = {
      kind = "executable";
      path = "bin/example";
      contract = {
        name = "example-cli";
        version = 1;
      };
    };
    resources.cli = {
      kind = "configuration";
      path = "config/cli.json";
    };
    executables.cli.from = "example:default:cli";
  };
}
