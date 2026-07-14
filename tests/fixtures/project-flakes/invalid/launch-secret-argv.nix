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
    executables.cli.from = "example:default:cli";
    launches.demo = {
      description = "Invalid secret argv reference.";
      parameters.token = {
        type = "secret";
        required = true;
      };
      processes.demo = {
        executable = "example:cli";
        argv = [ { parameter = "token"; } ];
      };
    };
  };
}
