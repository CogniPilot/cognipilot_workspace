{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    targets.default.artifacts.outputs.result = {
      kind = "file";
      path = "result.txt";
      contract = {
        name = "result-data";
        version = 1;
      };
    };
    executables.cli.from = "example:default:result";
  };
}
