{ root }:

let
  strict = command: ''
    set -euo pipefail
    ${command}
  '';
  flake = path: "path:$DEVENV_ROOT/${path}";
  synapseRust = "$DEVENV_ROOT/src/synapse_fbs/target/xtask/packages/rust";
  synapseC = "$DEVENV_ROOT/src/synapse_fbs/target/xtask/artifacts-work/synapse_fbs-c";
  resetStaleWestBuild = ''
    reset_stale_west_build() {
      local build_dir="$1"
      local west_root="$2"
      local cache="$build_dir/CMakeCache.txt"
      local cached_zephyr expected_zephyr
      [ -f "$cache" ] || return 0
      cached_zephyr="$(sed -n 's/^ZEPHYR_BASE:PATH=//p' "$cache" | head -1)"
      expected_zephyr="$(realpath -m "$west_root/zephyr")"
      if [ -n "$cached_zephyr" ] &&
         [ "$(realpath -m "$cached_zephyr")" != "$expected_zephyr" ]; then
        printf 'west root changed; removing generated build directory: %s\n' "$build_dir"
        rm -rf "$build_dir"
      fi
    }
  '';
in
{
  synapse_fbs = {
    displayName = "Synapse generated language bindings";
    path = "src/synapse_fbs";
    repo = {
      github = "CogniPilot/synapse_fbs";
      branch = "main";
    };
    dependencies = [ ];
    local.build = strict ''
      mkdir -p "$DEVENV_STATE/locks"
      flock "$DEVENV_STATE/locks/synapse_fbs-packages.lock" \
        nix develop "${flake "src/synapse_fbs"}" -c \
          cargo run --locked --manifest-path xtask/Cargo.toml -- ci --release-name local
    '';
    release.build = strict ''
      mkdir -p "$DEVENV_STATE/locks"
      flock "$DEVENV_STATE/locks/synapse_fbs-packages.lock" \
        nix develop "${flake "src/synapse_fbs"}" -c \
          cargo run --locked --manifest-path xtask/Cargo.toml -- ci --release-name local
    '';
    local.test = strict ''
      nix develop "${flake "src/synapse_fbs"}" -c \
        cargo run --locked --manifest-path xtask/Cargo.toml -- check
    '';
    release.test = strict ''
      nix flake check "${flake "src/synapse_fbs"}"
    '';
  };

  rumoca = {
    displayName = "Rumoca compiler and local Python/JavaScript artifacts";
    path = "src/rumoca";
    repo = {
      github = "CogniPilot/rumoca";
      branch = "main";
    };
    dependencies = [ ];
    local.build = strict ''
      mkdir -p "$DEVENV_STATE/results" "$DEVENV_STATE/wheels/rumoca"
      nix build "${flake "src/rumoca"}#rumoca" \
        --out-link "$DEVENV_STATE/results/rumoca"
      nix develop "${flake "src/rumoca"}" -c \
        maturin build --locked --release \
          --manifest-path crates/rumoca-bind-python/Cargo.toml \
          --out "$DEVENV_STATE/wheels/rumoca"
      nix develop "${flake "src/rumoca"}" -c \
        npm --prefix packages/rumoca run build:dev

      if [ ! -x "$DEVENV_STATE/python/bin/python" ]; then
        python -m venv "$DEVENV_STATE/python"
      fi
      wheel="$(find "$DEVENV_STATE/wheels/rumoca" -maxdepth 1 -name '*.whl' \
        -type f -printf '%T@ %p\n' | sort -nr | head -1 | cut -d' ' -f2-)"
      test -n "$wheel"
      "$DEVENV_STATE/python/bin/pip" install --no-deps --force-reinstall "$wheel"
    '';
    release.build = strict ''
      mkdir -p "$DEVENV_STATE/results"
      nix build "${flake "src/rumoca"}#rumoca" \
        --out-link "$DEVENV_STATE/results/rumoca-release"
    '';
    local.test = strict ''
      nix flake check "${flake "src/rumoca"}"
    '';
    release.test = strict ''
      nix flake check "${flake "src/rumoca"}"
    '';
  };

  modelica_models = {
    displayName = "CogniPilot Modelica model library";
    path = "src/modelica_models";
    repo = {
      github = "CogniPilot/modelica_models";
      branch = "main";
    };
    dependencies = [ "rumoca" ];
    local.build = strict ''
      mkdir -p "$DEVENV_STATE/results"
      nix build "${flake "src/modelica_models"}#ci" \
        --override-input rumoca "${flake "src/rumoca"}" \
        --out-link "$DEVENV_STATE/results/modelica-models"
    '';
    release.build = strict ''
      mkdir -p "$DEVENV_STATE/results"
      nix build "${flake "src/modelica_models"}#ci" \
        --out-link "$DEVENV_STATE/results/modelica-models-release"
    '';
    local.test = strict ''
      nix run "${flake "src/modelica_models"}#ci" \
        --override-input rumoca "${flake "src/rumoca"}"
    '';
    release.test = strict ''
      nix run "${flake "src/modelica_models"}#ci"
    '';
  };

  csyn = {
    displayName = "CSyn host CLI and Zephyr module";
    path = "src/csyn";
    repo = {
      github = "CogniPilot/csyn";
      branch = "main";
    };
    dependencies = [ "synapse_fbs" ];
    local.build = strict ''
      test -f "${synapseRust}/Cargo.toml"
      nix develop "${flake "src/csyn"}" -c \
        cargo build --locked --manifest-path rust/Cargo.toml \
          --config "paths=['${synapseRust}']"
    '';
    release.build = strict ''
      nix run "${flake "src/csyn"}#test-rust"
    '';
    local.test = strict ''
      nix develop "${flake "src/csyn"}" -c \
        cargo test --locked --manifest-path rust/Cargo.toml \
          --config "paths=['${synapseRust}']"
    '';
    release.test = strict ''
      nix run "${flake "src/csyn"}#ci"
    '';
  };

  cerebri_modules = {
    displayName = "Shared Cerebri Zephyr modules";
    path = "src/cerebri_modules";
    repo = {
      github = "CogniPilot/cerebri_modules";
      branch = "main";
    };
    dependencies = [ ];
    local.build = strict ''
      test -f zephyr/module.yml
      test -f zephyr/CMakeLists.txt
    '';
    release.build = strict ''
      test -f zephyr/module.yml
      test -f zephyr/CMakeLists.txt
    '';
    local.test = strict ''
      test -f tests/sequence/testcase.yaml
      test -f tests/sequence/CMakeLists.txt
    '';
    release.test = strict ''
      test -f tests/sequence/testcase.yaml
      test -f tests/sequence/CMakeLists.txt
    '';
  };

  zros = {
    displayName = "ZROS Zephyr pub/sub module";
    path = "src/zros";
    repo = {
      github = "CogniPilot/zros";
      branch = "main";
    };
    optional = true;
    dependencies = [ ];
    local.build = strict ''
      test -f zephyr/module.yml
      test -f zephyr/CMakeLists.txt
    '';
    release.build = strict ''
      test -f zephyr/module.yml
      test -f zephyr/CMakeLists.txt
    '';
    local.test = strict ''
      test -x scripts/format.py
    '';
    release.test = strict ''
      test -x scripts/format.py
    '';
  };

  zros_drivers = {
    displayName = "ZROS device drivers";
    path = "src/zros_drivers";
    repo = {
      github = "CogniPilot/zros_drivers";
      branch = "main";
    };
    optional = true;
    dependencies = [
      "synapse_fbs"
      "zros"
    ];
    local.build = strict ''
      test -f zephyr/module.yml
      test -f CMakeLists.txt
      test -f Kconfig
    '';
    release.build = strict ''
      test -f zephyr/module.yml
      test -f CMakeLists.txt
      test -f Kconfig
    '';
    local.test = strict ''
      test -d drivers
      test -d lib
    '';
    release.test = strict ''
      test -d drivers
      test -d lib
    '';
  };

  qualisys_rust_sdk = {
    displayName = "Qualisys Rust SDK";
    path = "src/qualisys_rust_sdk";
    repo = {
      github = "CogniPilot/qualisys_rust_sdk";
      branch = "main";
    };
    optional = true;
    dependencies = [ ];
    local.build = strict ''
      mkdir -p "$DEVENV_STATE/results"
      nix build "${flake "src/qualisys_rust_sdk"}#default" \
        --out-link "$DEVENV_STATE/results/qualisys-rust-sdk"
    '';
    release.build = strict ''
      mkdir -p "$DEVENV_STATE/results"
      nix build "${flake "src/qualisys_rust_sdk"}#default" \
        --out-link "$DEVENV_STATE/results/qualisys-rust-sdk-release"
    '';
    local.test = strict ''
      nix flake check "${flake "src/qualisys_rust_sdk"}"
    '';
    release.test = strict ''
      nix flake check "${flake "src/qualisys_rust_sdk"}"
    '';
  };

  synapse_qualisys_bridge = {
    displayName = "Synapse Qualisys motion-capture bridge";
    path = "src/synapse_qualisys_bridge";
    repo = {
      github = "CogniPilot/synapse_qualisys_bridge";
      branch = "main";
    };
    optional = true;
    dependencies = [
      "qualisys_rust_sdk"
      "synapse_fbs"
    ];
    local.build = strict ''
      test -f ../qualisys_rust_sdk/Cargo.toml
      test -f "${synapseRust}/Cargo.toml"
      nix develop "${flake "src/synapse_qualisys_bridge"}" -c \
        cargo build --locked \
          --config "paths=['${synapseRust}']"
    '';
    release.build = strict ''
      test -f ../qualisys_rust_sdk/Cargo.toml
      nix develop "${flake "src/synapse_qualisys_bridge"}" -c \
        cargo build --locked
    '';
    local.test = strict ''
      test -f ../qualisys_rust_sdk/Cargo.toml
      test -f "${synapseRust}/Cargo.toml"
      nix develop "${flake "src/synapse_qualisys_bridge"}" -c \
        cargo test --locked \
          --config "paths=['${synapseRust}']"
    '';
    release.test = strict ''
      test -f ../qualisys_rust_sdk/Cargo.toml
      nix develop "${flake "src/synapse_qualisys_bridge"}" -c \
        cargo test --locked
    '';
  };

  electrode_web = {
    displayName = "Electrode web ground station";
    path = "src/electrode_web";
    repo = {
      github = "CogniPilot/electrode_web";
      branch = "main";
    };
    dependencies = [
      "synapse_fbs"
      "rumoca"
    ];
    local.build = strict ''
      nix develop "${flake "src/electrode_web"}" -c \
        bash -euo pipefail -c '
          root="$1"
          synapse_js="$root/src/synapse_fbs/target/xtask/packages/js"
          synapse_rust="$root/src/synapse_fbs/target/xtask/packages/rust"
          rumoca_js="$root/src/rumoca/packages/rumoca/dist/dev-core"
          manifests=(
            Cargo.toml Cargo.lock package.json package-lock.json
            apps/web/package.json packages/electrode-sdk/package.json
          )
          manifests_before="$(sha256sum "''${manifests[@]}")"
          test -f "$synapse_js/package.json"
          test -f "$synapse_rust/Cargo.toml"
          test -f "$rumoca_js/package.json"
          npm ci
          npm install --no-save --package-lock=false "$synapse_js"
          npm install --workspace apps/web --no-save --package-lock=false "$rumoca_js"
          cargo build --locked \
            --config "paths=[\"$synapse_rust\"]"
          npm run build
          test "$manifests_before" = "$(sha256sum "''${manifests[@]}")"
        ' _ "$DEVENV_ROOT"
    '';
    release.build = strict ''
      nix develop "${flake "src/electrode_web"}" -c \
        bash -euo pipefail -c 'npm ci; cargo build --locked; npm run build'
    '';
    local.test = strict ''
      nix develop "${flake "src/electrode_web"}" -c \
        bash -euo pipefail -c '
          synapse_rust="$1"
          npm run ci
          cargo test --locked \
            --config "paths=[\"$synapse_rust\"]"
        ' _ "${synapseRust}"
    '';
    release.test = strict ''
      nix develop "${flake "src/electrode_web"}" -c \
        bash -euo pipefail -c 'npm ci; npm run ci; cargo test --locked'
    '';
  };

  cerebri_cubs2 = {
    displayName = "CUBS2 Zephyr firmware (native_sim)";
    path = "src/cerebri_cubs2";
    repo = {
      github = "CogniPilot/cerebri_cubs2";
      branch = "main";
    };
    dependencies = [
      "synapse_fbs"
      "rumoca"
      "modelica_models"
      "csyn"
      "cerebri_modules"
    ];
    needsWest = true;
    local.build = strict ''
      west_root="$(workspace-west path --mode local)"
      build_dir="$DEVENV_ROOT/src/cerebri_cubs2/build-native_sim_sil"
      ${resetStaleWestBuild}
      reset_stale_west_build "$build_dir" "$west_root"
      test -f "${synapseC}/CMakeLists.txt"
      test -x "$DEVENV_STATE/python/bin/python"
      CUBS2_WORKSPACE_ROOT="$west_root" \
      CUBS2_NATIVE_SIM_BUILD_DIR="$build_dir" \
      CUBS2_RUMOCA_PYTHON="$DEVENV_STATE/python/bin/python" \
      nix run "${flake "src/cerebri_cubs2"}#build-native-sim" -- \
        -p always -- \
        -DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C="${synapseC}"
    '';
    release.build = strict ''
      west_root="$(workspace-west path --mode release)"
      build_dir="$DEVENV_ROOT/src/cerebri_cubs2/build-native_sim_sil"
      ${resetStaleWestBuild}
      reset_stale_west_build "$build_dir" "$west_root"
      CUBS2_WORKSPACE_ROOT="$west_root" \
      CUBS2_NATIVE_SIM_BUILD_DIR="$build_dir" \
      nix run "${flake "src/cerebri_cubs2"}#build-native-sim" -- -p auto
    '';
    local.test = strict ''
      west_root="$(workspace-west path --mode local)"
      build_dir="$DEVENV_ROOT/src/cerebri_cubs2/build-native_sim_sil"
      ${resetStaleWestBuild}
      reset_stale_west_build "$build_dir" "$west_root"
      CUBS2_WORKSPACE_ROOT="$west_root" \
      CUBS2_NATIVE_SIM_BUILD_DIR="$build_dir" \
      CUBS2_RUMOCA_PYTHON="$DEVENV_STATE/python/bin/python" \
      nix run "${flake "src/cerebri_cubs2"}#native-sim-sil-test"
    '';
    release.test = strict ''
      west_root="$(workspace-west path --mode release)"
      build_dir="$DEVENV_ROOT/src/cerebri_cubs2/build-native_sim_sil"
      ${resetStaleWestBuild}
      reset_stale_west_build "$build_dir" "$west_root"
      CUBS2_WORKSPACE_ROOT="$west_root" \
      CUBS2_NATIVE_SIM_BUILD_DIR="$build_dir" \
      nix run "${flake "src/cerebri_cubs2"}#native-sim-sil-test"
    '';
  };

  cerebri_rdd2 = {
    displayName = "RDD2 Zephyr firmware (native_sim)";
    path = "src/cerebri_rdd2";
    repo = {
      github = "CogniPilot/cerebri_rdd2";
      branch = "main";
    };
    dependencies = [
      "synapse_fbs"
      "rumoca"
      "csyn"
      "cerebri_modules"
    ];
    needsWest = true;
    local.build = strict ''
      west_root="$(workspace-west path --mode local)"
      build_dir="$DEVENV_ROOT/src/cerebri_rdd2/build-native_sim"
      ${resetStaleWestBuild}
      reset_stale_west_build "$build_dir" "$west_root"
      test -f "${synapseC}/CMakeLists.txt"
      test -x "$DEVENV_STATE/results/rumoca/bin/rumoca"
      RDD2_WORKSPACE_ROOT="$west_root" \
      RDD2_NATIVE_SIM_BUILD_DIR="$build_dir" \
      nix run "${flake "src/cerebri_rdd2"}#build-native-sim" -- \
        -p always -- \
        -DRDD2_RUMOCA_VERSION=workspace \
        -DRDD2_RUMOCA_EXECUTABLE="$DEVENV_STATE/results/rumoca/bin/rumoca" \
        -DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C="${synapseC}"
    '';
    release.build = strict ''
      west_root="$(workspace-west path --mode release)"
      build_dir="$DEVENV_ROOT/src/cerebri_rdd2/build-native_sim"
      ${resetStaleWestBuild}
      reset_stale_west_build "$build_dir" "$west_root"
      RDD2_WORKSPACE_ROOT="$west_root" \
      RDD2_NATIVE_SIM_BUILD_DIR="$build_dir" \
      nix run "${flake "src/cerebri_rdd2"}#build-native-sim" -- -p auto
    '';
    local.test = strict ''
      west_root="$(workspace-west path --mode local)"
      build_dir="$DEVENV_ROOT/src/cerebri_rdd2/build-native_sim"
      ${resetStaleWestBuild}
      reset_stale_west_build "$build_dir" "$west_root"
      RDD2_WORKSPACE_ROOT="$west_root" \
      RDD2_NATIVE_SIM_BUILD_DIR="$build_dir" \
      nix run "${flake "src/cerebri_rdd2"}#build-native-sim" -- -p auto
    '';
    release.test = strict ''
      west_root="$(workspace-west path --mode release)"
      build_dir="$DEVENV_ROOT/src/cerebri_rdd2/build-native_sim"
      ${resetStaleWestBuild}
      reset_stale_west_build "$build_dir" "$west_root"
      RDD2_WORKSPACE_ROOT="$west_root" \
      RDD2_NATIVE_SIM_BUILD_DIR="$build_dir" \
      nix run "${flake "src/cerebri_rdd2"}#build-native-sim" -- -p auto
    '';
  };

  csyn_ros2_bridge = {
    displayName = "CSyn ROS 2 Jazzy bridge";
    path = "src/csyn_ros2_bridge";
    repo = {
      github = "CogniPilot/csyn_ros2_bridge";
      branch = "main";
    };
    dependencies = [
      "synapse_fbs"
      "csyn"
    ];
    optional = true;
    local.build = strict ''
      if [ ! -f "$DEVENV_ROOT/src/csyn_ros2_bridge/flake.nix" ]; then
        echo "ROS 2 support is optional and is not installed."
        echo "Run 'ws sync csyn_ros2_bridge' to enable it."
        exit 0
      fi
      nix develop "${flake "src/csyn_ros2_bridge"}" -c \
        bash "$DEVENV_ROOT/scripts/build-csyn-ros2"
    '';
    release.build = strict ''
      if [ ! -f "$DEVENV_ROOT/src/csyn_ros2_bridge/flake.nix" ]; then
        echo "ROS 2 support is not installed; skipping optional release build."
        exit 0
      fi
      cd "$DEVENV_ROOT/src/csyn_ros2_bridge"
      git submodule update --init --recursive
      nix run "${flake "src/csyn_ros2_bridge"}#ci"
    '';
    local.test = strict ''
      if [ ! -f "$DEVENV_ROOT/src/csyn_ros2_bridge/flake.nix" ]; then
        echo "ROS 2 support is not installed; skipping optional tests."
        exit 0
      fi
      test -f "$DEVENV_STATE/build/csyn_ros2_bridge/log/latest_test/events.log"
      nix develop "${flake "src/csyn_ros2_bridge"}" -c \
        colcon test-result \
          --test-result-base "$DEVENV_STATE/build/csyn_ros2_bridge/build" \
          --verbose
    '';
    release.test = strict ''
      if [ ! -f "$DEVENV_ROOT/src/csyn_ros2_bridge/flake.nix" ]; then
        echo "ROS 2 support is not installed; skipping optional release tests."
        exit 0
      fi
      cd "$DEVENV_ROOT/src/csyn_ros2_bridge"
      git submodule update --init --recursive
      nix run "${flake "src/csyn_ros2_bridge"}#ci"
    '';
  };

  FastDyn = {
    displayName = "FastDyn QEMU plugin";
    path = "src/FastDyn";
    repo = {
      github = "jgoppert/FastDyn";
      branch = "fix/cerebri-fastdyn-lockstep";
      private = true;
      allowDeployed = true;
    };
    dependencies = [ ];
    optional = true;
    local.build = strict ''
      if [ ! -f "$DEVENV_ROOT/src/FastDyn/setup.sh" ]; then
        echo "FastDyn is optional and is not installed."
        echo "Run 'ws sync FastDyn' or copy a deployed tree to:"
        echo "  $DEVENV_ROOT/src/FastDyn"
        exit 0
      fi
      cd "$DEVENV_ROOT/src/FastDyn"
      mkdir -p "$DEVENV_STATE/fastdyn"
      source ./setup.sh \
        --venv "$DEVENV_STATE/fastdyn/venv" \
        --qemu-root "$DEVENV_STATE/fastdyn/qemu" \
        --build-qemu --skip-optifuzz --skip-qemu-workspace
    '';
    release.build = strict ''
      if [ ! -f "$DEVENV_ROOT/src/FastDyn/setup.sh" ]; then
        echo "FastDyn unavailable; skipping optional release build."
        echo "Copy a deployed tree to $DEVENV_ROOT/src/FastDyn to enable it."
        exit 0
      fi
      cd "$DEVENV_ROOT/src/FastDyn"
      mkdir -p "$DEVENV_STATE/fastdyn-release"
      source ./setup.sh \
        --venv "$DEVENV_STATE/fastdyn-release/venv" \
        --qemu-root "$DEVENV_STATE/fastdyn-release/qemu" \
        --build-qemu --skip-optifuzz --skip-qemu-workspace
    '';
    local.test = strict ''
      if [ ! -x "$DEVENV_STATE/fastdyn/venv/bin/python" ]; then
        echo "FastDyn unavailable; skipping optional tests."
        exit 0
      fi
      cd "$DEVENV_ROOT/src/FastDyn"
      "$DEVENV_STATE/fastdyn/venv/bin/python" -m pytest
    '';
    release.test = strict ''
      if [ ! -x "$DEVENV_STATE/fastdyn-release/venv/bin/python" ]; then
        echo "FastDyn unavailable; skipping optional release tests."
        exit 0
      fi
      cd "$DEVENV_ROOT/src/FastDyn"
      "$DEVENV_STATE/fastdyn-release/venv/bin/python" -m pytest
    '';
  };
}
