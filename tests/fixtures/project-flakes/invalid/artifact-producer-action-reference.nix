{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    targets.default.artifacts.outputs.result = {
      producedBy = "publish";
      kind = "file";
      path = "dist/result";
      contract = {
        name = "result-data";
        version = 1;
      };
    };
  };
}
