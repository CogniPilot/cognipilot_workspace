{
  config,
  lib,
  pkgs,
  ...
}:

let
  root = config.git.root;
  source = repository: "${root}/src/${repository}";
  linux = lib.optionals pkgs.stdenv.hostPlatform.isLinux;

  rust = {
    packages =
      with pkgs;
      [
        cargo
        clippy
        pkg-config
        rust-analyzer
        rustc
        rustfmt
        sccache
      ]
      ++ linux [ pkgs.udev ];
    env = {
      CARGO_INCREMENTAL = "0";
      RUSTC_WRAPPER = lib.getExe pkgs.sccache;
    };
  };

  web = {
    packages = with pkgs; [
      nodejs_24
      wasm-pack
    ];
  };

  zephyr = {
    packages =
      with pkgs;
      [
        ccache
        cmake
        coreutils
        curl
        dtc
        file
        findutils
        gcc-arm-embedded
        git
        gitRepo
        gnumake
        gnugrep
        gnused
        gperf
        ncurses
        ninja
        openocd
        openssh
        picocom
        pkg-config
        python3
        python3Packages.pyocd
        unzip
        python3Packages.west
        which
        xz
        zip
      ]
      ++ linux (
        lib.optionals (pkgs.stdenv.hostPlatform.system == "x86_64-linux") [
          pkgs.gcc_multi
          pkgs.glibc_multi.dev
        ]
      );
    env = {
      NIX_HARDENING_ENABLE = "";
      ZEPHYR_TOOLCHAIN_VARIANT = "host";
    };
  };

  fastdyn = {
    packages = with pkgs; [
      bison
      cargo
      cjson
      cmake
      dtc
      expat
      flex
      gcc
      glib
      gnumake
      meson
      ninja
      pixman
      pkg-config
      python3
      python3Packages.distlib
      rustc
      universal-ctags
      zlib
    ];
    env = {
      LD_LIBRARY_PATH = lib.makeLibraryPath [
        pkgs.stdenv.cc.cc.lib
        pkgs.zlib
      ];
      PYTHONPATH = "${pkgs.python3Packages.distlib}/${pkgs.python3.sitePackages}";
    };
  };
in
{
  profiles = {
    rust.module = rust;
    web.module = web;
    zephyr.module = zephyr;

    diagnostics.module = {
      packages = with pkgs; [
        clang-tools
        hyperfine
      ];
    };

    synapse = {
      extends = [
        "rust"
        "web"
      ];
      module = {
        packages = with pkgs; [
          cmake
          github-cli
          openssl
          python3
          python3Packages.build
          python3Packages.twine
        ];
      };
    };

    modelica = {
      extends = [
        "rust"
        "web"
      ];
      module = {
        packages = with pkgs; [
          julia_111
          python312
          python312Packages.ipython
          python312Packages.numpy
          python312Packages.pandas
          python312Packages.sympy
        ];
      };
    };

    ground-station = {
      extends = [
        "modelica"
        "synapse"
      ];
      module =
        { config, ... }:
        let
          healthPort = config.processes.ground-station.ports.health.value;
        in
        {
          tasks."ground-station:state" = {
            description = "Create mutable ground-station runtime state.";
            exec = "mkdir -p ${lib.escapeShellArg (config.env.DEVENV_STATE + "/ground-station")}";
            before = [ "devenv:processes:ground-station" ];
          };

          processes.ground-station = {
            cwd = source "electrode_web";
            exec = "exec ./target/debug/electrode-ground-station --addr 127.0.0.1:${toString healthPort}";
            after = [ "electrode-web:build" ];
            ports.health.allocate = 8790;
            env = {
              ELECTRODE_GCS_AUTOPILOT_FILE = config.env.DEVENV_STATE + "/ground-station/autopilot.json";
              ELECTRODE_GCS_MAPPING_FILE = config.env.DEVENV_STATE + "/ground-station/mapping.json";
              ELECTRODE_GCS_SIMULATION_FILE = config.env.DEVENV_STATE + "/ground-station/simulation.json";
              ELECTRODE_GCS_TELEMETRY_ZENOH_CONNECT = "udp/192.168.10.2:7447";
              ELECTRODE_GCS_VELOCITY_BUDGET_CSV = config.env.DEVENV_STATE + "/ground-station/velocity-budget.csv";
              ELECTRODE_GCS_VELOCITY_BUDGET_DB =
                config.env.DEVENV_STATE + "/ground-station/velocity-budget-db.json";
            };
            ready = {
              http.get = {
                port = healthPort;
                path = "/gcs/health";
              };
              initial_delay = 1;
              period = 1;
              timeout = 300;
            };
            restart = {
              on = "on_failure";
              max = 3;
            };
          };
        };
    };

    qualisys = {
      extends = [ "synapse" ];
      module =
        { config, ... }:
        let
          dashboardPort = config.processes.qualisys-bridge.ports.dashboard.value;
        in
        {
          packages = with pkgs; [
            openssl
            playwright-test
            zenoh
          ];

          processes.qualisys-bridge = {
            cwd = source "synapse_qualisys_bridge";
            exec = ''
              exec ./target/debug/synapse-qualisys-bridge \
                --zenoh-mode client \
                --zenoh-connect udp/127.0.0.1:7447 \
                --web-bind 127.0.0.1:${toString dashboardPort}
            '';
            after = [ "qualisys-bridge:build" ];
            ports.dashboard.allocate = 8787;
            env.SYNAPSE_QUALISYS_BRIDGE_CONFIG = config.env.DEVENV_STATE + "/qualisys-bridge.toml";
            ready = {
              http.get = {
                port = dashboardPort;
                path = "/";
              };
              initial_delay = 1;
              period = 1;
              timeout = 300;
            };
            restart = {
              on = "on_failure";
              max = 3;
            };
          };
        };
    };

    ppm = {
      extends = [ "rust" ];
      module = { };
    };

    zros = {
      extends = [
        "synapse"
        "zephyr"
      ];
      module = { };
    };

    ros2 = {
      extends = [
        "rust"
        "synapse"
      ];
      module = { };
    };

    cubs2 = {
      extends = [
        "ground-station"
        "ppm"
        "synapse"
        "zephyr"
        "zros"
      ];
      module = { };
    };

    rdd2 = {
      extends = [
        "ground-station"
        "synapse"
        "zephyr"
        "zros"
      ];
      module = { };
    };

    simulation = {
      extends = [
        "cubs2"
        "qualisys"
      ];
      module =
        { config, ... }:
        let
          telemetryPort = config.processes.simulation.ports.telemetry.value;
          endpoint = "udp/127.0.0.1:${toString telemetryPort}";
        in
        {
          processes = {
            simulation = {
              cwd = source "electrode_web";
              exec = ''
                exec ./target/debug/electrode-fake-sim \
                  --role both \
                  --mode router \
                  --endpoint ${endpoint} \
                  --ws-endpoint ws/127.0.0.1:${toString telemetryPort}
              '';
              after = [ "electrode-web:build" ];
              ports.telemetry.allocate = 7447;
              restart = {
                on = "on_failure";
                max = 3;
              };
            };

            ground-station = {
              after = [ "devenv:processes:simulation@started" ];
              env.ELECTRODE_GCS_TELEMETRY_ZENOH_CONNECT = lib.mkForce endpoint;
            };

            qualisys-bridge = {
              after = [ "devenv:processes:simulation@started" ];
              exec = lib.mkForce ''
                exec ./target/debug/synapse-qualisys-bridge \
                  --zenoh-mode client \
                  --zenoh-connect ${endpoint} \
                  --web-bind 127.0.0.1:${toString config.processes.qualisys-bridge.ports.dashboard.value}
              '';
            };
          };
        };
    };

    fastdyn.module = fastdyn;

    release = {
      extends = [
        "cubs2"
        "diagnostics"
        "qualisys"
        "rdd2"
        "ros2"
      ];
      module = {
        packages = with pkgs; [
          cargo-release
          cachix
        ];
      };
    };

    ci = {
      extends = [ "diagnostics" ];
      module = { };
    };
  };
}
