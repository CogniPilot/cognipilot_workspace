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
      synapse_rust="${root}/src/synapse_fbs/target/xtask/packages/rust"
      if [[ ! -f "$synapse_rust/Cargo.toml" ]]; then
        printf 'local Synapse Rust package is missing; run: ws build synapse_fbs\n' >&2
        exit 1
      fi
      flake_ref="$(workspace-flake-ref --mode local "$PWD")"
      exec nix --accept-flake-config develop "$flake_ref" -c \
        cargo run --locked \
          --config "paths=[\"$synapse_rust\"]" \
          --bin synapse-qualisys-bridge -- \
          ${networkArgs}
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
