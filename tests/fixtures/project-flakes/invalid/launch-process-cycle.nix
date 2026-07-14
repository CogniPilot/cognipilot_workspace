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
      description = "Cyclic processes.";
      processes = {
        first = {
          executable = "example:worker";
          dependencies.second = "ready";
        };
        second = {
          executable = "example:worker";
          dependencies.first = "ready";
        };
      };
    };
  };
}
