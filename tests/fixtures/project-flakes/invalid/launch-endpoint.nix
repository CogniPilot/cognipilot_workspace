{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    targets.default.artifacts.outputs.worker = {
      kind = "executable";
      path = "bin/worker";
      contract = {
        name = "worker-cli";
        version = 1;
      };
    };
    executables.worker.from = "example:default:worker";
    launches.demo = {
      description = "Invalid endpoint parameter and readiness.";
      parameters.port = {
        type = "string";
        default = "7447";
      };
      processes.worker = {
        executable = "example:worker";
        endpoints.api = {
          protocol = "http";
          portParameter = "port";
        };
        readiness = {
          kind = "endpoint";
          endpoint = "missing";
        };
      };
    };
  };
}
