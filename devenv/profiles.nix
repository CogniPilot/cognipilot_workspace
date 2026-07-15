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
  emptyContainerHome = pkgs.runCommandLocal "cognipilot-workspace-container-home" { } ''
    mkdir -p "$out"
  '';

  zephyrPython = pkgs.python3.withPackages (
    pythonPackages: with pythonPackages; [
      anytree
      canopen
      cbor
      colorama
      coverage
      intelhex
      jsonschema
      junitparser
      mypy
      natsort
      numpy
      opencv4
      packaging
      patool
      ply
      psutil
      pyelftools
      pykwalify
      pylink-square
      pyocd
      pyserial
      pytest
      python-can
      python-dotenv
      pyyaml
      requests
      reuse
      semver
      spdx-tools
      tabulate
      tqdm
      west
    ]
  );

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
        gcovr
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
        zephyrPython
        unzip
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

  qualisysModule =
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
      extends = [ "synapse" ];
      module = {
        packages = with pkgs; [
          julia_111
          lld
          (python312.withPackages (
            pythonPackages: with pythonPackages; [
              ipython
              numpy
              pandas
              sympy
            ]
          ))
        ];
      };
    };

    qualisys = {
      extends = [ "synapse" ];
      module = qualisysModule;
    };

    ppm = {
      extends = [ "synapse" ];
      module = {
        processes.ppm-bridge = {
          cwd = source "synapse_ppm_bridge";
          exec = "exec ./target/debug/synapse-ppm-bridge";
          after = [ "ppm:build" ];
          restart = {
            on = "on_failure";
            max = 3;
          };
        };
      };
    };

    zros = {
      extends = [ "synapse" ];
      module = zephyr;
    };

    ros2 = {
      extends = [ "synapse" ];
      module = { };
    };

    cubs2 = {
      extends = [ "modelica" ];
      module =
        { config, ... }:
        let
          healthPort = config.processes.ground-station.ports.health.value;
          telemetryPort = config.processes.ground-station.ports.telemetry.value;
          lanRequestPort = config.processes.ground-station.ports.lan-request.value;
          endpoint = "udp/127.0.0.1:${toString telemetryPort}";
        in
        {
          imports = [
            zephyr
            qualisysModule
          ];

          tasks."ground-station:state" = {
            description = "Create mutable ground-station runtime state.";
            exec = "mkdir -p ${lib.escapeShellArg (config.env.DEVENV_STATE + "/ground-station")}";
            before = [ "devenv:processes:ground-station" ];
          };

          processes = {
            ground-station = {
              cwd = source "electrode_web";
              exec = "exec ./target/debug/electrode-ground-station --addr 127.0.0.1:${toString healthPort}";
              after = [ "electrode-web:build" ];
              ports = {
                health.allocate = 8790;
                telemetry.allocate = 7447;
                lan-request.allocate = 7448;
              };
              env = {
                ELECTRODE_GCS_AUTOPILOT_FILE = config.env.DEVENV_STATE + "/ground-station/autopilot.json";
                ELECTRODE_GCS_MAPPING_FILE = config.env.DEVENV_STATE + "/ground-station/mapping.json";
                ELECTRODE_GCS_SIMULATION_FILE = config.env.DEVENV_STATE + "/ground-station/simulation.json";
                ELECTRODE_GCS_LAN_REQUEST_LISTEN = "ws/0.0.0.0:${toString lanRequestPort}";
                ELECTRODE_GCS_TELEMETRY_ZENOH_CONNECT = lib.mkForce "";
                ELECTRODE_GCS_ZENOH_LISTEN = "udp/127.0.0.1:${toString telemetryPort}";
                ELECTRODE_GCS_ZENOH_WS_LISTEN = "ws/127.0.0.1:${toString telemetryPort}";
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

            simulation = {
              cwd = source "electrode_web";
              exec = ''
                exec ./target/debug/electrode-fake-sim \
                  --role autopilot \
                  --mode client \
                  --endpoint ${endpoint} \
                  --ws-endpoint ""
              '';
              after = [
                "electrode-web:build"
                "devenv:processes:ground-station@ready"
              ];
              restart = {
                on = "on_failure";
                max = 3;
              };
            };

            qualisys-bridge = {
              after = lib.mkAfter [ "devenv:processes:ground-station@ready" ];
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

    rdd2 = {
      extends = [ "modelica" ];
      module = zephyr;
    };

    fastdyn.module = fastdyn;

    release = {
      extends = [ "cubs2" ];
      module = {
        packages = with pkgs; [
          clang-tools
          cargo-release
          cachix
          hyperfine
        ];
      };
    };

    workspace = {
      extends = [
        "release"
        "fastdyn"
      ];
      module = {
        containers.shell = {
          name = "cognipilot-workspace";
          registry = "docker://ghcr.io/cognipilot/";
          version = "latest";
          copyToRoot = emptyContainerHome;
          startupCommand = "bash";
        };
      };
    };

    ci = {
      extends = [ "diagnostics" ];
      module = { };
    };
  };
}
