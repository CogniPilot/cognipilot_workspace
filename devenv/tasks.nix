{ config, lib, ... }:

let
  root = config.git.root;
  source = repository: "${root}/src/${repository}";
  task = repository: description: exec: {
    inherit description exec;
    cwd = source repository;
  };
  synapseRust = "${source "synapse_fbs"}/target/xtask/packages/rust";
  synapseC = "${source "synapse_fbs"}/target/xtask/artifacts-work/synapse_fbs-c";
  synapseJavascript = "${source "synapse_fbs"}/target/xtask/packages/js";
  rumocaJavascript = "${source "rumoca"}/packages/rumoca/dist/dev-full-web";
  resultRoot = "${root}/.devenv/state/results";
  cubs2West = "${root}/.devenv/state/west/cubs2";
  rdd2West = "${root}/.devenv/state/west/rdd2";
  cubs2NativeTwisterEnv = {
    WEST_TOPDIR = cubs2West;
    ZEPHYR_BASE = "${cubs2West}/zephyr";
    ZEPHYR_TOOLCHAIN_VARIANT = "host";
  };
  editableZephyrModules = lib.concatStringsSep ";" [
    (source "cerebri_modules")
    (source "zros")
    (source "csyn")
  ];
  sourceRepositories = [
    {
      name = "synapse_fbs";
      url = "https://github.com/CogniPilot/synapse_fbs.git";
    }
    {
      name = "cerebri_cubs2";
      url = "https://github.com/CogniPilot/cerebri_cubs2.git";
    }
    {
      name = "electrode_web";
      url = "https://github.com/CogniPilot/electrode_web.git";
    }
    {
      name = "rumoca";
      url = "https://github.com/CogniPilot/rumoca.git";
    }
    {
      name = "modelica_models";
      url = "https://github.com/CogniPilot/modelica_models.git";
    }
    {
      name = "csyn";
      url = "https://github.com/CogniPilot/csyn.git";
    }
    {
      name = "cerebri_modules";
      url = "https://github.com/CogniPilot/cerebri_modules.git";
    }
    {
      name = "zros";
      url = "https://github.com/CogniPilot/zros.git";
    }
    {
      name = "zros_drivers";
      url = "https://github.com/CogniPilot/zros_drivers.git";
    }
    {
      name = "qualisys_rust_sdk";
      url = "https://github.com/CogniPilot/qualisys_rust_sdk.git";
    }
    {
      name = "synapse_qualisys_bridge";
      url = "https://github.com/CogniPilot/synapse_qualisys_bridge.git";
    }
    {
      name = "synapse_ppm_bridge";
      url = "https://github.com/CogniPilot/synapse_ppm_bridge.git";
    }
    {
      name = "cerebri_rdd2";
      url = "https://github.com/CogniPilot/cerebri_rdd2.git";
    }
    {
      name = "csyn_ros2_bridge";
      url = "https://github.com/CogniPilot/csyn_ros2_bridge.git";
      submodules = true;
    }
    {
      name = "FastDyn";
      url = "https://github.com/jgoppert/FastDyn.git";
    }
  ];
  sourceSyncTasks = builtins.listToAttrs (
    map (
      repository:
      lib.nameValuePair "sources:sync:${repository.name}" {
        description = "Clone ${repository.name}, or fetch its latest origin/main.";
        cwd = root;
        after = [ "sources:prepare" ];
        exec = ''
          if test -d src/${repository.name}/.git; then
            git -C src/${repository.name} fetch --prune origin main
          else
            git clone --branch main --single-branch ${repository.url} src/${repository.name}
          fi
          ${lib.optionalString (repository.submodules or false) ''
            git -C src/${repository.name} submodule sync --recursive
            git -C src/${repository.name} submodule update --init --recursive
          ''}
        '';
      }
    ) sourceRepositories
  );
  sourceStatusTasks = builtins.listToAttrs (
    map (
      repository:
      lib.nameValuePair "sources:status:${repository.name}" {
        description = "Show ${repository.name} Git status.";
        cwd = source repository.name;
        exec = "git status --short --branch";
      }
    ) sourceRepositories
  );
  cacheFlakeOutputs = [
    {
      repository = "cerebri_cubs2";
      attribute = "default";
    }
    {
      repository = "cerebri_rdd2";
      attribute = "default";
    }
    {
      repository = "csyn";
      attribute = "default";
    }
    {
      repository = "csyn_ros2_bridge";
      attribute = "default";
    }
    {
      repository = "qualisys_rust_sdk";
      attribute = "default";
    }
    {
      repository = "rumoca";
      attribute = "rumoca";
    }
    {
      repository = "rumoca";
      attribute = "rumoca-python-env";
    }
  ];
  cacheFlakeTasks = builtins.listToAttrs (
    map (
      output:
      lib.nameValuePair "cache:${output.repository}:${output.attribute}" (
        task output.repository "Realize ${output.repository}#${output.attribute} for Cachix." ''
          nix build --no-link .#${output.attribute}
        ''
      )
    ) cacheFlakeOutputs
  );
in
{
  tasks =
    sourceSyncTasks
    // sourceStatusTasks
    // cacheFlakeTasks
    // {
      "sources:prepare" = {
        description = "Create the editable source checkout directory.";
        cwd = root;
        status = "test -d src";
        exec = "mkdir -p src";
      };

      "sources:sync" = {
        description = "Clone missing repositories and fetch origin/main for existing checkouts.";
        after = map (repository: "sources:sync:${repository.name}") sourceRepositories;
      };

      "sources:status" = {
        description = "Show Git status for every editable top-level repository.";
        after = map (repository: "sources:status:${repository.name}") sourceRepositories;
      };

      "results:prepare" = {
        description = "Create the workspace-local Nix result directory.";
        cwd = root;
        status = "test -d .devenv/state/results";
        exec = "mkdir -p .devenv/state/results";
      };

      "workspace:validate" = {
        description = "Validate the Devenv configuration.";
        cwd = root;
        before = [ "devenv:enterTest" ];
        exec = ''
          nix-instantiate --parse devenv.nix >/dev/null
          nix-instantiate --parse devenv/tasks.nix >/dev/null
          nix-instantiate --parse devenv/profiles.nix >/dev/null
          for path in manifest modules models zephyr .west src/modules src/models; do
            if test -e "$path"; then
              printf 'unexpected shared workspace path: %s\n' "$path" >&2
              exit 1
            fi
          done
        '';
      };

      "synapse-fbs:build" = task "synapse_fbs" "Generate all local Synapse language packages." ''
        nix develop --command \
          cargo run --locked --manifest-path xtask/Cargo.toml -- build --release-name local
      '';

      "synapse-fbs:test" =
        (task "synapse_fbs" "Run Synapse package checks." ''
          nix develop --command \
            cargo run --locked --manifest-path xtask/Cargo.toml -- check
        '')
        // {
          after = [ "synapse-fbs:build" ];
        };

      "rumoca:compiler" =
        (task "rumoca" "Build the local Rumoca compiler." ''
          nix build .#rumoca --out-link ${resultRoot}/rumoca
        '')
        // {
          after = [ "results:prepare" ];
        };

      "rumoca:python" =
        (task "rumoca" "Build the local Rumoca Python environment." ''
          nix build .#rumoca-python-env --out-link ${resultRoot}/rumoca-python
        '')
        // {
          after = [ "results:prepare" ];
        };

      "rumoca:javascript" = task "rumoca" "Build the local Rumoca JavaScript package." ''
        npm --prefix packages/rumoca run build:dev:full-web
      '';

      "rumoca:test" =
        (task "rumoca" "Run the Rumoca flake checks." ''
          nix flake check
        '')
        // {
          after = [
            "rumoca:compiler"
            "rumoca:python"
            "rumoca:javascript"
          ];
        };

      "modelica-models:build" =
        (task "modelica_models" "Build the Modelica model checker with the editable Rumoca source." ''
          nix build --override-input rumoca "git+file://${source "rumoca"}" .#default \
            --out-link ${resultRoot}/modelica-models
        '')
        // {
          after = [ "rumoca:compiler" ];
        };

      "modelica-models:test" =
        (task "modelica_models" "Run the Modelica checks with the editable Rumoca source." ''
          nix run --override-input rumoca "git+file://${source "rumoca"}" .#default
        '')
        // {
          after = [ "modelica-models:build" ];
        };

      "csyn:build" =
        (task "csyn" "Build CSyn against the generated local Synapse Rust package." ''
          cargo build --locked --manifest-path rust/Cargo.toml \
            --config "paths=['${synapseRust}']"
        '')
        // {
          after = [ "synapse-fbs:build" ];
        };

      "csyn:test" =
        (task "csyn" "Test CSyn against the generated local Synapse Rust package." ''
          cargo test --locked --manifest-path rust/Cargo.toml \
            --config "paths=['${synapseRust}']"
        '')
        // {
          after = [ "csyn:build" ];
        };

      "electrode-web:npm-install" =
        task "electrode_web" "Install the locked Electrode JavaScript workspace."
          ''
            npm ci
          '';

      "electrode-web:bind-synapse" =
        (task "electrode_web" "Bind the generated local Synapse JavaScript package." ''
          npm install --no-save --package-lock=false "${synapseJavascript}"
        '')
        // {
          after = [
            "electrode-web:npm-install"
            "synapse-fbs:build"
          ];
        };

      "electrode-web:bind-rumoca" =
        (task "electrode_web" "Bind the generated local Rumoca JavaScript package." ''
          npm install --workspace apps/web --no-save --package-lock=false "${rumocaJavascript}"
        '')
        // {
          after = [
            "electrode-web:bind-synapse"
            "rumoca:javascript"
          ];
        };

      "electrode-web:cargo-build" =
        (task "electrode_web" "Build the Electrode Rust workspace against local Synapse." ''
          cargo build --locked --workspace \
            --config "paths=['${synapseRust}']"
        '')
        // {
          after = [
            "electrode-web:npm-install"
            "synapse-fbs:build"
          ];
        };

      "electrode-web:npm-build" =
        (task "electrode_web" "Build the Electrode web application." ''
          npm run build
        '')
        // {
          after = [ "electrode-web:bind-rumoca" ];
        };

      "electrode-web:build" = {
        description = "Build the complete Electrode ground station.";
        after = [
          "electrode-web:cargo-build"
          "electrode-web:npm-build"
        ];
      };

      "electrode-web:test" =
        (task "electrode_web" "Run the Electrode Rust and JavaScript tests." ''
          npm run ci
          cargo test --locked --workspace \
            --config "paths=['${synapseRust}']"
        '')
        // {
          after = [ "electrode-web:build" ];
        };

      "cubs2:west-update" =
        (task "cerebri_cubs2" "Initialize or update the isolated CUBS2 West workspace." ''
          nix run .#west-update -- --name-cache ${root}/src \
            zephyr cmsis cmsis_6 hal_nxp zenoh-pico zephyr_boards
        '')
        // {
          env = {
            CUBS2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
            CUBS2_CSYN_ROOT = source "csyn";
            CUBS2_MODELICA_ROOT = source "modelica_models";
            CUBS2_WEST_WORKSPACE = cubs2West;
            CUBS2_ZROS_ROOT = source "zros";
          };
        };

      "cubs2:build-native-64" =
        (task "cerebri_cubs2" "Build the 64-bit CUBS2 native simulator against local generated packages." ''
          nix run .#build-native-sim-64 -- -p auto -- \
            "-DCUBS2_CEREBRI_MODULES_ROOT=${source "cerebri_modules"}" \
            "-DCUBS2_ZROS_ROOT=${source "zros"}" \
            "-DCUBS2_CSYN_ROOT=${source "csyn"}" \
            "-DZEPHYR_EXTRA_MODULES=${editableZephyrModules}" \
            -DZEPHYR_TOOLCHAIN_VARIANT=host \
            "-DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C=${synapseC}"
        '')
        // {
          after = [
            "cubs2:west-update"
            "rumoca:python"
            "synapse-fbs:build"
          ];
          env = {
            CUBS2_MODELICA_ROOT = source "modelica_models";
            CUBS2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
            CUBS2_CSYN_ROOT = source "csyn";
            CUBS2_RUMOCA_PYTHON = "${resultRoot}/rumoca-python/bin/python";
            CUBS2_WEST_WORKSPACE = cubs2West;
            CUBS2_ZROS_ROOT = source "zros";
            ZEPHYR_TOOLCHAIN_VARIANT = "host";
          };
        };

      "cubs2:build-native-32" =
        (task "cerebri_cubs2" "Build the 32-bit CUBS2 native simulator against local generated packages." ''
          nix run .#build-native-sim -- -p auto -- \
            "-DCUBS2_CEREBRI_MODULES_ROOT=${source "cerebri_modules"}" \
            "-DCUBS2_ZROS_ROOT=${source "zros"}" \
            "-DCUBS2_CSYN_ROOT=${source "csyn"}" \
            "-DZEPHYR_EXTRA_MODULES=${editableZephyrModules}" \
            -DZEPHYR_TOOLCHAIN_VARIANT=host \
            "-DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C=${synapseC}"
        '')
        // {
          after = [
            "cubs2:west-update"
            "rumoca:python"
            "synapse-fbs:build"
          ];
          env = {
            CUBS2_MODELICA_ROOT = source "modelica_models";
            CUBS2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
            CUBS2_CSYN_ROOT = source "csyn";
            CUBS2_RUMOCA_PYTHON = "${resultRoot}/rumoca-python/bin/python";
            CUBS2_WEST_WORKSPACE = cubs2West;
            CUBS2_ZROS_ROOT = source "zros";
            ZEPHYR_TOOLCHAIN_VARIANT = "host";
          };
        };

      "cubs2:test" =
        (task "cerebri_cubs2" "Run the project-owned CUBS2 native simulator SIL tests." ''
          nix run .#native-sim-64-sil-test
        '')
        // {
          after = [ "cubs2:build-native-64" ];
          env = {
            CUBS2_MODELICA_ROOT = source "modelica_models";
            CUBS2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
            CUBS2_CSYN_ROOT = source "csyn";
            CUBS2_RUMOCA_PYTHON = "${resultRoot}/rumoca-python/bin/python";
            CUBS2_SYNAPSE_C_ROOT = synapseC;
            CUBS2_WEST_WORKSPACE = cubs2West;
            CUBS2_ZROS_ROOT = source "zros";
            ZEPHYR_TOOLCHAIN_VARIANT = "host";
          };
        };

      "cubs2:build-hardware" =
        (task "cerebri_cubs2" "Build CUBS2 firmware for the default hardware target." ''
          nix run .#build -- -p auto -- \
            "-DCUBS2_CEREBRI_MODULES_ROOT=${source "cerebri_modules"}" \
            "-DCUBS2_ZROS_ROOT=${source "zros"}" \
            "-DCUBS2_CSYN_ROOT=${source "csyn"}" \
            "-DZEPHYR_EXTRA_MODULES=${editableZephyrModules}" \
            -DZEPHYR_TOOLCHAIN_VARIANT=gnuarmemb \
            "-DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C=${synapseC}"
        '')
        // {
          after = [
            "cubs2:west-update"
            "rumoca:python"
            "synapse-fbs:build"
          ];
          env = {
            CUBS2_MODELICA_ROOT = source "modelica_models";
            CUBS2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
            CUBS2_CSYN_ROOT = source "csyn";
            CUBS2_RUMOCA_PYTHON = "${resultRoot}/rumoca-python/bin/python";
            CUBS2_WEST_WORKSPACE = cubs2West;
            CUBS2_ZROS_ROOT = source "zros";
            ZEPHYR_TOOLCHAIN_VARIANT = "gnuarmemb";
          };
        };

      "cubs2:flash" =
        (task "cerebri_cubs2" "Flash the previously built CUBS2 firmware." ''
          nix run .#flash
        '')
        // {
          after = [ "cubs2:build-hardware" ];
          env = {
            CUBS2_MODELICA_ROOT = source "modelica_models";
            CUBS2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
            CUBS2_CSYN_ROOT = source "csyn";
            CUBS2_WEST_WORKSPACE = cubs2West;
            CUBS2_ZROS_ROOT = source "zros";
            ZEPHYR_TOOLCHAIN_VARIANT = "gnuarmemb";
          };
        };

      "rdd2:west-update" =
        (task "cerebri_rdd2" "Initialize or update the isolated RDD2 West workspace." ''
          nix run .#west-update -- --name-cache ${root}/src \
            zephyr cmsis cmsis-dsp cmsis_6 fatfs hal_nxp zenoh-pico zephyr_boards
        '')
        // {
          env = {
            RDD2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
            RDD2_CSYN_ROOT = source "csyn";
            RDD2_WEST_WORKSPACE = rdd2West;
            RDD2_ZROS_ROOT = source "zros";
          };
        };

      "rdd2:build" =
        (task "cerebri_rdd2" "Build the RDD2 native simulator against local generated packages." ''
          nix run .#build-native-sim -- -p auto -- \
            -DRDD2_RUMOCA_VERSION=workspace \
            "-DRDD2_RUMOCA_EXECUTABLE=${resultRoot}/rumoca/bin/rumoca" \
            "-DRDD2_CEREBRI_MODULES_ROOT=${source "cerebri_modules"}" \
            "-DRDD2_ZROS_ROOT=${source "zros"}" \
            "-DRDD2_CSYN_ROOT=${source "csyn"}" \
            "-DZEPHYR_EXTRA_MODULES=${editableZephyrModules}" \
            -DZEPHYR_TOOLCHAIN_VARIANT=host \
            "-DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C=${synapseC}"
        '')
        // {
          after = [
            "rdd2:west-update"
            "rumoca:compiler"
            "synapse-fbs:build"
          ];
          env = {
            RDD2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
            RDD2_CSYN_ROOT = source "csyn";
            RDD2_WEST_WORKSPACE = rdd2West;
            RDD2_ZROS_ROOT = source "zros";
            ZEPHYR_TOOLCHAIN_VARIANT = "host";
          };
        };

      "rdd2:build-hardware" =
        (task "cerebri_rdd2" "Build RDD2 firmware for the default hardware target." ''
          nix run .#build -- -p auto -- \
            -DRDD2_RUMOCA_VERSION=workspace \
            "-DRDD2_RUMOCA_EXECUTABLE=${resultRoot}/rumoca/bin/rumoca" \
            "-DRDD2_CEREBRI_MODULES_ROOT=${source "cerebri_modules"}" \
            "-DRDD2_ZROS_ROOT=${source "zros"}" \
            "-DRDD2_CSYN_ROOT=${source "csyn"}" \
            "-DZEPHYR_EXTRA_MODULES=${editableZephyrModules}" \
            -DZEPHYR_TOOLCHAIN_VARIANT=gnuarmemb \
            "-DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C=${synapseC}"
        '')
        // {
          after = [
            "rdd2:west-update"
            "rumoca:compiler"
            "synapse-fbs:build"
          ];
          env = {
            RDD2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
            RDD2_CSYN_ROOT = source "csyn";
            RDD2_WEST_WORKSPACE = rdd2West;
            RDD2_ZROS_ROOT = source "zros";
            ZEPHYR_TOOLCHAIN_VARIANT = "gnuarmemb";
          };
        };

      "rdd2:flash" =
        (task "cerebri_rdd2" "Flash the previously built RDD2 firmware." ''
          nix run .#flash
        '')
        // {
          after = [ "rdd2:build-hardware" ];
          env = {
            RDD2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
            RDD2_CSYN_ROOT = source "csyn";
            RDD2_WEST_WORKSPACE = rdd2West;
            RDD2_ZROS_ROOT = source "zros";
            ZEPHYR_TOOLCHAIN_VARIANT = "gnuarmemb";
          };
        };

      "cerebri-modules:build" = {
        description = "Build the editable Cerebri module tests in the CUBS2 West workspace.";
        cwd = cubs2West;
        after = [ "cubs2:west-update" ];
        exec = ''
          west twister -T ${source "cerebri_modules"}/tests \
            -p native_sim/native/64 --force-platform \
            --extra-args "ZEPHYR_EXTRA_MODULES=${editableZephyrModules}" \
            --outdir ${source "cerebri_modules"}/build/twister/build \
            --no-clean --build-only
        '';
        env = cubs2NativeTwisterEnv;
      };

      "cerebri-modules:test" = {
        description = "Run the editable Cerebri module tests in the CUBS2 West workspace.";
        cwd = cubs2West;
        after = [ "cerebri-modules:build" ];
        exec = ''
          west twister -T ${source "cerebri_modules"}/tests \
            -p native_sim/native/64 --force-platform \
            --extra-args "ZEPHYR_EXTRA_MODULES=${editableZephyrModules}" \
            --outdir ${source "cerebri_modules"}/build/twister/test \
            --no-clean
        '';
        env = cubs2NativeTwisterEnv;
      };

      "zros:build" = {
        description = "Build the editable ZROS tests in the CUBS2 West workspace.";
        cwd = cubs2West;
        after = [ "cubs2:west-update" ];
        exec = ''
          west twister -T ${source "zros"}/tests \
            -p native_sim/native/64 --force-platform \
            --extra-args "ZEPHYR_EXTRA_MODULES=${editableZephyrModules}" \
            --outdir ${source "zros"}/build/twister/build \
            --no-clean --build-only
        '';
        env = cubs2NativeTwisterEnv;
      };

      "zros:test" = {
        description = "Run the editable ZROS tests in the CUBS2 West workspace.";
        cwd = cubs2West;
        after = [ "zros:build" ];
        exec = ''
          west twister -T ${source "zros"}/tests \
            -p native_sim/native/64 --force-platform \
            --extra-args "ZEPHYR_EXTRA_MODULES=${editableZephyrModules}" \
            --outdir ${source "zros"}/build/twister/test \
            --no-clean
        '';
        env = cubs2NativeTwisterEnv;
      };

      "csyn:qualification" = {
        description = "Run editable CSyn Zephyr tests in the CUBS2 West workspace.";
        cwd = cubs2West;
        after = [
          "cubs2:west-update"
          "csyn:test"
        ];
        exec = ''
          west twister -T ${source "csyn"}/zephyr/tests \
            -p native_sim/native/64 --force-platform \
            --extra-args "ZEPHYR_EXTRA_MODULES=${editableZephyrModules}" \
            --extra-args "FETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C=${synapseC}" \
            --outdir ${source "csyn"}/build/twister/qualification \
            --no-clean
        '';
        env = cubs2NativeTwisterEnv;
      };

      "qualisys-sdk:build" = task "qualisys_rust_sdk" "Build the Qualisys Rust SDK and simulator." ''
        cargo build --locked --all-targets
      '';

      "qualisys-sdk:test" =
        (task "qualisys_rust_sdk" "Test the Qualisys Rust SDK." ''
          cargo test --locked --all-targets
        '')
        // {
          after = [ "qualisys-sdk:build" ];
        };

      "qualisys-bridge:build" =
        (task "synapse_qualisys_bridge" "Build the Qualisys bridge against local Synapse." ''
          cargo build --locked --bin synapse-qualisys-bridge \
            --config "paths=['${synapseRust}']"
        '')
        // {
          after = [ "synapse-fbs:build" ];
        };

      "qualisys-bridge:test" =
        (task "synapse_qualisys_bridge" "Test the Qualisys bridge against local Synapse." ''
          cargo test --locked \
            --config "paths=['${synapseRust}']"
        '')
        // {
          after = [ "qualisys-bridge:build" ];
        };

      "qualisys-bridge:e2e" =
        (task "synapse_qualisys_bridge" "Run the project-owned Qualisys bridge browser tests." ''
          playwright test --config tests/e2e/playwright.config.js
        '')
        // {
          after = [
            "qualisys-bridge:test"
            "qualisys-sdk:build"
          ];
          env = {
            BRIDGE_BIN = "target/debug/synapse-qualisys-bridge";
            QUALISYS_SIM_BIN = "${source "qualisys_rust_sdk"}/target/debug/qualisys-sim";
          };
        };

      "ppm:build" =
        (task "synapse_ppm_bridge" "Build the PPM bridge against local Synapse." ''
          cargo build --locked --config "paths=['${synapseRust}']"
        '')
        // {
          after = [ "synapse-fbs:build" ];
        };

      "ppm:test" =
        (task "synapse_ppm_bridge" "Test the PPM bridge." ''
          cargo test --locked --config "paths=['${synapseRust}']"
        '')
        // {
          after = [ "ppm:build" ];
        };

      "ros2:test" =
        (task "csyn_ros2_bridge" "Run the project-owned colcon/ROS 2 CI application." ''
          nix run .#ci
        '')
        // {
          after = [ "synapse-fbs:build" ];
          env = {
            CSYN_SYNAPSE_FBS_DIR = "${source "synapse_fbs"}/fbs";
            CSYN_SYNAPSE_FBS_RUST_DIR = synapseRust;
          };
        };

      "fastdyn:build" = task "FastDyn" "Create FastDyn's project-owned Python environment." ''
        python -m venv build/venv
        PATH="$PWD/build/venv/bin:$PATH" ./setup.sh
      '';

      "fastdyn:test" =
        (task "FastDyn" "Run the FastDyn unit tests." ''
          build/venv/bin/python -m pytest tests/unit
        '')
        // {
          after = [ "fastdyn:build" ];
        };

      "cache:all" = {
        description = "Realize the bounded project flake outputs uploaded by dedicated cache CI.";
        after = (map (output: "cache:${output.repository}:${output.attribute}") cacheFlakeOutputs) ++ [
          "modelica-models:build"
        ];
      };

      "release:compliance:cargo" =
        (task "synapse_fbs" "Require every Rust consumer to use the generated Synapse version." ''
          expected="$(cargo metadata --no-deps --format-version 1 \
            --manifest-path target/xtask/packages/rust/Cargo.toml | jq -r '.packages[0].version')"
          failed=0
          for manifest in \
            ../csyn/rust/Cargo.toml \
            ../csyn_ros2_bridge/csyn_ros2_bridge/Cargo.toml \
            ../electrode_web/Cargo.toml \
            ../synapse_qualisys_bridge/Cargo.toml \
            ../synapse_ppm_bridge/Cargo.toml
          do
            requirement="$(cargo metadata --no-deps --format-version 1 --manifest-path "$manifest" |
              jq -r '[.packages[].dependencies[] | select(.name == "synapse_fbs") | .req] |
                unique | join(",")')"
            if test "$requirement" != "^$expected" && test "$requirement" != "=$expected"; then
              printf '%s requires synapse_fbs %s; workspace requires ^%s or =%s\n' \
                "$manifest" "$requirement" "$expected" "$expected" >&2
              failed=1
            fi
          done
          test "$failed" -eq 0
        '')
        // {
          after = [ "synapse-fbs:build" ];
        };

      "release:compliance:npm" =
        (task "electrode_web" "Require every npm consumer to use the generated Synapse version." ''
          expected="$(cargo metadata --no-deps --format-version 1 \
            --manifest-path ${synapseRust}/Cargo.toml | jq -r '.packages[0].version')"
          test "$(jq -r '.dependencies["@cognipilot/synapse-fbs"]' package.json)" = "^$expected"
          test "$(jq -r '.dependencies["@cognipilot/synapse-fbs"]' packages/electrode-sdk/package.json)" = "^$expected"
        '')
        // {
          after = [ "synapse-fbs:build" ];
        };

      "release:compliance:nix" =
        (task "cerebri_cubs2" "Require the CUBS2 Python package to use the generated Synapse version." ''
          expected="$(cargo metadata --no-deps --format-version 1 \
            --manifest-path ${synapseRust}/Cargo.toml | jq -r '.packages[0].version')"
          test "$(nix eval --raw .#synapse-fbs.version)" = "$expected"
        '')
        // {
          after = [ "synapse-fbs:build" ];
        };

      "release:compliance:cmake" =
        (task "csyn" "Require the standalone CSyn C archive to use the generated Synapse version." ''
          expected="$(cargo metadata --no-deps --format-version 1 \
            --manifest-path ${synapseRust}/Cargo.toml | jq -r '.packages[0].version')"
          grep -F "/v$expected/synapse_fbs-c.tar.gz" zephyr/CMakeLists.txt >/dev/null
        '')
        // {
          after = [ "synapse-fbs:build" ];
        };

      "release:compliance" = {
        description = "Require every package ecosystem to use the generated Synapse version.";
        after = [
          "release:compliance:cargo"
          "release:compliance:cmake"
          "release:compliance:nix"
          "release:compliance:npm"
        ];
      };

      "release:qualify" = {
        description = "Run every bounded release qualification task without publishing.";
        after = [
          "cerebri-modules:test"
          "csyn:qualification"
          "cubs2:test"
          "electrode-web:test"
          "modelica-models:test"
          "ppm:test"
          "qualisys-bridge:e2e"
          "qualisys-sdk:test"
          "rdd2:build"
          "ros2:test"
          "rumoca:test"
          "release:compliance"
          "synapse-fbs:test"
          "zros:test"
        ];
      };

      "release:synapse-fbs" =
        (task "synapse_fbs" "Build every Synapse package-manager and archive artifact." ''
          nix develop --command \
            cargo run --locked --manifest-path xtask/Cargo.toml -- ci --release-name local
        '')
        // {
          after = [
            "release:compliance"
            "synapse-fbs:test"
          ];
        };

      "release:rumoca" =
        (task "rumoca" "Build and pack both Rumoca npm packages without publishing." ''
          nix build --no-link .#rumoca .#rumoca-python-env
          npm --prefix packages/rumoca run build:release:core:pack
          npm --prefix packages/rumoca run build:release:full-web:pack
        '')
        // {
          after = [ "rumoca:test" ];
        };

      "release:modelica-models" = {
        description = "Qualify the Modelica model package and local Rumoca integration.";
        after = [ "modelica-models:test" ];
      };

      "release:electrode-web" = {
        description = "Qualify the Electrode application and Pages payload.";
        after = [ "electrode-web:test" ];
      };

      "release:qualisys-bridge" =
        (task "synapse_qualisys_bridge" "Build the host Qualisys bridge release binary." ''
          cargo build --release --locked --bin synapse-qualisys-bridge \
            --config "paths=['${synapseRust}']"
        '')
        // {
          after = [ "qualisys-bridge:e2e" ];
        };

      "release:firmware" = {
        description = "Build CUBS2 and RDD2 hardware firmware against workspace dependencies.";
        after = [
          "cubs2:build-hardware"
          "rdd2:build-hardware"
        ];
      };

      "release:ppm" =
        (task "synapse_ppm_bridge" "Verify the PPM bridge Cargo release without publishing." ''
          cargo publish --locked --dry-run
        '')
        // {
          after = [ "ppm:test" ];
        };

      "release:csyn" =
        (task "csyn" "Verify the CSyn Cargo release without publishing." ''
          cargo publish --locked --dry-run --manifest-path rust/Cargo.toml
        '')
        // {
          after = [ "csyn:qualification" ];
        };

      "release:qualisys-sdk" =
        (task "qualisys_rust_sdk" "Verify the Qualisys SDK Cargo release without publishing." ''
          cargo publish --locked --dry-run
        '')
        // {
          after = [ "qualisys-sdk:test" ];
        };

      "release:all" = {
        description = "Complete every configured release qualification; this never publishes.";
        after = [
          "release:qualify"
          "release:csyn"
          "release:electrode-web"
          "release:firmware"
          "release:modelica-models"
          "release:ppm"
          "release:qualisys-bridge"
          "release:qualisys-sdk"
          "release:rumoca"
          "release:synapse-fbs"
        ];
      };

      "ci:documented" = {
        description = "Run every terminating build and test workflow documented in the README.";
        after = [
          "cubs2:build-native-32"
          "fastdyn:test"
          "release:all"
        ];
      };
    };
}
