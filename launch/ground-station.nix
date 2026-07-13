{ config, ... }:

let
  root = config.devenv.root;
  state = "${config.devenv.state}/launch/ground-station";
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
    };
    exec = ''
      set -euo pipefail
      mkdir -p "${state}"

      synapse_js="${root}/src/synapse_fbs/target/xtask/packages/js"
      synapse_rust="${root}/src/synapse_fbs/target/xtask/packages/rust"
      rumoca_js="${root}/src/rumoca/packages/rumoca/dist/dev-core"
      for package in "$synapse_js/package.json" "$synapse_rust/Cargo.toml" "$rumoca_js/package.json"; do
        if [[ ! -f "$package" ]]; then
          printf 'local generated package is missing: %s\n' "$package" >&2
          printf 'run: ws build electrode_web\n' >&2
          exit 1
        fi
      done

      flake_ref="$(workspace-flake-ref --mode local "$PWD")"
      exec nix --accept-flake-config develop "$flake_ref" -c \
        bash -euo pipefail -c '
          if [[ ! -d node_modules ]]; then
            npm ci
          fi
          npm install --no-save --package-lock=false "$1"
          npm install --workspace apps/web --no-save --package-lock=false "$3"
          npm run build
          exec cargo run --locked \
            --config "paths=[\"$2\"]" \
            -p electrode-ground-station --
        ' _ "$synapse_js" "$synapse_rust" "$rumoca_js"
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
