{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cmake-v1";
    targets = {
      first.artifacts = {
        outputs.result = {
          kind = "file";
          path = "first/result";
          contract = {
            name = "cycle-data";
            version = 1;
          };
        };
        inputs.previous = {
          from = "example:second:result";
          contract = {
            name = "cycle-data";
            version = 1;
          };
        };
      };
      second.artifacts = {
        outputs.result = {
          kind = "file";
          path = "second/result";
          contract = {
            name = "cycle-data";
            version = 1;
          };
        };
        inputs.previous = {
          from = "example:first:result";
          contract = {
            name = "cycle-data";
            version = 1;
          };
        };
      };
    };
  };
}
