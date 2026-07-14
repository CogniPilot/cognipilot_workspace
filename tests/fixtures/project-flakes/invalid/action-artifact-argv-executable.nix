{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    customActions.invalid = {
      kind = "other";
      argv = [ { artifactInput = "api"; } ];
    };
    targets.default.artifacts.inputs.api = {
      from = "example:default:api";
      consumedBy = [ "invalid" ];
      contract = {
        name = "api";
        version = 1;
      };
    };
    targets.default.artifacts.outputs.api = {
      producedBy = "build";
      kind = "directory";
      path = "target/api";
      contract = {
        name = "api";
        version = 1;
      };
    };
  };
}
