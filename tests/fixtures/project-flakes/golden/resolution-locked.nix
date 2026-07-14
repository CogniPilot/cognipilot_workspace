{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.runtime = {
    deployability = "deployable";
    preset = "cargo-v1";

    targets.default = {
      release = {
        provider = "runtime_release";
        package = "runtime";
      };
      artifacts.outputs.cli = {
        kind = "executable";
        path = "bin/runtime";
        contract = {
          name = "runtime-cli";
          version = 1;
        };
      };
    };

    resources.config = {
      kind = "configuration";
      path = "share/runtime/config.json";
    };

    executables.runtime.from = "runtime:default:cli";
  };
}
