{ config, lib, ... }:

let
  root = config.devenv.root;
  hasGroundStation = builtins.hasAttr "ground-station" config.processes;
  hasMocap = builtins.hasAttr "mocap" config.processes;
  usesSharedRouter = hasGroundStation || hasMocap;
  role = if hasMocap then "autopilot" else "both";
  networkArgs =
    if usesSharedRouter then
      ''--mode client --endpoint udp/127.0.0.1:7447 --ws-endpoint ""''
    else
      "--mode router --endpoint udp/127.0.0.1:7447 --ws-endpoint ws/127.0.0.1:7447";
  routerProcess = if hasGroundStation then "ground-station" else "mocap";
in
{
  processes.simulation = {
    cwd = "${root}/src/electrode_web";
    after = lib.optional usesSharedRouter "devenv:processes:${routerProcess}";
    exec = ''
      set -euo pipefail
      synapse_rust="${root}/src/synapse_fbs/target/xtask/packages/rust"
      if [[ ! -f "$synapse_rust/Cargo.toml" ]]; then
        printf 'local Synapse Rust package is missing; run: ws build synapse_fbs\n' >&2
        exit 1
      fi
      flake_ref="$(workspace-flake-ref --mode local "$PWD")"
      exec nix --accept-flake-config develop "$flake_ref" -c \
        cargo run --locked \
          --config "paths=[\"$synapse_rust\"]" \
          -p electrode-fake-sim -- \
          --role ${role} ${networkArgs}
    '';
    restart = {
      on = "on_failure";
      max = 3;
    };
  };
}
