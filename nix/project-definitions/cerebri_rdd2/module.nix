{
  cognipilot.projects.cerebri_rdd2 = {
    lifecycle = "stable";
    deployability = "qualification";
    owner = "CogniPilot";
    license.spdx = "Apache-2.0";
    source.visibility = "public";
    source.dependencies = [
      "cerebri_modules"
      "csyn"
      "rumoca"
      "synapse_fbs"
    ];
    definition.origin = "external";
    preset = "rdd2-v1";

    targets.default = {
      artifacts = {
        inputs = {
          synapse-c = {
            from = "synapse_fbs:default:c";
            consumedBy = [ "build" ];
            contract = {
              name = "synapse-c-bindings";
              version = 1;
            };
          };
          rumoca-compiler = {
            from = "rumoca:default:compiler";
            consumedBy = [ "build" ];
            contract = {
              name = "rumoca-compiler";
              version = 1;
            };
          };
        };
        outputs.simulator = {
          producedBy = "build";
          kind = "executable";
          path = "build-native_sim/zephyr/zephyr.exe";
          contract = {
            name = "cerebri-rdd2-native-sim";
            version = 1;
          };
        };
      };
      actionRequirements = {
        prepare.exclusiveLocks = [ "west-workspace" ];
        build.exclusiveLocks = [ "west-workspace" ];
      };
    };

    resources.west-manifest = {
      kind = "configuration";
      path = "west.yml";
    };
    executables.simulator.from = "cerebri_rdd2:default:simulator";
  };
}
