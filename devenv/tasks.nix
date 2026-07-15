{ config, ... }:

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
  rumocaJavascript = "${source "rumoca"}/packages/rumoca/dist/dev-core";
in
{
  tasks = {
    "workspace:init" = {
      description = "Initialize the shared West source workspace.";
      cwd = root;
      status = ''
        test -f .west/config &&
          test "$(west config manifest.path)" = manifest &&
          test "$(west config manifest.file)" = west.yml
      '';
      exec = ''
        mkdir -p .west
        touch .west/config
        west config --local manifest.path manifest
        west config --local manifest.file west.yml
      '';
    };

    "workspace:sync" = {
      description = "Clone or update every repository at the manifest revision.";
      cwd = root;
      after = [ "workspace:links" ];
      exec = "west update";
    };

    "workspace:links" = {
      description = "Expose editable West projects at the paths required by the vehicle applications.";
      cwd = root;
      after = [ "workspace:init" ];
      status = ''
        test "$(readlink modules/lib/cerebri_lockstep)" = ../../src/cerebri_modules &&
          test "$(readlink modules/lib/csyn)" = ../../src/csyn &&
          test "$(readlink modules/lib/zros)" = ../../src/zros &&
          test "$(readlink models/vendor/CMM-v0.0.2)" = ../../src/modelica_models
      '';
      exec = ''
        mkdir -p modules/lib models/vendor
        ln -sfn ../../src/cerebri_modules modules/lib/cerebri_lockstep
        ln -sfn ../../src/csyn modules/lib/csyn
        ln -sfn ../../src/zros modules/lib/zros
        ln -sfn ../../src/modelica_models models/vendor/CMM-v0.0.2
      '';
    };

    "workspace:status" = {
      description = "Show Git status for every managed repository.";
      cwd = root;
      after = [ "workspace:init" ];
      exec = "west status";
    };

    "workspace:validate" = {
      description = "Validate the Devenv configuration and shared West manifest.";
      cwd = root;
      after = [ "workspace:init" ];
      before = [ "devenv:enterTest" ];
      exec = ''
        nix-instantiate --parse devenv.nix >/dev/null
        nix-instantiate --parse devenv/tasks.nix >/dev/null
        nix-instantiate --parse devenv/profiles.nix >/dev/null
        if test -f .west/config; then
          west manifest --validate
        fi
      '';
    };

    "synapse-fbs:build" = task "synapse_fbs" "Generate all local Synapse language packages." ''
      cargo run --locked --manifest-path xtask/Cargo.toml -- build --release-name local
    '';

    "synapse-fbs:test" =
      (task "synapse_fbs" "Run Synapse package checks." ''
        cargo run --locked --manifest-path xtask/Cargo.toml -- check
      '')
      // {
        after = [ "synapse-fbs:build" ];
      };

    "rumoca:compiler" = task "rumoca" "Build the local Rumoca compiler." ''
      nix build --no-pure-eval .#rumoca --out-link result-rumoca
    '';

    "rumoca:python" = task "rumoca" "Build the local Rumoca Python environment." ''
      nix build --no-pure-eval .#rumoca-python-env --out-link result-rumoca-python
    '';

    "rumoca:javascript" = task "rumoca" "Build the local Rumoca JavaScript package." ''
      npm --prefix packages/rumoca run build:dev
    '';

    "rumoca:test" =
      (task "rumoca" "Run the Rumoca flake checks." ''
        nix flake check --no-pure-eval
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
        nix build --no-pure-eval --override-input rumoca "path:${source "rumoca"}" .#default --out-link result-default
      '')
      // {
        after = [ "rumoca:compiler" ];
      };

    "modelica-models:test" =
      (task "modelica_models" "Run the Modelica checks with the editable Rumoca source." ''
        nix run --no-pure-eval --override-input rumoca "path:${source "rumoca"}" .#default
      '')
      // {
        after = [ "modelica-models:build" ];
      };

    "csyn:build" =
      (task "csyn" "Build CSyn against the generated local Synapse Rust package." ''
        cargo build --locked --manifest-path rust/Cargo.toml \
          --config "patch.crates-io.synapse_fbs.path='${synapseRust}'"
      '')
      // {
        after = [ "synapse-fbs:build" ];
      };

    "csyn:test" =
      (task "csyn" "Test CSyn against the generated local Synapse Rust package." ''
        cargo test --locked --manifest-path rust/Cargo.toml \
          --config "patch.crates-io.synapse_fbs.path='${synapseRust}'"
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
          --config "patch.crates-io.synapse_fbs.paths=['${synapseRust}']"
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
          --config "patch.crates-io.synapse_fbs.paths=['${synapseRust}']"
      '')
      // {
        after = [ "electrode-web:build" ];
      };

    "cubs2:build" =
      (task "cerebri_cubs2" "Build the CUBS2 native simulator against local generated packages." ''
        nix run --no-pure-eval .#build-native-sim-64 -- -p auto -- \
          "-DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C=${synapseC}"
      '')
      // {
        after = [
          "workspace:links"
          "rumoca:python"
          "synapse-fbs:build"
        ];
        env = {
          CUBS2_ALLOW_FOREIGN_WEST = "1";
          CUBS2_RUMOCA_PYTHON = "${source "rumoca"}/result-rumoca-python/bin/python";
          CUBS2_WORKSPACE_ROOT = root;
        };
      };

    "cubs2:test" =
      (task "cerebri_cubs2" "Run the project-owned CUBS2 native simulator SIL tests." ''
        nix run --no-pure-eval .#native-sim-64-sil-test
      '')
      // {
        after = [ "cubs2:build" ];
        env = {
          CUBS2_ALLOW_FOREIGN_WEST = "1";
          CUBS2_RUMOCA_PYTHON = "${source "rumoca"}/result-rumoca-python/bin/python";
          CUBS2_WORKSPACE_ROOT = root;
        };
      };

    "cubs2:build-hardware" =
      (task "cerebri_cubs2" "Build CUBS2 firmware for the default hardware target." ''
        nix run --no-pure-eval .#build
      '')
      // {
        after = [ "workspace:links" ];
        env = {
          CUBS2_ALLOW_FOREIGN_WEST = "1";
          CUBS2_WORKSPACE_ROOT = root;
        };
      };

    "cubs2:flash" =
      (task "cerebri_cubs2" "Flash the previously built CUBS2 firmware." ''
        nix run --no-pure-eval .#flash
      '')
      // {
        after = [ "cubs2:build-hardware" ];
        env = {
          CUBS2_ALLOW_FOREIGN_WEST = "1";
          CUBS2_WORKSPACE_ROOT = root;
        };
      };

    "rdd2:build" =
      (task "cerebri_rdd2" "Build the RDD2 native simulator against local generated packages." ''
        nix run --no-pure-eval .#build-native-sim -- -p auto -- \
          -DRDD2_RUMOCA_VERSION=workspace \
          "-DRDD2_RUMOCA_EXECUTABLE=${source "rumoca"}/result-rumoca/bin/rumoca" \
          "-DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C=${synapseC}"
      '')
      // {
        after = [
          "workspace:links"
          "rumoca:compiler"
          "synapse-fbs:build"
        ];
        env = {
          RDD2_ALLOW_FOREIGN_WEST = "1";
          RDD2_WORKSPACE_ROOT = root;
        };
      };

    "rdd2:build-hardware" =
      (task "cerebri_rdd2" "Build RDD2 firmware for the default hardware target." ''
        nix run --no-pure-eval .#build
      '')
      // {
        after = [ "workspace:links" ];
        env = {
          RDD2_ALLOW_FOREIGN_WEST = "1";
          RDD2_WORKSPACE_ROOT = root;
        };
      };

    "rdd2:flash" =
      (task "cerebri_rdd2" "Flash the previously built RDD2 firmware." ''
        nix run --no-pure-eval .#flash
      '')
      // {
        after = [ "rdd2:build-hardware" ];
        env = {
          RDD2_ALLOW_FOREIGN_WEST = "1";
          RDD2_WORKSPACE_ROOT = root;
        };
      };

    "cerebri-modules:build" = {
      description = "Build the Cerebri module tests in the shared West workspace.";
      cwd = root;
      after = [ "workspace:links" ];
      exec = ''
        west twister -T modules/lib/cerebri_lockstep/tests \
          -p native_sim/native/64 --force-platform \
          --outdir modules/lib/cerebri_lockstep/build/twister/build \
          --no-clean --build-only
      '';
    };

    "cerebri-modules:test" = {
      description = "Run the Cerebri module tests in the shared West workspace.";
      cwd = root;
      after = [ "cerebri-modules:build" ];
      exec = ''
        west twister -T modules/lib/cerebri_lockstep/tests \
          -p native_sim/native/64 --force-platform \
          --outdir modules/lib/cerebri_lockstep/build/twister/test \
          --no-clean
      '';
    };

    "zros:build" = {
      description = "Build the ZROS tests in the shared West workspace.";
      cwd = root;
      after = [ "workspace:links" ];
      exec = ''
        west twister -T modules/lib/zros/tests \
          -p native_sim/native/64 --force-platform \
          --outdir modules/lib/zros/build/twister/build \
          --no-clean --build-only
      '';
    };

    "zros:test" = {
      description = "Run the ZROS tests in the shared West workspace.";
      cwd = root;
      after = [ "zros:build" ];
      exec = ''
        west twister -T modules/lib/zros/tests \
          -p native_sim/native/64 --force-platform \
          --outdir modules/lib/zros/build/twister/test \
          --no-clean
      '';
    };

    "csyn:qualification" = {
      description = "Run the CSyn Zephyr qualification tests in the shared West workspace.";
      cwd = root;
      after = [
        "csyn:test"
        "workspace:links"
      ];
      exec = ''
        west twister -T modules/lib/csyn/zephyr/tests \
          -p native_sim/native/64 --force-platform \
          --outdir modules/lib/csyn/build/twister/qualification \
          --no-clean
      '';
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
          --config "patch.crates-io.synapse_fbs.path='${synapseRust}'"
      '')
      // {
        after = [ "synapse-fbs:build" ];
      };

    "qualisys-bridge:test" =
      (task "synapse_qualisys_bridge" "Test the Qualisys bridge against local Synapse." ''
        cargo test --locked \
          --config "patch.crates-io.synapse_fbs.path='${synapseRust}'"
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

    "ppm:build" = task "synapse_ppm_bridge" "Build the PPM bridge." ''
      cargo build --locked
    '';

    "ppm:test" =
      (task "synapse_ppm_bridge" "Test the PPM bridge." ''
        cargo test --locked
      '')
      // {
        after = [ "ppm:build" ];
      };

    "ros2:test" = task "csyn_ros2_bridge" "Run the project-owned colcon/ROS 2 CI application." ''
      nix run --no-pure-eval .#ci
    '';

    "fastdyn:build" = task "FastDyn" "Run the project-owned FastDyn/QEMU setup." ''
      ./setup.sh --python python3 --venv build/venv --qemu-root build/qemu \
        --build-qemu --skip-optifuzz --skip-qemu-workspace
    '';

    "fastdyn:test" =
      (task "FastDyn" "Run the FastDyn unit tests." ''
        build/venv/bin/python -m pytest tests/unit
      '')
      // {
        after = [ "fastdyn:build" ];
      };

    "release:check" = {
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
        "synapse-fbs:test"
        "zros:test"
      ];
    };

    "release:ppm" =
      (task "synapse_ppm_bridge" "Verify the PPM bridge Cargo release without publishing." ''
        cargo publish --locked --dry-run
      '')
      // {
        after = [ "release:check" ];
      };

    "release:csyn" =
      (task "csyn" "Verify the CSyn Cargo release without publishing." ''
        cargo publish --locked --dry-run --manifest-path rust/Cargo.toml
      '')
      // {
        after = [ "release:check" ];
      };

    "release:qualisys-sdk" =
      (task "qualisys_rust_sdk" "Verify the Qualisys SDK Cargo release without publishing." ''
        cargo publish --locked --dry-run
      '')
      // {
        after = [ "release:check" ];
      };

    "release:all" = {
      description = "Complete every configured release dry run; this never publishes.";
      after = [
        "release:csyn"
        "release:ppm"
        "release:qualisys-sdk"
      ];
    };
  };
}
