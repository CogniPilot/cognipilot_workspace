{
  cognipilot.projects.cerebri_cubs2 = {
    lifecycle = "stable";
    deployability = "qualification";
    owner = "CogniPilot";
    license.spdx = "Apache-2.0";

    source.visibility = "public";
    definition.origin = "external";
    preset = "zephyr-native-sim-v1";

    targets.default = {
      variants.dimensions.board = {
        # Zephyr board targets are native tool values, not Nix attribute IDs.
        # Keep the exact board string consumed by the project flake so the
        # declaration does not invent an alias that no action can execute.
        values = [ "native_sim/native/64" ];
        default = "native_sim/native/64";
      };
      artifacts = {
        inputs.synapse-c = {
          from = "synapse_fbs:default:c";
          consumedBy = [
            "build"
            "test"
          ];
          environment = "CUBS2_SYNAPSE_C_ROOT";
          contract = {
            name = "synapse-c-bindings";
            version = 1;
          };
        };
        inputs.rumoca-python = {
          from = "rumoca:default:python";
          consumedBy = [
            "build"
            "test"
          ];
          environment = "CUBS2_RUMOCA_PYTHON";
          contract = {
            name = "rumoca-python-environment";
            version = 1;
          };
        };
        outputs.simulator = {
          kind = "executable";
          path = "build-native_sim_native_64_sil/zephyr/zephyr.exe";
          contract = {
            name = "cerebri-native-sim";
            version = 1;
          };
        };
      };
      actionRequirements.build = {
        cpu = 2;
        memoryMiB = 4096;
        exclusiveLocks = [ "west-workspace" ];
      };
    };

    resources.west-manifest = {
      kind = "configuration";
      path = "west.yml";
    };
    executables.simulator.from = "cerebri_cubs2:default:simulator";
  };
}
