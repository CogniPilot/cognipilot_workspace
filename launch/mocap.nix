{ config, lib, ... }:

let
  root = config.devenv.root;
  state = "${config.devenv.state}/launch/mocap";
  hasGroundStation = builtins.hasAttr "ground-station" config.processes;
  networkArgs = lib.optionalString hasGroundStation (
    lib.concatStringsSep " " [
      "--zenoh-mode client"
      "--zenoh-connect udp/127.0.0.1:7447"
    ]
  );
in
{
  processes.mocap = {
    cwd = "${root}/src/synapse_qualisys_bridge";
    after = lib.optional hasGroundStation "devenv:processes:ground-station";
    env.SYNAPSE_QUALISYS_BRIDGE_CONFIG = "${state}/bridge.toml";
    exec = ''
      set -euo pipefail
      mkdir -p "${state}"
      bridge="${root}/src/synapse_qualisys_bridge/target/debug/synapse-qualisys-bridge"
      if [[ ! -x "$bridge" ]]; then
        printf 'mocap build artifact is missing or not executable: %s\n' "$bridge" >&2
        printf 'run: ws build synapse_qualisys_bridge\n' >&2
        exit 1
      fi

      exec "$bridge" ${networkArgs}
    '';
    ready = {
      http.get = {
        port = 8787;
        path = "/";
      };
      initial_delay = 1;
      period = 2;
      timeout = 300;
      failure_threshold = 150;
    };
    restart = {
      on = "on_failure";
      max = 3;
    };
  };
}
