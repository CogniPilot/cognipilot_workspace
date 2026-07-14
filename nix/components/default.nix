{ lib, pkgs, ... }:

let
  inherit (lib) mkOption types;
  modeType = types.submodule {
    options = {
      build = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Native command used to build this component.";
      };
      buildOutputs = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = "Required local outputs used to validate an incremental task-cache entry.";
      };
      test = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Native command used to test this component.";
      };
    };
  };
  repoType = types.submodule {
    options = {
      github = mkOption { type = types.str; };
      branch = mkOption {
        type = types.str;
        default = "main";
      };
      private = mkOption {
        type = types.bool;
        default = false;
      };
      allowDeployed = mkOption {
        type = types.bool;
        default = false;
      };
    };
  };
  componentType = types.submodule (
    { name, ... }:
    {
      options = {
        displayName = mkOption { type = types.str; };
        path = mkOption {
          type = types.str;
          default = "src/${name}";
        };
        repo = mkOption { type = repoType; };
        dependencies = mkOption {
          type = types.listOf types.str;
          default = [ ];
          description = "Repositories whose source must be present in the editable workspace.";
        };
        buildDependencies = mkOption {
          type = types.listOf types.str;
          default = [ ];
          description = "Components whose built artifacts are required before this component builds.";
        };
        needsWest = mkOption {
          type = types.bool;
          default = false;
        };
        devenvProfile = mkOption {
          type = types.nullOr types.str;
          default = null;
        };
        local = mkOption { type = modeType; };
        release = mkOption { type = modeType; };
      };
    }
  );
  strict = command: ''
    set -euo pipefail
    ${command}
  '';
  # Tasks run in component-specific shells whose PATH is intentionally small.
  # Use the pinned executable rather than relying on a host profile entry.
  nix = "${pkgs.nix}/bin/nix --accept-flake-config";
  flake = path: ''$(workspace-flake-ref --mode local "$DEVENV_ROOT/${path}")'';
  releaseFlake = path: ''$(workspace-flake-ref --mode release "$DEVENV_ROOT/${path}")'';
  synapseRust = "$COGNIPILOT_DEVEL_ROOT/synapse_fbs/rust";
  synapseC = "$COGNIPILOT_DEVEL_ROOT/synapse_fbs/c";
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
  twister =
    {
      mode,
      component,
      buildOnly ? false,
      format ? false,
    }:
    strict ''
      west_root="$(workspace-west path --mode ${mode})"
      outdir="$COGNIPILOT_BUILD_ROOT/twister/${component}/${mode}-${
        if buildOnly then "build" else "test"
      }"
      ${lib.optionalString format ''
        "$COGNIPILOT_ZEPHYR_PYTHON" scripts/format.py --check
      ''}
      twister_args=(
        -T "$DEVENV_ROOT/src/${component}/tests"
        -p native_sim/native/64
        --force-platform
        --outdir "$outdir"
        --clobber-output
      )
      ${lib.optionalString buildOnly ''
        twister_args+=(--build-only)
      ''}
      NIX_HARDENING_ENABLE="" \
        ZEPHYR_BASE="$west_root/zephyr" \
        "$COGNIPILOT_ZEPHYR_PYTHON" "$west_root/zephyr/scripts/twister" \
          "''${twister_args[@]}"
    '';
in
{
  options.workspace.components = mkOption {
    type = types.attrsOf componentType;
    default = { };
    description = "Editable repositories and their local dependency graph.";
  };

  config.workspace.components = {
    synapse_fbs = {
      displayName = "Synapse generated language bindings";
      path = "src/synapse_fbs";
      repo = {
        github = "CogniPilot/synapse_fbs";
        branch = "main";
      };
      dependencies = [ ];
      local = {
        build = strict ''
          mkdir -p "$COGNIPILOT_BUILD_ROOT/locks" "$COGNIPILOT_DEVEL_ROOT/synapse_fbs"
          flock "$COGNIPILOT_BUILD_ROOT/locks/synapse_fbs-packages.lock" \
            ${nix} develop "${flake "src/synapse_fbs"}" -c \
              cargo run --locked --manifest-path xtask/Cargo.toml -- build --release-name local
          ln -sfnT "$DEVENV_ROOT/src/synapse_fbs/target/xtask/packages/rust" \
            "$COGNIPILOT_DEVEL_ROOT/synapse_fbs/rust"
          ln -sfnT "$DEVENV_ROOT/src/synapse_fbs/target/xtask/packages/python" \
            "$COGNIPILOT_DEVEL_ROOT/synapse_fbs/python"
          ln -sfnT "$DEVENV_ROOT/src/synapse_fbs/target/xtask/packages/js" \
            "$COGNIPILOT_DEVEL_ROOT/synapse_fbs/js"
          ln -sfnT "$DEVENV_ROOT/src/synapse_fbs/target/xtask/artifacts-work/synapse_fbs-c" \
            "$COGNIPILOT_DEVEL_ROOT/synapse_fbs/c"
        '';
        buildOutputs = [
          "devel:synapse_fbs/rust"
          "devel:synapse_fbs/python"
          "devel:synapse_fbs/js"
          "devel:synapse_fbs/c"
        ];
        test = strict ''
          ${nix} develop "${flake "src/synapse_fbs"}" -c \
            cargo run --locked --manifest-path xtask/Cargo.toml -- check
        '';
      };
      release.build = strict ''
        mkdir -p "$COGNIPILOT_BUILD_ROOT/locks"
        flock "$COGNIPILOT_BUILD_ROOT/locks/synapse_fbs-packages.lock" \
          ${nix} develop "${releaseFlake "src/synapse_fbs"}" -c \
            cargo run --locked --manifest-path xtask/Cargo.toml -- ci --release-name local
      '';
      release.test = strict ''
        ${nix} flake check "${releaseFlake "src/synapse_fbs"}"
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
      local = {
        build = strict ''
          mkdir -p "$COGNIPILOT_DEVEL_ROOT"
          ${nix} build "${flake "src/rumoca"}#rumoca" \
            --out-link "$COGNIPILOT_DEVEL_ROOT/rumoca"
          ${nix} build "${flake "src/rumoca"}#rumoca-python-env" \
            --out-link "$COGNIPILOT_DEVEL_ROOT/rumoca-python"
          ${nix} develop "${flake "src/rumoca"}" -c \
            npm --prefix packages/rumoca run build:dev
          ln -sfnT "$DEVENV_ROOT/src/rumoca/packages/rumoca/dist/dev-core" \
            "$COGNIPILOT_DEVEL_ROOT/rumoca-js"
        '';
        buildOutputs = [
          "devel:rumoca/bin/rumoca"
          "devel:rumoca-python/bin/python"
          "devel:rumoca-js/package.json"
        ];
        test = strict ''
          ${nix} flake check "${flake "src/rumoca"}"
        '';
      };
      release.build = strict ''
        mkdir -p "$COGNIPILOT_RELEASE_RESULTS"
        ${nix} build "${releaseFlake "src/rumoca"}#rumoca" \
          --out-link "$COGNIPILOT_RELEASE_RESULTS/rumoca"
      '';
      release.test = strict ''
        ${nix} flake check "${releaseFlake "src/rumoca"}"
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
      buildDependencies = [ "rumoca" ];
      local = {
        build = strict ''
          mkdir -p "$COGNIPILOT_DEVEL_ROOT"
          ${nix} build "${flake "src/modelica_models"}#ci" \
            --override-input rumoca "${flake "src/rumoca"}" \
            --out-link "$COGNIPILOT_DEVEL_ROOT/modelica-models"
        '';
        buildOutputs = [ "devel:modelica-models" ];
        test = strict ''
          ${nix} run "${flake "src/modelica_models"}#ci" \
            --override-input rumoca "${flake "src/rumoca"}"
        '';
      };
      release.build = strict ''
        mkdir -p "$COGNIPILOT_RELEASE_RESULTS"
        ${nix} build "${releaseFlake "src/modelica_models"}#ci" \
          --out-link "$COGNIPILOT_RELEASE_RESULTS/modelica-models"
      '';
      release.test = strict ''
        ${nix} run "${releaseFlake "src/modelica_models"}#ci"
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
      buildDependencies = [ "synapse_fbs" ];
      local = {
        build = strict ''
          test -f "${synapseRust}/Cargo.toml"
          ${nix} develop "${flake "src/csyn"}" -c \
            cargo build --locked --manifest-path rust/Cargo.toml \
              --config "paths=['${synapseRust}']"
        '';
        buildOutputs = [ "repo:rust/target/debug/csyn" ];
        test = strict ''
          ${nix} develop "${flake "src/csyn"}" -c \
            cargo test --locked --manifest-path rust/Cargo.toml \
              --config "paths=['${synapseRust}']"
        '';
      };
      release.build = strict ''
        ${nix} run "${releaseFlake "src/csyn"}#test-rust"
      '';
      release.test = strict ''
        ${nix} run "${releaseFlake "src/csyn"}#ci"
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
      needsWest = true;
      devenvProfile = "zephyr-tests";
      local = {
        build = twister {
          mode = "local";
          component = "cerebri_modules";
          buildOnly = true;
        };
        buildOutputs = [ "build:twister/cerebri_modules/local-build/twister.json" ];
        test = twister {
          mode = "local";
          component = "cerebri_modules";
        };
      };
      release.build = twister {
        mode = "release";
        component = "cerebri_modules";
        buildOnly = true;
      };
      release.test = twister {
        mode = "release";
        component = "cerebri_modules";
      };
    };

    zros = {
      displayName = "ZROS Zephyr pub/sub module";
      path = "src/zros";
      repo = {
        github = "CogniPilot/zros";
        branch = "main";
      };
      dependencies = [ ];
      needsWest = true;
      devenvProfile = "zephyr-tests";
      local.build = twister {
        mode = "local";
        component = "zros";
        buildOnly = true;
        format = true;
      };
      release.build = twister {
        mode = "release";
        component = "zros";
        buildOnly = true;
        format = true;
      };
      local.test = twister {
        mode = "local";
        component = "zros";
        format = true;
      };
      release.test = twister {
        mode = "release";
        component = "zros";
        format = true;
      };
    };

    zros_drivers = {
      displayName = "ZROS device drivers";
      path = "src/zros_drivers";
      repo = {
        github = "CogniPilot/zros_drivers";
        branch = "main";
      };
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
      local.test = null;
      release.test = null;
    };

    qualisys_rust_sdk = {
      displayName = "Qualisys Rust SDK";
      path = "src/qualisys_rust_sdk";
      repo = {
        github = "CogniPilot/qualisys_rust_sdk";
        branch = "main";
      };
      dependencies = [ ];
      local.build = strict ''
        mkdir -p "$COGNIPILOT_DEVEL_ROOT"
        ${nix} build "${flake "src/qualisys_rust_sdk"}#default" \
          --out-link "$COGNIPILOT_DEVEL_ROOT/qualisys-rust-sdk"
      '';
      release.build = strict ''
        mkdir -p "$COGNIPILOT_RELEASE_RESULTS"
        ${nix} build "${releaseFlake "src/qualisys_rust_sdk"}#default" \
          --out-link "$COGNIPILOT_RELEASE_RESULTS/qualisys-rust-sdk"
      '';
      local.test = strict ''
        ${nix} flake check "${flake "src/qualisys_rust_sdk"}"
      '';
      release.test = strict ''
        ${nix} flake check "${releaseFlake "src/qualisys_rust_sdk"}"
      '';
    };

    synapse_qualisys_bridge = {
      displayName = "Synapse Qualisys motion-capture bridge";
      path = "src/synapse_qualisys_bridge";
      repo = {
        github = "CogniPilot/synapse_qualisys_bridge";
        branch = "main";
      };
      dependencies = [
        "qualisys_rust_sdk"
        "synapse_fbs"
      ];
      buildDependencies = [ "synapse_fbs" ];
      local.build = strict ''
        test -f ../qualisys_rust_sdk/Cargo.toml
        test -f "${synapseRust}/Cargo.toml"
        ${nix} develop "${flake "src/synapse_qualisys_bridge"}" -c \
          cargo build --locked \
            --config "paths=['${synapseRust}']"
      '';
      release.build = strict ''
        test -f ../qualisys_rust_sdk/Cargo.toml
        ${nix} develop "${releaseFlake "src/synapse_qualisys_bridge"}" -c \
          cargo build --locked
      '';
      local.test = strict ''
        test -f ../qualisys_rust_sdk/Cargo.toml
        test -f "${synapseRust}/Cargo.toml"
        ${nix} develop "${flake "src/synapse_qualisys_bridge"}" -c \
          cargo test --locked \
            --config "paths=['${synapseRust}']"
      '';
      release.test = strict ''
        test -f ../qualisys_rust_sdk/Cargo.toml
        ${nix} develop "${releaseFlake "src/synapse_qualisys_bridge"}" -c \
          cargo test --locked
      '';
    };

    synapse_ppm_bridge = {
      displayName = "Synapse PPM serial bridge";
      path = "src/synapse_ppm_bridge";
      repo = {
        github = "CogniPilot/synapse_ppm_bridge";
        branch = "main";
      };
      dependencies = [ ];
      devenvProfile = "rust-serial-toolchain";
      local = {
        build = strict ''
          cargo build --locked
        '';
        buildOutputs = [ "repo:target/debug/synapse-ppm-bridge" ];
        test = strict ''
          cargo test --locked
        '';
      };
      release.build = strict ''
        cargo build --locked --release
      '';
      release.test = strict ''
        cargo test --locked --release
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
      buildDependencies = [
        "synapse_fbs"
        "rumoca"
      ];
      local = {
        build = strict ''
          ${nix} develop "${flake "src/electrode_web"}" -c \
            bash -euo pipefail -c '
              root="$1"
              synapse_js="$COGNIPILOT_DEVEL_ROOT/synapse_fbs/js"
              synapse_rust="$COGNIPILOT_DEVEL_ROOT/synapse_fbs/rust"
              rumoca_js="$COGNIPILOT_DEVEL_ROOT/rumoca-js"
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
        buildOutputs = [
          "repo:target/debug/electrode-ground-station"
          "repo:target/debug/electrode-fake-sim"
          "repo:apps/web/build/index.html"
        ];
        test = strict ''
          ${nix} develop "${flake "src/electrode_web"}" -c \
            bash -euo pipefail -c '
              synapse_rust="$1"
              npm run ci
              cargo test --locked \
                --config "paths=[\"$synapse_rust\"]"
            ' _ "${synapseRust}"
        '';
      };
      release.build = strict ''
        ${nix} develop "${releaseFlake "src/electrode_web"}" -c \
          bash -euo pipefail -c 'npm ci; cargo build --locked; npm run build'
      '';
      release.test = strict ''
        ${nix} develop "${releaseFlake "src/electrode_web"}" -c \
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
      buildDependencies = [
        "synapse_fbs"
        "rumoca"
      ];
      needsWest = true;
      local.build = strict ''
        west_root="$(workspace-west path --mode local)"
        build_dir="$COGNIPILOT_BUILD_ROOT/cerebri_cubs2/native_sim_sil"
        flake_ref="${flake "src/cerebri_cubs2"}"
        west_marker="$(dirname "$west_root")/shared/.cognipilot_workspace.json"
        ${resetStaleWestBuild}
        reset_stale_west_build "$build_dir" "$west_root"
        test -f "${synapseC}/CMakeLists.txt"
        test -x "$COGNIPILOT_DEVEL_ROOT/rumoca-python/bin/python"
        BOARD="native_sim/native/64" \
        ZEPHYR_TOOLCHAIN_VARIANT="host" \
        CUBS2_WORKSPACE_ROOT="$west_root" \
        CUBS2_NATIVE_SIM_BUILD_DIR="$build_dir" \
        CUBS2_RUMOCA_PYTHON="$COGNIPILOT_DEVEL_ROOT/rumoca-python/bin/python" \
        "$DEVENV_ROOT/scripts/workspace-cmake-build" \
          --build-dir "$build_dir" \
          --flake-ref "$flake_ref" \
          --config-file "$DEVENV_ROOT/src/cerebri_cubs2/flake.nix" \
          --config-file "$DEVENV_ROOT/src/cerebri_cubs2/flake.lock" \
          --config-file "$west_marker" \
          --config-value "board=native_sim/native/64" \
          --config-value "synapse-c=${synapseC}" \
          -- \
          ${nix} run "$flake_ref#build-native-sim" -- \
            -p auto -- \
            -DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C="${synapseC}"
      '';
      release.build = strict ''
        west_root="$(workspace-west path --mode release)"
        build_dir="$COGNIPILOT_BUILD_ROOT/release/cerebri_cubs2/native_sim_sil"
        ${resetStaleWestBuild}
        reset_stale_west_build "$build_dir" "$west_root"
        CUBS2_WORKSPACE_ROOT="$west_root" \
        CUBS2_NATIVE_SIM_BUILD_DIR="$build_dir" \
        ${nix} run "${releaseFlake "src/cerebri_cubs2"}#build-native-sim" -- -p auto
      '';
      local.test = strict ''
        west_root="$(workspace-west path --mode local)"
        build_dir="$COGNIPILOT_BUILD_ROOT/cerebri_cubs2/native_sim_sil"
        ${resetStaleWestBuild}
        reset_stale_west_build "$build_dir" "$west_root"
        test -x "$COGNIPILOT_DEVEL_ROOT/rumoca-python/bin/python"
        CUBS2_WORKSPACE_ROOT="$west_root" \
        CUBS2_NATIVE_SIM_BUILD_DIR="$build_dir" \
        CUBS2_RUMOCA_PYTHON="$COGNIPILOT_DEVEL_ROOT/rumoca-python/bin/python" \
        CUBS2_SYNAPSE_C_ROOT="${synapseC}" \
        ${nix} run "${flake "src/cerebri_cubs2"}#native-sim-sil-test"
      '';
      release.test = strict ''
        west_root="$(workspace-west path --mode release)"
        build_dir="$COGNIPILOT_BUILD_ROOT/release/cerebri_cubs2/native_sim_sil"
        ${resetStaleWestBuild}
        reset_stale_west_build "$build_dir" "$west_root"
        CUBS2_WORKSPACE_ROOT="$west_root" \
        CUBS2_NATIVE_SIM_BUILD_DIR="$build_dir" \
        ${nix} run "${releaseFlake "src/cerebri_cubs2"}#native-sim-sil-test"
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
      buildDependencies = [
        "synapse_fbs"
        "rumoca"
      ];
      needsWest = true;
      local.build = strict ''
        west_root="$(workspace-west path --mode local)"
        build_dir="$COGNIPILOT_BUILD_ROOT/cerebri_rdd2/native_sim"
        ${resetStaleWestBuild}
        reset_stale_west_build "$build_dir" "$west_root"
        test -f "${synapseC}/CMakeLists.txt"
        test -x "$COGNIPILOT_DEVEL_ROOT/rumoca/bin/rumoca"
        RDD2_WORKSPACE_ROOT="$west_root" \
        RDD2_NATIVE_SIM_BUILD_DIR="$build_dir" \
        ${nix} run "${flake "src/cerebri_rdd2"}#build-native-sim" -- \
          -p auto -- \
          -DRDD2_RUMOCA_VERSION=workspace \
          -DRDD2_RUMOCA_EXECUTABLE="$COGNIPILOT_DEVEL_ROOT/rumoca/bin/rumoca" \
          -DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C="${synapseC}"
      '';
      release.build = strict ''
        west_root="$(workspace-west path --mode release)"
        build_dir="$COGNIPILOT_BUILD_ROOT/release/cerebri_rdd2/native_sim"
        ${resetStaleWestBuild}
        reset_stale_west_build "$build_dir" "$west_root"
        RDD2_WORKSPACE_ROOT="$west_root" \
        RDD2_NATIVE_SIM_BUILD_DIR="$build_dir" \
        ${nix} run "${releaseFlake "src/cerebri_rdd2"}#build-native-sim" -- -p auto
      '';
      # RDD2 does not yet expose a runtime/SIL test app; do not report a rebuild
      # as a passing test.
      local.test = null;
      release.test = null;
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
      buildDependencies = [ "synapse_fbs" ];
      local.build = strict ''
        if [ ! -f "$DEVENV_ROOT/src/csyn_ros2_bridge/flake.nix" ]; then
          echo "ROS 2 support is optional and is not installed."
          echo "Run 'ws sync csyn_ros2_bridge' to enable it."
          exit 0
        fi
        ${nix} develop "${flake "src/csyn_ros2_bridge"}" -c \
          bash "$DEVENV_ROOT/scripts/build-csyn-ros2"
      '';
      release.build = strict ''
        if [ ! -f "$DEVENV_ROOT/src/csyn_ros2_bridge/flake.nix" ]; then
          echo "ROS 2 support is not installed; skipping optional release build."
          exit 0
        fi
        cd "$DEVENV_ROOT/src/csyn_ros2_bridge"
        git submodule update --init --recursive
        ${nix} run "${releaseFlake "src/csyn_ros2_bridge"}#ci"
      '';
      local.test = strict ''
        if [ ! -f "$DEVENV_ROOT/src/csyn_ros2_bridge/flake.nix" ]; then
          echo "ROS 2 support is not installed; skipping optional tests."
          exit 0
        fi
        test -f "$COGNIPILOT_BUILD_ROOT/csyn_ros2_bridge/log/latest_test/events.log"
        ${nix} develop "${flake "src/csyn_ros2_bridge"}" -c \
          colcon test-result \
            --test-result-base "$COGNIPILOT_BUILD_ROOT/csyn_ros2_bridge/build" \
            --verbose
      '';
      release.test = strict ''
        if [ ! -f "$DEVENV_ROOT/src/csyn_ros2_bridge/flake.nix" ]; then
          echo "ROS 2 support is not installed; skipping optional release tests."
          exit 0
        fi
        cd "$DEVENV_ROOT/src/csyn_ros2_bridge"
        git submodule update --init --recursive
        ${nix} run "${releaseFlake "src/csyn_ros2_bridge"}#ci"
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
      devenvProfile = "fastdyn-toolchain";
      local = {
        build = strict ''
          if [ ! -f "$DEVENV_ROOT/src/FastDyn/setup.sh" ]; then
            echo "FastDyn is optional and is not installed."
            echo "Run 'ws sync FastDyn' or copy a deployed tree to:"
            echo "  $DEVENV_ROOT/src/FastDyn"
            exit 0
          fi
          cd "$DEVENV_ROOT/src/FastDyn"
          mkdir -p "$COGNIPILOT_BUILD_ROOT/fastdyn"
          source ./setup.sh \
            --python "''${COGNIPILOT_FASTDYN_PYTHON:-python3}" \
            --venv "$COGNIPILOT_BUILD_ROOT/fastdyn/venv" \
            --qemu-root "$COGNIPILOT_BUILD_ROOT/fastdyn/qemu" \
            --build-qemu --skip-optifuzz --skip-qemu-workspace
        '';
        buildOutputs = [
          "build:fastdyn/venv/bin/python"
          "build:fastdyn/qemu/build/qemu-system-arm"
        ];
        test = strict ''
          if [ ! -x "$COGNIPILOT_BUILD_ROOT/fastdyn/venv/bin/python" ]; then
            echo "FastDyn unavailable; skipping optional tests."
            exit 0
          fi
          cd "$DEVENV_ROOT/src/FastDyn"
          "$COGNIPILOT_BUILD_ROOT/fastdyn/venv/bin/python" -m pytest tests/unit
        '';
      };
      release.build = strict ''
        if [ ! -f "$DEVENV_ROOT/src/FastDyn/setup.sh" ]; then
          echo "FastDyn unavailable; skipping optional release build."
          echo "Copy a deployed tree to $DEVENV_ROOT/src/FastDyn to enable it."
          exit 0
        fi
        cd "$DEVENV_ROOT/src/FastDyn"
        mkdir -p "$COGNIPILOT_BUILD_ROOT/release/fastdyn"
        source ./setup.sh \
          --python "''${COGNIPILOT_FASTDYN_PYTHON:-python3}" \
          --venv "$COGNIPILOT_BUILD_ROOT/release/fastdyn/venv" \
          --qemu-root "$COGNIPILOT_BUILD_ROOT/release/fastdyn/qemu" \
          --build-qemu --skip-optifuzz --skip-qemu-workspace
      '';
      release.test = strict ''
        if [ ! -x "$COGNIPILOT_BUILD_ROOT/release/fastdyn/venv/bin/python" ]; then
          echo "FastDyn unavailable; skipping optional release tests."
          exit 0
        fi
        cd "$DEVENV_ROOT/src/FastDyn"
        "$COGNIPILOT_BUILD_ROOT/release/fastdyn/venv/bin/python" -m pytest tests/unit
      '';
    };
  };
}
