{ config, lib, ... }:

let
  root = config.devenv.root;
  state = "${config.devenv.state}/launch/ground-station";
  hasLocalMocap = builtins.hasAttr "mocap" config.processes;
in
{
  processes.ground-station = {
    cwd = "${root}/src/electrode_web";
    env = {
      ELECTRODE_GCS_MAPPING_FILE = "${state}/mapping.json";
      ELECTRODE_GCS_AUTOPILOT_FILE = "${state}/autopilot.json";
      ELECTRODE_GCS_SIMULATION_FILE = "${state}/simulation.json";
      ELECTRODE_GCS_VELOCITY_BUDGET_DB = "${state}/velocity-budget-db.json";
      ELECTRODE_GCS_VELOCITY_BUDGET_CSV = "${state}/velocity-budget.csv";
    } // lib.optionalAttrs (!hasLocalMocap) {
      # Ground-station-only mode consumes Qualisys from the field router.
      # When the local mocap process is selected it connects directly to the
      # private vehicle router instead, so no external telemetry client is needed.
      ELECTRODE_GCS_TELEMETRY_ZENOH_CONNECT = "udp/192.168.10.2:7447";
    };
    exec = ''
      set -euo pipefail
      mkdir -p "${state}"

      station="${root}/src/electrode_web/target/debug/electrode-ground-station"
      web_index="${root}/src/electrode_web/apps/web/build/index.html"
      for artifact in "$station" "$web_index"; do
        if [[ ! -f "$artifact" ]]; then
          printf 'ground-station build artifact is missing: %s\n' "$artifact" >&2
          printf 'run: ws build electrode_web\n' >&2
          exit 1
        fi
      done
      if [[ ! -x "$station" ]]; then
        printf 'ground-station binary is not executable: %s\n' "$station" >&2
        printf 'run: ws build electrode_web\n' >&2
        exit 1
      fi

      exec "$station"
    '';
    ready = {
      http.get = {
        port = 8790;
        path = "/gcs/health";
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
