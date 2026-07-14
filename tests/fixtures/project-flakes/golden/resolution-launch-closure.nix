{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects = {
    app = {
      preset = "cargo-v1";
      launches.bundle = {
        description = "Start the worker bundle.";
        includes.worker.launch = "worker:service";
      };
    };

    worker = {
      preset = "cargo-v1";
      targets.default.artifacts.outputs.worker-bin = {
        kind = "executable";
        path = "bin/worker";
        contract = {
          name = "worker-cli";
          version = 1;
        };
      };
      resources.config = {
        kind = "configuration";
        path = "config/worker.json";
      };
      executables.worker.from = "worker:default:worker-bin";
      launches.service = {
        description = "Start the worker service.";
        requiredArtifacts = [ "worker:default:worker-bin" ];
        requiredResources = [ "worker:config" ];
        processes.worker.executable = "worker:worker";
      };
    };
  };
}
