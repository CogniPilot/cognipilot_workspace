{
  cognipilot.projects.synapse_fbs = {
    lifecycle = "stable";
    softwareVersion = {
      source = "literal";
      value = "0.8.0";
    };
    deployability = "qualification";
    owner = "CogniPilot";
    license.spdx = "Apache-2.0";

    source.visibility = "public";
    definition.origin = "external";
    preset = "cargo-xtask-v1";

    targets.default = {
      actionRequirements.build.exclusiveLocks = [ "synapse_fbs-packages" ];
      artifacts.outputs = {
        c = {
          kind = "directory";
          path = "target/xtask/artifacts-work/synapse_fbs-c";
          contract = {
            name = "synapse-c-bindings";
            version = 1;
          };
        };
        rust = {
          kind = "directory";
          path = "target/xtask/packages/rust";
          contract = {
            name = "synapse-rust-bindings";
            version = 1;
          };
        };
        python = {
          kind = "directory";
          path = "target/xtask/packages/python";
          contract = {
            name = "synapse-python-bindings";
            version = 1;
          };
        };
        javascript = {
          kind = "directory";
          path = "target/xtask/packages/js";
          contract = {
            name = "synapse-javascript-bindings";
            version = 1;
          };
        };
      };
    };
  };
}
