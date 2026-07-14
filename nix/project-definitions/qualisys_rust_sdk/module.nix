{
  cognipilot.projects.qualisys_rust_sdk = {
    lifecycle = "stable";
    softwareVersion = {
      source = "literal";
      value = "0.1.0";
    };
    deployability = "qualification";
    owner = "CogniPilot";
    license.spdx = "MIT OR Apache-2.0";
    source.visibility = "public";
    definition.origin = "external";
    preset = "nix-flake-check-v1";

    targets.default.artifacts.outputs = {
      realtime-client = {
        producedBy = "build";
        kind = "executable";
        path = "result-default/bin/qualisys-rt";
        contract = {
          name = "qualisys-realtime-client";
          version = 1;
        };
      };
      simulator = {
        producedBy = "build";
        kind = "executable";
        path = "result-default/bin/qualisys-sim";
        contract = {
          name = "qualisys-simulator";
          version = 1;
        };
      };
    };
    executables = {
      realtime-client.from = "qualisys_rust_sdk:default:realtime-client";
      simulator.from = "qualisys_rust_sdk:default:simulator";
    };
  };
}
