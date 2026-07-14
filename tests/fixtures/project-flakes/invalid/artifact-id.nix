{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    targets.default.artifacts.outputs."Invalid Artifact" = {
      kind = "file";
      path = "result.bin";
      contract = {
        name = "result-data";
        version = 1;
      };
    };
  };
}
