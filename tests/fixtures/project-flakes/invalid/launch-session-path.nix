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
      description = "Unsafe session path.";
      sessionEnvironment.STATE_FILE = {
        path = "../escape.json";
        create = "parent";
      };
      processes.worker.executable = "example:worker";
    };
  };
}
