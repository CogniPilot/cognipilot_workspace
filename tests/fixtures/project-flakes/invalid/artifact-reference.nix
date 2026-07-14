{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    targets.default.artifacts.inputs.missing = {
      from = "producer:default:missing";
      contract = {
        name = "missing-data";
        version = 1;
      };
    };
  };
}
