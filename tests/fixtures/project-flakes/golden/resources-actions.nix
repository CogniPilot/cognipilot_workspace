{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.app = {
    repositoryId = "app";
    source.input = "app-source";
    preset = "cargo-v1";

    targets.default = {
      actionRequirements.build = {
        cpu = 2;
        memoryMiB = 1024;
        exclusiveLocks = [
          "cargo-target"
          "usb-device"
        ];
      };
      artifacts.outputs.cli = {
        kind = "executable";
        path = "bin/app";
        contract = {
          name = "app-cli";
          version = 1;
        };
      };
    };

    resources.default-config = {
      kind = "configuration";
      path = "config/default.json";
    };
    executables.app = {
      from = "app:default:cli";
      argv = [
        "--config"
        "config/default.json"
      ];
    };
  };
}
