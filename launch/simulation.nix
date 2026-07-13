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
      simulator="${root}/src/electrode_web/target/debug/electrode-fake-sim"
      if [[ ! -x "$simulator" ]]; then
        printf 'simulation build artifact is missing or not executable: %s\n' "$simulator" >&2
        printf 'run: ws build electrode_web\n' >&2
        exit 1
      fi

      exec "$simulator" --role ${role} ${networkArgs}
    '';
    restart = {
      on = "on_failure";
      max = 3;
    };
  };
}
