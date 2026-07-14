let
  stackSessionEnvironment = {
    ELECTRODE_GCS_MAPPING_FILE = {
      path = "ground-station/mapping.json";
      create = "parent";
    };
    ELECTRODE_GCS_AUTOPILOT_FILE = {
      path = "ground-station/autopilot.json";
      create = "parent";
    };
    ELECTRODE_GCS_SIMULATION_FILE = {
      path = "ground-station/simulation.json";
      create = "parent";
    };
    ELECTRODE_GCS_VELOCITY_BUDGET_DB = {
      path = "ground-station/velocity-budget-db.json";
      create = "parent";
    };
    ELECTRODE_GCS_VELOCITY_BUDGET_CSV = {
      path = "ground-station/velocity-budget.csv";
      create = "parent";
    };
  };

  stackGroundStation = {
    executable = "electrode_web:ground-station";
    argv = [
      { literal = "--addr"; }
      {
        parameter = "ground-station-health-port";
        prefix = "127.0.0.1:";
      }
    ];
    workingDirectory = ".";
    endpoints.health = {
      protocol = "http";
      portParameter = "ground-station-health-port";
      path = "/gcs/health";
      expectedStatus = 200;
    };
    readiness = {
      kind = "endpoint";
      endpoint = "health";
      timeoutMs = 300000;
    };
    restart = {
      policy = "on-failure";
      maxAttempts = 3;
      backoffMs = 1000;
    };
  };

  stackSimulation = {
    executable = "electrode_web:fake-sim";
    dependencies.ground-station = "ready";
    argv = (map (literal: { inherit literal; }) [
      "--role"
      "autopilot"
      "--mode"
      "client"
      "--endpoint"
      "udp/127.0.0.1:7447"
      "--ws-endpoint"
    ]) ++ [ { parameter = "simulation-websocket-endpoint"; } ];
    workingDirectory = ".";
    restart = {
      policy = "on-failure";
      maxAttempts = 3;
    };
  };

  simulationStack = {
    description = "Run the local simulation and ground station.";
    parameters = {
      ground-station-health-port = {
        type = "port";
        default = 8790;
        allocation = "automatic";
      };
      simulation-websocket-endpoint = {
        type = "string";
        description = "Simulator WebSocket endpoint; empty disables the standalone server.";
        default = "";
      };
    };
    requiredArtifacts = [
      "electrode_web:default:fake-sim"
      "electrode_web:default:ground-station"
      "electrode_web:default:web-index"
    ];
    sessionEnvironment = stackSessionEnvironment;
    processes = {
      ground-station = stackGroundStation;
      simulation = stackSimulation;
    };
    capabilities.provides = [
      "ground-station"
      "simulation"
      "simulation-stack"
    ];
  };

  simulationStackMocap = simulationStack // {
    description = "Run the local simulation, ground station, and Qualisys bridge.";
    parameters = simulationStack.parameters // {
      mocap-health-port = {
        type = "port";
        default = 8787;
        allocation = "automatic";
      };
    };
    requiredArtifacts = simulationStack.requiredArtifacts ++ [
      "synapse_qualisys_bridge:default:bridge"
    ];
    sessionEnvironment = simulationStack.sessionEnvironment // {
      SYNAPSE_QUALISYS_BRIDGE_CONFIG = {
        path = "mocap/bridge.toml";
        create = "parent";
      };
    };
    processes = simulationStack.processes // {
      mocap = {
        executable = "synapse_qualisys_bridge:bridge";
        dependencies.ground-station = "ready";
        argv = map (literal: { inherit literal; }) [
          "--zenoh-mode"
          "client"
          "--zenoh-connect"
          "udp/127.0.0.1:7447"
        ] ++ [
          { literal = "--web-bind"; }
          {
            parameter = "mocap-health-port";
            prefix = "127.0.0.1:";
          }
        ];
        workingDirectory = ".";
        endpoints.dashboard = {
          protocol = "http";
          portParameter = "mocap-health-port";
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
    };
    capabilities = {
      provides = simulationStack.capabilities.provides ++ [
        "mocap"
        "simulation-stack-mocap"
      ];
    };
  };
in
{
  cognipilot.projects.electrode_web = {
    lifecycle = "stable";
    softwareVersion = {
      source = "literal";
      value = "0.1.0";
    };
    deployability = "qualification";
    owner = "CogniPilot";
    license.spdx = "Apache-2.0";

    source.visibility = "public";
    definition.origin = "external";
    preset = "cargo-npm-v1";

    targets.default.artifacts = {
      inputs = {
        synapse-rust = {
          from = "synapse_fbs:default:rust";
          consumedBy = [
            "cargo-build"
            "cargo-test"
          ];
          contract = {
            name = "synapse-rust-bindings";
            version = 1;
          };
        };
        synapse-javascript = {
          from = "synapse_fbs:default:javascript";
          consumedBy = [ "npm-bind-synapse" ];
          contract = {
            name = "synapse-javascript-bindings";
            version = 1;
          };
        };
        rumoca-javascript = {
          from = "rumoca:default:javascript";
          consumedBy = [ "npm-bind-rumoca" ];
          contract = {
            name = "rumoca-javascript-package";
            version = 1;
          };
        };
      };
      outputs = {
        ground-station = {
          producedBy = "cargo-build";
          kind = "executable";
          path = "target/debug/electrode-ground-station";
          contract = {
            name = "electrode-ground-station";
            version = 1;
          };
        };
        fake-sim = {
          producedBy = "cargo-build";
          kind = "executable";
          path = "target/debug/electrode-fake-sim";
          contract = {
            name = "electrode-fake-sim";
            version = 1;
          };
        };
        web-index = {
          producedBy = "npm-build";
          kind = "file";
          path = "apps/web/build/index.html";
          contract = {
            name = "electrode-web-index";
            version = 1;
          };
        };
      };
    };

    executables = {
      ground-station.from = "electrode_web:default:ground-station";
      fake-sim.from = "electrode_web:default:fake-sim";
    };

    launches.ground-station = {
      description = "Run the Electrode ground station against a telemetry router.";
      parameters = {
        telemetry-endpoint = {
          type = "url";
          description = "Zenoh telemetry router endpoint.";
          default = "udp/192.168.10.2:7447";
        };
        health-port = {
          type = "port";
          description = "Ground-station HTTP health endpoint port.";
          default = 8790;
        };
      };
      requiredArtifacts = [
        "electrode_web:default:ground-station"
        "electrode_web:default:web-index"
      ];
      sessionEnvironment = {
        ELECTRODE_GCS_MAPPING_FILE = {
          path = "mapping.json";
          create = "parent";
        };
        ELECTRODE_GCS_AUTOPILOT_FILE = {
          path = "autopilot.json";
          create = "parent";
        };
        ELECTRODE_GCS_SIMULATION_FILE = {
          path = "simulation.json";
          create = "parent";
        };
        ELECTRODE_GCS_VELOCITY_BUDGET_DB = {
          path = "velocity-budget-db.json";
          create = "parent";
        };
        ELECTRODE_GCS_VELOCITY_BUDGET_CSV = {
          path = "velocity-budget.csv";
          create = "parent";
        };
      };
      processes.ground-station = {
        executable = "electrode_web:ground-station";
        argv = [
          { literal = "--addr"; }
          {
            parameter = "health-port";
            prefix = "127.0.0.1:";
          }
        ];
        environment.ELECTRODE_GCS_TELEMETRY_ZENOH_CONNECT.parameter = "telemetry-endpoint";
        workingDirectory = ".";
        endpoints.health = {
          protocol = "http";
          portParameter = "health-port";
          path = "/gcs/health";
          expectedStatus = 200;
        };
        readiness = {
          kind = "endpoint";
          endpoint = "health";
          timeoutMs = 300000;
        };
        restart = {
          policy = "on-failure";
          maxAttempts = 3;
          backoffMs = 1000;
        };
        shutdown = {
          signal = "SIGTERM";
          timeoutMs = 10000;
          killSignal = "SIGKILL";
        };
      };
      capabilities.provides = [ "ground-station" ];
    };

    launches.simulation = {
      description = "Run the Electrode fake simulator as a local Zenoh router.";
      requiredArtifacts = [ "electrode_web:default:fake-sim" ];
      processes.simulation = {
        executable = "electrode_web:fake-sim";
        argv = map (literal: { inherit literal; }) [
          "--role"
          "both"
          "--mode"
          "router"
          "--endpoint"
          "udp/127.0.0.1:7447"
          "--ws-endpoint"
          "ws/127.0.0.1:7447"
        ];
        workingDirectory = ".";
        restart = {
          policy = "on-failure";
          maxAttempts = 3;
        };
      };
      capabilities.provides = [ "simulation" ];
    };

    # The default stack remains usable without the optional Qualisys provider.
    # Selecting the mocap capability is an explicit, differently named bundle.
    launches.simulation-stack = simulationStack;
    launches.simulation-stack-mocap = simulationStackMocap;
  };
}
