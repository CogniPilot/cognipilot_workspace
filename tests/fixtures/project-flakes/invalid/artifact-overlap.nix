{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    targets.default.artifacts.outputs = {
      bundle = {
        kind = "directory";
        path = "bundle";
        contract = {
          name = "result-bundle";
          version = 1;
        };
      };
      image = {
        kind = "file";
        path = "bundle/image.bin";
        contract = {
          name = "result-image";
          version = 1;
        };
      };
    };
  };
}
