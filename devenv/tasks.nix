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
  fastdynState = "${root}/.devenv/state/fastdyn";
  fastdynQemu = "${fastdynState}/qemu";
  fastdynVenv = "${fastdynState}/venv";
  cubs2West = "${root}/.devenv/state/west/cubs2";
  rdd2West = "${root}/.devenv/state/west/rdd2";
  cubs2NativeTwisterEnv = {
    WEST_TOPDIR = cubs2West;
    ZEPHYR_BASE = "${cubs2West}/zephyr";
    ZEPHYR_TOOLCHAIN_VARIANT = "host";
  };
  cubs2WestEnv = {
    CUBS2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
    CUBS2_CSYN_ROOT = source "csyn";
    CUBS2_MODELICA_MODELS_ROOT = source "modelica_models";
    CUBS2_WEST_WORKSPACE = cubs2West;
    CUBS2_ZROS_ROOT = source "zros";
  };
  cubs2WestUpdate = ''
    nix run .#west-update -- --name-cache ${root}/src \
      zephyr cmsis cmsis_6 hal_nxp zenoh-pico zephyr_boards
  '';
  rdd2WestEnv = {
    RDD2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
    RDD2_CSYN_ROOT = source "csyn";
    RDD2_MODELICA_MODELS_ROOT = source "modelica_models";
    RDD2_WEST_WORKSPACE = rdd2West;
    RDD2_ZROS_ROOT = source "zros";
  };
  rdd2WestUpdate = ''
    nix run .#west-update -- --name-cache ${root}/src \
      zephyr cmsis cmsis-dsp cmsis_6 fatfs hal_nxp zenoh-pico zephyr_boards
  '';
  editableZephyrModules = lib.concatStringsSep ";" [
    (source "cerebri_modules")
    (source "zros")
    (source "csyn")
  ];
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
  allTasks = {
    "sources:checkout" = {
      description = "Clone missing repositories at the exact revisions in cognipilot.repos.";
      cwd = root;
      exec = ''
        mkdir -p src
        vcs import --skip-existing --input cognipilot.repos src
      '';
    };

    "sources:status" = {
      description = "Show Git status for every editable top-level repository.";
      cwd = root;
      exec = "vcs status src";
    };

    "sources:diff" = {
      description = "Show uncommitted diffs across editable top-level repositories.";
      cwd = root;
      exec = "vcs diff src";
    };

    "sources:lock" = {
      description = "Record the exact reachable revisions of the current repositories.";
      cwd = root;
      exec = "vcs export --exact --lint src > cognipilot.repos";
    };

    "sources:verify" = {
      description = "Verify that editable repositories match the exact cognipilot.repos snapshot.";
      cwd = root;
      before = [ "devenv:enterTest" ];
      exec = ''
        vcs validate < cognipilot.repos
        vcs export --exact --lint src > "$DEVENV_STATE/cognipilot.repos.actual"
        diff -u cognipilot.repos "$DEVENV_STATE/cognipilot.repos.actual"
      '';
    };

  }
  // cacheFlakeTasks
  // {
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
      nix develop --command npm --prefix packages/rumoca run build:dev:full-web
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

    "modelica-models:cubs2:qualify" =
      (task "modelica_models" "Run the CUBS2 model-level qualification missions." ''
        MODELICA_MODELS_ROOT="$PWD" \
          nix run --override-input rumoca "git+file://${source "rumoca"}" \
            .#cubs2-qualification -- "$@"
      '')
      // {
        after = [
          "modelica-models:build"
          "rumoca:python"
        ];
      };

    "modelica-models:rdd2:qualify" =
      (task "modelica_models" "Run the RDD2 model-level qualification mission." ''
        MODELICA_MODELS_ROOT="$PWD" \
          nix run --override-input rumoca "git+file://${source "rumoca"}" \
            .#rdd2-qualification -- "$@"
      '')
      // {
        after = [
          "modelica-models:build"
          "rumoca:python"
        ];
      };

    "modelica-models:cubs2:export-controller" =
      (task "modelica_models" "Export the CUBS2 controller as eFMI Production Code." ''
        MODELICA_MODELS_ROOT="$PWD" \
          nix run --override-input rumoca "git+file://${source "rumoca"}" \
            .#cubs2-export-controller
      '')
      // {
        after = [ "rumoca:compiler" ];
      };

    "modelica-models:cubs2:export-plant" =
      (task "modelica_models" "Export the event-aware CUBS2 FMI 3 plant." ''
        MODELICA_MODELS_ROOT="$PWD" \
          nix run --override-input rumoca "git+file://${source "rumoca"}" \
            .#cubs2-export-plant
      '')
      // {
        after = [ "rumoca:compiler" ];
      };

    "modelica-models:rdd2:export-controller" =
      (task "modelica_models" "Export the RDD2 controller as eFMI Production Code." ''
        MODELICA_MODELS_ROOT="$PWD" \
          nix run --override-input rumoca "git+file://${source "rumoca"}" \
            .#rdd2-export-controller
      '')
      // {
        after = [ "rumoca:compiler" ];
      };

    "modelica-models:rdd2:export-estimator" =
      (task "modelica_models" "Export the shared RDD2 attitude estimator as eFMI Production Code." ''
        MODELICA_MODELS_ROOT="$PWD" \
          nix run --override-input rumoca "git+file://${source "rumoca"}" \
            .#rdd2-export-estimator
      '')
      // {
        after = [ "rumoca:compiler" ];
      };

    "modelica-models:rdd2:export-plant" =
      (task "modelica_models" "Export the event-aware RDD2 FMI 3 plant." ''
        MODELICA_MODELS_ROOT="$PWD" \
          nix run --override-input rumoca "git+file://${source "rumoca"}" \
            .#rdd2-export-plant
      '')
      // {
        after = [ "rumoca:compiler" ];
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
      (task "electrode_web" "Install the locked Electrode JavaScript workspace." ''
        npm ci
        nix hash file package-lock.json > node_modules/.cognipilot-package-lock
      '')
      // {
        status = ''
          test -d node_modules || exit 1
          test -f node_modules/.cognipilot-package-lock || exit 1
          nix hash file package-lock.json | cmp -s - node_modules/.cognipilot-package-lock || exit 1
          npm ls --depth=0 >/dev/null 2>&1
        '';
      };

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

    "cubs2:workspace:update" =
      (task "cerebri_cubs2" "Update the isolated CUBS2 West workspace." cubs2WestUpdate)
      // {
        env = cubs2WestEnv;
      };

    "cubs2:workspace:ready" =
      (task "cerebri_cubs2" "Initialize the isolated CUBS2 West workspace when missing." cubs2WestUpdate)
      // {
        env = cubs2WestEnv;
        status = ''
          test -d ${cubs2West}/.west &&
          test -d ${cubs2West}/zephyr &&
          test -d ${cubs2West}/modules/hal/cmsis &&
          test -d ${cubs2West}/modules/hal/cmsis_6 &&
          test -d ${cubs2West}/modules/hal/nxp &&
          test -d ${cubs2West}/modules/lib/zenoh-pico &&
          test -d ${cubs2West}/modules/lib/zephyr_boards
        '';
      };

    "cubs2:simulation:sil:build" =
      (task "cerebri_cubs2" "Build the 64-bit CUBS2 native simulator against local generated packages." ''
        nix run .#build-native-sim-64 -- -p auto -- \
          "-DCUBS2_CEREBRI_MODULES_ROOT=${source "cerebri_modules"}" \
          "-DCUBS2_ZROS_ROOT=${source "zros"}" \
          "-DCUBS2_CSYN_ROOT=${source "csyn"}" \
          "-DCUBS2_MODELICA_MODELS_ROOT=${source "modelica_models"}" \
          "-DZEPHYR_EXTRA_MODULES=${editableZephyrModules}" \
          -DZEPHYR_TOOLCHAIN_VARIANT=host \
          "-DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C=${synapseC}"
      '')
      // {
        after = [
          "cubs2:workspace:ready"
          "rumoca:python"
          "synapse-fbs:build"
        ];
        env = {
          CUBS2_MODELICA_MODELS_ROOT = source "modelica_models";
          CUBS2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
          CUBS2_CSYN_ROOT = source "csyn";
          CUBS2_RUMOCA_PYTHON = "${resultRoot}/rumoca-python/bin/python";
          CUBS2_WEST_WORKSPACE = cubs2West;
          CUBS2_ZROS_ROOT = source "zros";
          ZEPHYR_TOOLCHAIN_VARIANT = "host";
        };
      };

    "cubs2:simulation:sil:build-32" =
      (task "cerebri_cubs2" "Build the 32-bit CUBS2 native simulator against local generated packages." ''
        nix run .#build-native-sim -- -p auto -- \
          "-DCUBS2_CEREBRI_MODULES_ROOT=${source "cerebri_modules"}" \
          "-DCUBS2_ZROS_ROOT=${source "zros"}" \
          "-DCUBS2_CSYN_ROOT=${source "csyn"}" \
          "-DCUBS2_MODELICA_MODELS_ROOT=${source "modelica_models"}" \
          "-DZEPHYR_EXTRA_MODULES=${editableZephyrModules}" \
          -DZEPHYR_TOOLCHAIN_VARIANT=host \
          "-DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C=${synapseC}"
      '')
      // {
        after = [
          "cubs2:workspace:ready"
          "rumoca:python"
          "synapse-fbs:build"
        ];
        env = {
          CUBS2_MODELICA_MODELS_ROOT = source "modelica_models";
          CUBS2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
          CUBS2_CSYN_ROOT = source "csyn";
          CUBS2_RUMOCA_PYTHON = "${resultRoot}/rumoca-python/bin/python";
          CUBS2_WEST_WORKSPACE = cubs2West;
          CUBS2_ZROS_ROOT = source "zros";
          ZEPHYR_TOOLCHAIN_VARIANT = "host";
        };
      };

    "cubs2:simulation:sil:test" =
      (task "cerebri_cubs2" "Run the project-owned CUBS2 native simulator and Rumoca SIL tests." ''
        if printf '%s' "$DEVENV_TASK_INPUT" | jq -e '.reuse_router == true' >/dev/null; then
          nix run .#native-sim-64-sil-test -- --reuse-router
        else
          nix run .#native-sim-64-sil-test
        fi
      '')
      // {
        after = [ "cubs2:simulation:sil:build" ];
        input.reuse_router = false;
        env = {
          CUBS2_MODELICA_MODELS_ROOT = source "modelica_models";
          CUBS2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
          CUBS2_CSYN_ROOT = source "csyn";
          CUBS2_RUMOCA_PYTHON = "${resultRoot}/rumoca-python/bin/python";
          CUBS2_SYNAPSE_C_ROOT = synapseC;
          CUBS2_WEST_WORKSPACE = cubs2West;
          CUBS2_ZROS_ROOT = source "zros";
          ZEPHYR_TOOLCHAIN_VARIANT = "host";
        };
      };

    "cubs2:simulation:modelica:test" =
      (task "modelica_models" "Run the pure Modelica CUBS2 controller and physics scenarios with Rumoca."
        ''
          MODELICA_MODELS_ROOT="$PWD" \
            nix run --override-input rumoca "git+file://${source "rumoca"}" \
              .#cubs2-qualification
        ''
      )
      // {
        after = [
          "modelica-models:build"
          "rumoca:python"
        ];
      };

    "cubs2:simulation:bil:build" =
      (task "cerebri_cubs2" "Build the CUBS2 hardware binary and FastDyn lockstep bridge for BIL." ''
        fastdyn_config="$PWD/fastdyn/mr_vmu_tropic.toml"
        if ! test -f "$PWD/fastdyn/prj.conf" || ! test -f "$fastdyn_config"; then
          echo 'The CUBS2-owned FastDyn integration is incomplete.' >&2
          exit 1
        fi
        conf_file="$(realpath fastdyn/prj.conf)"
        overlay="$(realpath fastdyn/mr_vmu_tropic.overlay)"
        CUBS2_BUILD_DIR="$PWD/build-mr_vmu_tropic-fastdyn" nix run .#build -- -p auto -- \
          -DCONF_FILE="$conf_file" \
          -DDTC_OVERLAY_FILE="$overlay" \
          "-DCUBS2_CEREBRI_MODULES_ROOT=${source "cerebri_modules"}" \
          "-DCUBS2_ZROS_ROOT=${source "zros"}" \
          "-DCUBS2_CSYN_ROOT=${source "csyn"}" \
          "-DCUBS2_MODELICA_MODELS_ROOT=${source "modelica_models"}" \
          "-DZEPHYR_EXTRA_MODULES=${editableZephyrModules}" \
          -DZEPHYR_TOOLCHAIN_VARIANT=gnuarmemb \
          "-DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C=${synapseC}"
        cargo build --release --locked \
          --manifest-path tools/fastdyn_bridge/Cargo.toml
      '')
      // {
        after = [
          "cubs2:workspace:ready"
          "fastdyn:runtime:build"
          "rumoca:python"
          "synapse-fbs:build"
        ];
        env = cubs2WestEnv // {
          CUBS2_FASTDYN_BUILD_DIR = "${source "cerebri_cubs2"}/build-mr_vmu_tropic-fastdyn";
          CUBS2_MODELICA_MODELS_ROOT = source "modelica_models";
          CUBS2_RUMOCA_PYTHON = "${resultRoot}/rumoca-python/bin/python";
          CUBS2_WORKSPACE_ROOT = cubs2West;
          CEREBRI_CUBS2_ROOT = source "cerebri_cubs2";
          FASTDYN_QEMU_PATH = "${fastdynQemu}/build/qemu-system-arm";
          ZEPHYR_TOOLCHAIN_VARIANT = "gnuarmemb";
        };
      };

    "cubs2:simulation:bil:test" =
      (task "FastDyn" "Run the FastDyn-rehosted CUBS2 binary against its Rumoca-generated FMI3 plant." ''
        export PATH="${fastdynVenv}/bin:$PATH"
        export LD_LIBRARY_PATH="$PWD/out/deps/cjson/install/lib:$PWD/build''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
        exec ${source "cerebri_cubs2"}/fastdyn/run_mission.sh
      '')
      // {
        after = [ "cubs2:simulation:bil:build" ];
        env = {
          CUBS2_FASTDYN_BUILD_DIR = "${source "cerebri_cubs2"}/build-mr_vmu_tropic-fastdyn";
          CUBS2_MODELICA_MODELS_ROOT = source "modelica_models";
          CUBS2_RUMOCA_PYTHON = "${resultRoot}/rumoca-python/bin/python";
          CUBS2_SYNAPSE_C_ROOT = synapseC;
          CUBS2_WORKSPACE_ROOT = cubs2West;
          CEREBRI_CUBS2_ROOT = source "cerebri_cubs2";
          FASTDYN_MONITOR_ELF = "${fastdynQemu}/ws/monitor.elf";
          FASTDYN_ROOT = source "FastDyn";
          FASTDYN_QEMU_PATH = "${fastdynQemu}/build/qemu-system-arm";
        };
      };

    "cubs2:simulation:compare" =
      (task "cerebri_cubs2" "Compare CUBS2 mission logs across all configured execution paths." ''
        exec nix run .#trajectory-compare
      '')
      // {
        after = [
          "cubs2:simulation:bil:test"
          "cubs2:simulation:modelica:test"
          "cubs2:simulation:sil:test"
        ];
        env = cubs2WestEnv;
      };

    "cubs2:firmware:build" =
      (task "cerebri_cubs2" "Build CUBS2 firmware for the default hardware target." ''
        nix run .#build -- -p auto -- \
          "-DCUBS2_CEREBRI_MODULES_ROOT=${source "cerebri_modules"}" \
          "-DCUBS2_ZROS_ROOT=${source "zros"}" \
          "-DCUBS2_CSYN_ROOT=${source "csyn"}" \
          "-DCUBS2_MODELICA_MODELS_ROOT=${source "modelica_models"}" \
          "-DZEPHYR_EXTRA_MODULES=${editableZephyrModules}" \
          -DZEPHYR_TOOLCHAIN_VARIANT=gnuarmemb \
          "-DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C=${synapseC}"
      '')
      // {
        after = [
          "cubs2:workspace:ready"
          "rumoca:python"
          "synapse-fbs:build"
        ];
        env = {
          CUBS2_MODELICA_MODELS_ROOT = source "modelica_models";
          CUBS2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
          CUBS2_CSYN_ROOT = source "csyn";
          CUBS2_RUMOCA_PYTHON = "${resultRoot}/rumoca-python/bin/python";
          CUBS2_WEST_WORKSPACE = cubs2West;
          CUBS2_ZROS_ROOT = source "zros";
          ZEPHYR_TOOLCHAIN_VARIANT = "gnuarmemb";
        };
      };

    "cubs2:firmware:configure" =
      (task "cerebri_cubs2" "Open Zephyr menuconfig for the CUBS2 hardware build." ''
        nix run .#menuconfig
      '')
      // {
        after = [ "cubs2:firmware:build" ];
        env = {
          CUBS2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
          CUBS2_CSYN_ROOT = source "csyn";
          CUBS2_WEST_WORKSPACE = cubs2West;
          CUBS2_ZROS_ROOT = source "zros";
          ZEPHYR_TOOLCHAIN_VARIANT = "gnuarmemb";
        };
      };

    "cubs2:firmware:flash" =
      (task "cerebri_cubs2" "Flash the previously built CUBS2 firmware." ''
        if ! printf '%s' "$DEVENV_TASK_INPUT" | jq -e '.confirm == true' >/dev/null; then
          echo 'refusing to flash without --input confirm=true' >&2
          exit 2
        fi
        nix run .#flash
      '')
      // {
        after = [ "cubs2:firmware:build" ];
        input.confirm = false;
        env = {
          CUBS2_MODELICA_MODELS_ROOT = source "modelica_models";
          CUBS2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
          CUBS2_CSYN_ROOT = source "csyn";
          CUBS2_WEST_WORKSPACE = cubs2West;
          CUBS2_ZROS_ROOT = source "zros";
          ZEPHYR_TOOLCHAIN_VARIANT = "gnuarmemb";
        };
      };

    "rdd2:workspace:update" =
      (task "cerebri_rdd2" "Update the isolated RDD2 West workspace." rdd2WestUpdate)
      // {
        env = rdd2WestEnv;
      };

    "rdd2:workspace:ready" =
      (task "cerebri_rdd2" "Initialize the isolated RDD2 West workspace when missing." rdd2WestUpdate)
      // {
        env = rdd2WestEnv;
        status = ''
          test -d ${rdd2West}/.west &&
          test -d ${rdd2West}/zephyr &&
          test -d ${rdd2West}/modules/fs/fatfs &&
          test -d ${rdd2West}/modules/hal/cmsis &&
          test -d ${rdd2West}/modules/hal/cmsis_6 &&
          test -d ${rdd2West}/modules/hal/nxp &&
          test -d ${rdd2West}/modules/lib/cmsis-dsp &&
          test -d ${rdd2West}/modules/lib/zenoh-pico &&
          test -d ${rdd2West}/modules/lib/zephyr_boards
        '';
      };

    "rdd2:simulation:sil:build" =
      (task "cerebri_rdd2" "Build the 64-bit RDD2 native simulator against local generated packages." ''
        nix run .#build-native-sim -- -p auto -- \
          "-DEXTRA_CONF_FILE=$PWD/tests/zephyr/native_sil.conf" \
          -DRDD2_RUMOCA_VERSION=workspace \
          "-DRDD2_RUMOCA_EXECUTABLE=${resultRoot}/rumoca/bin/rumoca" \
          "-DRDD2_CEREBRI_MODULES_ROOT=${source "cerebri_modules"}" \
          "-DRDD2_ZROS_ROOT=${source "zros"}" \
          "-DRDD2_CSYN_ROOT=${source "csyn"}" \
          "-DRDD2_MODELICA_MODELS_ROOT=${source "modelica_models"}" \
          "-DZEPHYR_EXTRA_MODULES=${editableZephyrModules}" \
          -DZEPHYR_TOOLCHAIN_VARIANT=host \
          "-DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C=${synapseC}"
      '')
      // {
        after = [
          "rdd2:workspace:ready"
          "rumoca:compiler"
          "synapse-fbs:build"
        ];
        env = {
          RDD2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
          RDD2_CSYN_ROOT = source "csyn";
          RDD2_MODELICA_MODELS_ROOT = source "modelica_models";
          RDD2_WEST_WORKSPACE = rdd2West;
          RDD2_ZROS_ROOT = source "zros";
          ZEPHYR_TOOLCHAIN_VARIANT = "host";
        };
      };

    "rdd2:simulation:sil:build-32" =
      (task "cerebri_rdd2" "Build the 32-bit RDD2 native simulator against local generated packages." ''
        RDD2_NATIVE_SIM_BOARD=native_sim \
        RDD2_NATIVE_SIM_BUILD_DIR="$PWD/build-native_sim32" \
          nix run .#build-native-sim -- -p auto -- \
            "-DEXTRA_CONF_FILE=$PWD/tests/zephyr/native_sil.conf" \
            -DRDD2_RUMOCA_VERSION=workspace \
            "-DRDD2_RUMOCA_EXECUTABLE=${resultRoot}/rumoca/bin/rumoca" \
            "-DRDD2_CEREBRI_MODULES_ROOT=${source "cerebri_modules"}" \
            "-DRDD2_ZROS_ROOT=${source "zros"}" \
            "-DRDD2_CSYN_ROOT=${source "csyn"}" \
            "-DRDD2_MODELICA_MODELS_ROOT=${source "modelica_models"}" \
            "-DZEPHYR_EXTRA_MODULES=${editableZephyrModules}" \
            -DZEPHYR_TOOLCHAIN_VARIANT=host \
            "-DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C=${synapseC}"
      '')
      // {
        after = [
          "rdd2:workspace:ready"
          "rumoca:compiler"
          "synapse-fbs:build"
        ];
        env = rdd2WestEnv // {
          ZEPHYR_TOOLCHAIN_VARIANT = "host";
        };
      };

    "rdd2:simulation:sil:test" =
      (task "FastDyn" "Run the RDD2 native simulator against its Rumoca-generated FMI 3 plant." ''
        cargo build --release --locked \
          --manifest-path ${source "cerebri_rdd2"}/tools/fastdyn_mission/Cargo.toml
        exec ${source "cerebri_rdd2"}/tools/fastdyn_mission/target/release/cerebri-rdd2-mission \
          --native-sim ${source "cerebri_rdd2"}/build-native_sim/zephyr/zephyr.exe \
          --shared-memory ${root}/.devenv/state/rdd2-sil-lockstep.bin \
          --plant-library "$RDD2_RUMOCA_PLANT_LIBRARY" \
          --plant-description "$RDD2_RUMOCA_PLANT_DESCRIPTION" \
          --report ${source "cerebri_rdd2"}/artifacts/sil/mission.json \
          --trajectory ${source "cerebri_rdd2"}/artifacts/sil/mission-trajectory.csv
      '')
      // {
        after = [
          "modelica-models:rdd2:export-plant"
          "rdd2:simulation:sil:build"
        ];
        env = {
          RDD2_RUMOCA_PLANT_DESCRIPTION = "${source "modelica_models"}/artifacts/vehicles/rdd2/plant/modelDescription.xml";
          RDD2_RUMOCA_PLANT_LIBRARY = "${source "modelica_models"}/artifacts/vehicles/rdd2/plant/binaries/x86_64-linux/Vehicles_Rdd2_AvionicsPlant.so";
        };
      };

    "rdd2:simulation:modelica:test" =
      (task "modelica_models" "Run the pure Modelica RDD2 controller and physics mission with Rumoca." ''
        MODELICA_MODELS_ROOT="$PWD" \
          nix run --override-input rumoca "git+file://${source "rumoca"}" \
            .#rdd2-qualification
      '')
      // {
        after = [
          "modelica-models:build"
          "rumoca:python"
        ];
      };

    "rdd2:simulation:bil:build" =
      (task "cerebri_rdd2" "Build the RDD2 hardware binary and FastDyn lockstep mission bridge for BIL."
        ''
          if ! test -f "$PWD/fastdyn/prj.conf"; then
            echo 'The RDD2-owned FastDyn integration is incomplete.' >&2
            exit 1
          fi
          conf_file="$(realpath fastdyn/prj.conf)"
          overlay="$(realpath fastdyn/mr_vmu_tropic.overlay)"
          RDD2_BUILD_DIR="$PWD/build-mr_vmu_tropic-fastdyn" nix run .#build -- -p auto -- \
            -DCONF_FILE="$conf_file" \
            -DDTC_OVERLAY_FILE="$overlay" \
            -DRDD2_RUMOCA_VERSION=workspace \
            "-DRDD2_RUMOCA_EXECUTABLE=${resultRoot}/rumoca/bin/rumoca" \
            "-DRDD2_CEREBRI_MODULES_ROOT=${source "cerebri_modules"}" \
            "-DRDD2_ZROS_ROOT=${source "zros"}" \
            "-DRDD2_CSYN_ROOT=${source "csyn"}" \
            "-DRDD2_MODELICA_MODELS_ROOT=${source "modelica_models"}" \
            "-DZEPHYR_EXTRA_MODULES=${editableZephyrModules}" \
            -DZEPHYR_TOOLCHAIN_VARIANT=gnuarmemb \
            "-DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C=${synapseC}"
          cargo build --release --locked \
            --manifest-path tools/fastdyn_mission/Cargo.toml
        ''
      )
      // {
        after = [
          "fastdyn:runtime:build"
          "modelica-models:rdd2:export-plant"
          "rdd2:workspace:ready"
          "rumoca:compiler"
          "synapse-fbs:build"
        ];
        env = rdd2WestEnv // {
          CEREBRI_RDD2_ROOT = source "cerebri_rdd2";
          FASTDYN_QEMU_PATH = "${fastdynQemu}/build/qemu-system-arm";
          RDD2_FASTDYN_BUILD_DIR = "${source "cerebri_rdd2"}/build-mr_vmu_tropic-fastdyn";
          RDD2_RUMOCA_PLANT_DESCRIPTION = "${source "modelica_models"}/artifacts/vehicles/rdd2/plant/modelDescription.xml";
          RDD2_RUMOCA_PLANT_LIBRARY = "${source "modelica_models"}/artifacts/vehicles/rdd2/plant/binaries/x86_64-linux/Vehicles_Rdd2_AvionicsPlant.so";
          RDD2_WORKSPACE_ROOT = rdd2West;
          ZEPHYR_TOOLCHAIN_VARIANT = "gnuarmemb";
        };
      };

    "rdd2:simulation:bil:test" =
      (task "FastDyn" "Run the FastDyn-rehosted RDD2 binary with its Rumoca-generated controller." ''
        export PATH="${fastdynVenv}/bin:$PATH"
        export LD_LIBRARY_PATH="$PWD/out/deps/cjson/install/lib:$PWD/build''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
        exec ${source "cerebri_rdd2"}/fastdyn/run_mission.sh
      '')
      // {
        after = [ "rdd2:simulation:bil:build" ];
        env = {
          CEREBRI_RDD2_ROOT = source "cerebri_rdd2";
          FASTDYN_MONITOR_ELF = "${fastdynQemu}/ws/monitor.elf";
          FASTDYN_ROOT = source "FastDyn";
          FASTDYN_QEMU_PATH = "${fastdynQemu}/build/qemu-system-arm";
          RDD2_FASTDYN_BUILD_DIR = "${source "cerebri_rdd2"}/build-mr_vmu_tropic-fastdyn";
          RDD2_RUMOCA_PLANT_DESCRIPTION = "${source "modelica_models"}/artifacts/vehicles/rdd2/plant/modelDescription.xml";
          RDD2_RUMOCA_PLANT_LIBRARY = "${source "modelica_models"}/artifacts/vehicles/rdd2/plant/binaries/x86_64-linux/Vehicles_Rdd2_AvionicsPlant.so";
          RDD2_WORKSPACE_ROOT = rdd2West;
        };
      };

    "rdd2:simulation:compare" =
      (task "cerebri_rdd2" "Compare RDD2 mission logs across all configured execution paths." ''
        exec nix run .#trajectory-compare
      '')
      // {
        after = [
          "rdd2:simulation:bil:test"
          "rdd2:simulation:modelica:test"
          "rdd2:simulation:sil:test"
        ];
        env = rdd2WestEnv;
      };

    "rdd2:firmware:build" =
      (task "cerebri_rdd2" "Build RDD2 firmware for the default hardware target." ''
        nix run .#build -- -p auto -- \
          -DRDD2_RUMOCA_VERSION=workspace \
          "-DRDD2_RUMOCA_EXECUTABLE=${resultRoot}/rumoca/bin/rumoca" \
          "-DRDD2_CEREBRI_MODULES_ROOT=${source "cerebri_modules"}" \
          "-DRDD2_ZROS_ROOT=${source "zros"}" \
          "-DRDD2_CSYN_ROOT=${source "csyn"}" \
          "-DRDD2_MODELICA_MODELS_ROOT=${source "modelica_models"}" \
          "-DZEPHYR_EXTRA_MODULES=${editableZephyrModules}" \
          -DZEPHYR_TOOLCHAIN_VARIANT=gnuarmemb \
          "-DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C=${synapseC}"
      '')
      // {
        after = [
          "rdd2:workspace:ready"
          "rumoca:compiler"
          "synapse-fbs:build"
        ];
        env = {
          RDD2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
          RDD2_CSYN_ROOT = source "csyn";
          RDD2_MODELICA_MODELS_ROOT = source "modelica_models";
          RDD2_WEST_WORKSPACE = rdd2West;
          RDD2_ZROS_ROOT = source "zros";
          ZEPHYR_TOOLCHAIN_VARIANT = "gnuarmemb";
        };
      };

    "rdd2:firmware:configure" =
      (task "cerebri_rdd2" "Open Zephyr menuconfig for the RDD2 hardware build." ''
        nix run .#menuconfig
      '')
      // {
        after = [ "rdd2:firmware:build" ];
        env = rdd2WestEnv // {
          ZEPHYR_TOOLCHAIN_VARIANT = "gnuarmemb";
        };
      };

    "rdd2:firmware:flash" =
      (task "cerebri_rdd2" "Flash the previously built RDD2 firmware." ''
        if ! printf '%s' "$DEVENV_TASK_INPUT" | jq -e '.confirm == true' >/dev/null; then
          echo 'refusing to flash without --input confirm=true' >&2
          exit 2
        fi
        nix run .#flash
      '')
      // {
        after = [ "rdd2:firmware:build" ];
        input.confirm = false;
        env = {
          RDD2_CEREBRI_MODULES_ROOT = source "cerebri_modules";
          RDD2_CSYN_ROOT = source "csyn";
          RDD2_MODELICA_MODELS_ROOT = source "modelica_models";
          RDD2_WEST_WORKSPACE = rdd2West;
          RDD2_ZROS_ROOT = source "zros";
          ZEPHYR_TOOLCHAIN_VARIANT = "gnuarmemb";
        };
      };

    "cerebri-modules:build" = {
      description = "Build the editable Cerebri module tests in the CUBS2 West workspace.";
      cwd = cubs2West;
      after = [ "cubs2:workspace:ready" ];
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
      after = [ "cubs2:workspace:ready" ];
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
        "cubs2:workspace:ready"
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

    "fastdyn:build" =
      (task "FastDyn" "Create FastDyn's project-owned Python environment." ''
        python -m venv ${fastdynVenv}
        PATH="${fastdynVenv}/bin:$PATH" \
          ./setup.sh --venv ${fastdynVenv} --skip-optifuzz --skip-rumoca
        {
          nix hash file requirements.txt
          nix hash file setup.sh
        } > ${fastdynVenv}/.cognipilot-inputs
      '')
      // {
        status = ''
          test -x ${fastdynVenv}/bin/python || exit 1
          test -f ${fastdynVenv}/.cognipilot-inputs || exit 1
          {
            nix hash file requirements.txt
            nix hash file setup.sh
          } | cmp -s - ${fastdynVenv}/.cognipilot-inputs
        '';
      };

    "fastdyn:runtime:build" =
      task "FastDyn" "Build FastDyn's Rumoca, patched QEMU, plugin, and device-model runtime."
        ''
          python -m venv ${fastdynVenv}
          PATH="${fastdynVenv}/bin:$PATH" \
            ./setup.sh --venv ${fastdynVenv} --build-qemu --qemu-root ${fastdynQemu} \
              --with-rumoca --skip-optifuzz
          export PATH="${fastdynVenv}/bin:$PATH"
          export PKG_CONFIG_PATH="$PWD/out/deps/cjson/install/lib/pkgconfig''${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
          export LD_LIBRARY_PATH="$PWD/out/deps/cjson/install/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
          make qemu_path=${fastdynQemu} DEV=true PHY=true FLIGHT_CONTROLLERS=true FMU=true
          cmake -S boardrunner/boardrunner_sdk -B boardrunner/boardrunner_sdk/build \
            -DFASTDYN_INCLUDE_DIR="$PWD/include" \
            -DQEMU_INCLUDE_DIR="${fastdynQemu}/include"
          cmake --build boardrunner/boardrunner_sdk/build -j2
        '';

    "fastdyn:test" =
      (task "FastDyn" "Run the FastDyn unit tests." ''
        ${fastdynVenv}/bin/python -m pytest tests/unit
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
        "cubs2:simulation:compare"
        "electrode-web:test"
        "modelica-models:test"
        "ppm:test"
        "qualisys-bridge:e2e"
        "qualisys-sdk:test"
        "rdd2:simulation:compare"
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
        nix develop --command npm --prefix packages/rumoca run build:release:core:pack
        nix develop --command npm --prefix packages/rumoca run build:release:full-web:pack
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
        "cubs2:firmware:build"
        "rdd2:firmware:build"
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
        "cubs2:simulation:sil:build-32"
        "fastdyn:test"
        "release:all"
      ];
    };
  };

  selectTasks = prefixes: {
    tasks = lib.filterAttrs (
      name: _task: lib.any (prefix: lib.hasPrefix prefix name) prefixes
    ) allTasks;
  };
in
{
  # These are ordinary Devenv modules. Profiles import only the task sets they
  # support, so task discovery and execution use the selected tool environment.
  common = selectTasks [
    "results:"
    "sources:"
    "workspace:"
  ];
  cache = selectTasks [ "cache:" ];
  synapse = selectTasks [ "synapse-fbs:" ];
  rumoca = selectTasks [ "rumoca:" ];
  modelica = selectTasks [ "modelica-models:" ];
  modelicaCore = {
    tasks = lib.getAttrs [
      "modelica-models:build"
      "modelica-models:test"
    ] allTasks;
  };
  modelicaCubs2 = selectTasks [ "modelica-models:cubs2:" ];
  modelicaRdd2 = selectTasks [ "modelica-models:rdd2:" ];
  csyn = selectTasks [ "csyn:" ];
  electrode = selectTasks [ "electrode-web:" ];
  cubs2 = selectTasks [ "cubs2:" ];
  cubs2West = {
    tasks = lib.getAttrs [
      "cubs2:workspace:ready"
      "cubs2:workspace:update"
    ] allTasks;
  };
  rdd2 = selectTasks [ "rdd2:" ];
  zephyr-libraries = selectTasks [
    "cerebri-modules:"
    "zros:"
  ];
  qualisys = selectTasks [
    "qualisys-bridge:"
    "qualisys-sdk:"
  ];
  ppm = selectTasks [ "ppm:" ];
  ros2 = selectTasks [ "ros2:" ];
  fastdynRuntime = {
    tasks = lib.getAttrs [ "fastdyn:runtime:build" ] allTasks;
  };
  fastdyn = selectTasks [ "fastdyn:" ];
  release = selectTasks [ "release:" ];
  ci = selectTasks [ "ci:" ];
}
