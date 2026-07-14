{
  cognipilot.projects.csyn = {
    lifecycle = "stable";
    deployability = "qualification";
    owner = "CogniPilot";
    license.spdx = "Apache-2.0";
    source.visibility = "public";
    definition.origin = "external";
    preset = "cargo-rust-manifest-v1";

    targets.default.artifacts = {
      inputs.synapse-rust = {
        from = "synapse_fbs:default:rust";
        consumedBy = [
          "build"
          "test"
        ];
        environment = "CSYN_SYNAPSE_RUST_ROOT";
        contract = {
          name = "synapse-rust-bindings";
          version = 1;
        };
      };
      outputs.cli = {
        producedBy = "build";
        kind = "executable";
        path = "rust/target/debug/csyn";
        contract = {
          name = "csyn-cli";
          version = 1;
        };
      };
    };
    targets.default.actionRequirements.qualification.exclusiveLocks = [ "west-workspace" ];
    resources = {
      west-manifest = {
        kind = "configuration";
        path = "west.yml";
      };
      zephyr-module = {
        kind = "configuration";
        path = "zephyr/module.yml";
      };
    };
    executables.cli.from = "csyn:default:cli";
  };
}
