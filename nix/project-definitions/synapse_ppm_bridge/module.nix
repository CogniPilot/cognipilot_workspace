{
  cognipilot.projects.synapse_ppm_bridge = {
    lifecycle = "stable";
    softwareVersion = {
      source = "literal";
      value = "0.1.0";
    };
    deployability = "deployable";
    owner = "CogniPilot";
    license.spdx = "MIT OR Apache-2.0";
    source.visibility = "public";
    definition.origin = "external";
    preset = "cargo-locked-v1";

    targets.default = {
      release = {
        provider = "synapse_ppm_bridge_definition";
        package = "default";
      };
      artifacts.outputs = {
        bridge = {
          producedBy = "build";
          kind = "executable";
          path = "target/debug/synapse-ppm-bridge";
          contract = {
            name = "synapse-ppm-bridge-cli";
            version = 1;
          };
        };
      };
    };
    executables = {
      bridge.from = "synapse_ppm_bridge:default:bridge";
    };
  };
}
