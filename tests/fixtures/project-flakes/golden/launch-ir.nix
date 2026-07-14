{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.app = {
    repositoryId = "app";
    source.input = "app-source";
    preset = "cargo-v1";

    targets.default.artifacts.outputs = {
      router-bin = {
        kind = "executable";
        path = "bin/router";
        contract = {
          name = "router-cli";
          version = 1;
        };
      };
      monitor-bin = {
        kind = "executable";
        path = "bin/monitor";
        contract = {
          name = "monitor-cli";
          version = 1;
        };
      };
    };
    resources.router-config = {
      kind = "configuration";
      path = "config/router.json";
    };
    executables = {
      router.from = "app:default:router-bin";
      monitor.from = "app:default:monitor-bin";
    };

    launches = {
      router = {
        description = "Start the router and readiness monitor.";
        parameters = {
          host = {
            type = "host";
            default = "127.0.0.1";
          };
          port = {
            type = "port";
            default = 7447;
            minimum = 1;
            maximum = 65535;
          };
          log-level = {
            type = "enum";
            enumValues = [
              "debug"
              "info"
              "warn"
            ];
            default = "info";
          };
          config = {
            type = "path";
            default = "config/router.json";
            allowedRoots = [ "config" ];
            mustExist = true;
          };
          token = {
            type = "secret";
            required = true;
          };
        };
        requiredArtifacts = [ "app:default:router-bin" ];
        requiredResources = [ "app:router-config" ];
        sessionEnvironment.ROUTER_STATE = {
          path = "router/state.json";
          create = "parent";
        };
        processes = {
          router = {
            executable = "app:router";
            argv = [
              { literal = "--host"; }
              { parameter = "host"; }
              { literal = "--port"; }
              { parameter = "port"; }
              { literal = "--config"; }
              { parameter = "config"; }
            ];
            environment = {
              LOG_LEVEL.parameter = "log-level";
              ROUTER_TOKEN.parameter = "token";
            };
            workingDirectory = ".";
            endpoints.http = {
              protocol = "http";
              hostParameter = "host";
              portParameter = "port";
              path = "/ready";
              expectedStatus = 204;
            };
            readiness = {
              kind = "endpoint";
              endpoint = "http";
              timeoutMs = 5000;
            };
            restart = {
              policy = "on-failure";
              maxAttempts = 3;
              backoffMs = 250;
            };
            shutdown = {
              signal = "SIGTERM";
              timeoutMs = 3000;
              killSignal = "SIGKILL";
            };
            onExit = "restart";
            onReadinessLoss = "restart";
          };
          monitor = {
            executable = "app:monitor";
            dependencies.router = "ready";
            readiness.kind = "started";
          };
        };
        capabilities.provides = [ "router" ];
      };

      stack = {
        description = "Compose the router with renamed bundle parameters.";
        parameters = {
          router-host = {
            type = "host";
            default = "127.0.0.1";
          };
          router-port = {
            type = "port";
            default = 7447;
          };
          router-log = {
            type = "enum";
            enumValues = [
              "debug"
              "info"
              "warn"
            ];
            default = "info";
          };
          router-config = {
            type = "path";
            default = "config/router.json";
            allowedRoots = [ "config" ];
          };
          router-token = {
            type = "secret";
            required = true;
          };
        };
        includes.base = {
          launch = "app:router";
          parameters = {
            host = "router-host";
            port = "router-port";
            log-level = "router-log";
            config = "router-config";
            token = "router-token";
          };
        };
        capabilities.provides = [ "simulation-stack" ];
        capabilities.requires = [ "router" ];
      };
    };
  };
}
