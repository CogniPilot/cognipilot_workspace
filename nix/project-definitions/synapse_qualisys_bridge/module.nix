{
  cognipilot.projects.synapse_qualisys_bridge = {
    lifecycle = "stable";
    softwareVersion = {
      source = "literal";
      value = "0.5.0";
    };
    deployability = "qualification";
    owner = "CogniPilot";
    license.spdx = "MIT OR Apache-2.0";
    source.visibility = "public";
    definition.origin = "external";
    preset = "synapse-qualisys-v1";

    targets.default.artifacts = {
      inputs.synapse-rust = {
        from = "synapse_fbs:default:rust";
        consumedBy = [
          "build"
          "test"
        ];
        environment = "SYNAPSE_RUST_ROOT";
        contract = {
          name = "synapse-rust-bindings";
          version = 1;
        };
      };
      inputs.qualisys-simulator = {
        from = "qualisys_rust_sdk:default:simulator";
        consumedBy = [ "qualification" ];
        environment = "QUALISYS_SIM_BIN";
        contract = {
          name = "qualisys-simulator";
          version = 1;
        };
      };
      outputs.bridge = {
        producedBy = "build";
        kind = "executable";
        path = "target/debug/synapse-qualisys-bridge";
        contract = {
          name = "synapse-qualisys-bridge-cli";
          version = 1;
        };
      };
    };
    executables.bridge.from = "synapse_qualisys_bridge:default:bridge";

    launches.mocap = {
      description = "Run the Synapse Qualisys bridge with its local dashboard.";
      parameters.health-port = {
        type = "port";
        description = "Mocap bridge HTTP dashboard port.";
        default = 8787;
      };
      requiredArtifacts = [ "synapse_qualisys_bridge:default:bridge" ];
      sessionEnvironment.SYNAPSE_QUALISYS_BRIDGE_CONFIG = {
        path = "bridge.toml";
        create = "parent";
      };
      processes.mocap = {
        executable = "synapse_qualisys_bridge:bridge";
        argv = [
          { literal = "--web-bind"; }
          {
            parameter = "health-port";
            prefix = "127.0.0.1:";
          }
        ];
        workingDirectory = ".";
        endpoints.dashboard = {
          protocol = "http";
          portParameter = "health-port";
          path = "/";
          expectedStatus = 200;
        };
        readiness = {
          kind = "endpoint";
          endpoint = "dashboard";
          timeoutMs = 300000;
        };
        restart = {
          policy = "on-failure";
          maxAttempts = 3;
          backoffMs = 1000;
        };
      };
      capabilities.provides = [ "mocap" ];
    };
  };
}
