{
  config,
  lib,
  pkgs,
  taskModules,
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
    languages.rust = {
      enable = true;
      channel = "nixpkgs";
    };
    packages =
      with pkgs;
      [
        pkg-config
        sccache
      ]
      ++ linux [ pkgs.udev ];
    env = {
      CARGO_INCREMENTAL = "0";
      RUSTC_WRAPPER = lib.getExe pkgs.sccache;
    };
  };

  web = {
    languages.javascript = {
      enable = true;
      package = pkgs.nodejs_24;
    };
    packages = [ pkgs.wasm-pack ];
  };

  modelicaTools = {
    imports = [ taskModules.rumoca ];
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
    packages =
      with pkgs;
      [
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
        protobuf
        python3
        python3Packages.distlib
        rustc
        universal-ctags
        which
        zlib
      ]
      ++ linux [ pkgs.iproute2 ];
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
      dashboardPort = config.processes.synapse-qualisys-bridge.ports.dashboard.value;
    in
    {
      packages = with pkgs; [
        openssl
        playwright-test
        zenoh
      ];

      processes.synapse-qualisys-bridge = {
        cwd = source "synapse_qualisys_bridge";
        exec = ''
          exec cargo run --locked --bin synapse-qualisys-bridge \
            --config "paths=['${source "synapse_fbs"}/target/xtask/packages/rust']" -- \
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
        watch = {
          paths = [ (source "synapse_qualisys_bridge") ];
          extensions = [
            "rs"
            "toml"
          ];
          ignore = [ "target" ];
        };
      };
    };
in
{
  profiles = {
    synapse = {
      module = {
        imports = [
          rust
          web
          taskModules.synapse
        ];
        env.COGNIPILOT_PROFILE = "synapse";
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
        imports = [
          modelicaTools
          taskModules.modelica
        ];
        env.COGNIPILOT_PROFILE = "modelica";
      };
    };

    electrode = {
      extends = [ "synapse" ];
      module = {
        imports = [
          taskModules.rumoca
          taskModules.electrode
        ];
        env.COGNIPILOT_PROFILE = "electrode";
      };
    };

    qualisys = {
      extends = [ "synapse" ];
      module = {
        imports = [
          qualisysModule
          taskModules.qualisys
        ];
        env.COGNIPILOT_PROFILE = "qualisys";
      };
    };

    ppm = {
      extends = [ "synapse" ];
      module = {
        imports = [
          taskModules.ppm
        ];
        env.COGNIPILOT_PROFILE = "ppm";
        processes.synapse-ppm-bridge = {
          cwd = source "synapse_ppm_bridge";
          exec = ''
            exec cargo run --locked \
              --config "paths=['${source "synapse_fbs"}/target/xtask/packages/rust']"
          '';
          after = [ "ppm:build" ];
          restart = {
            on = "on_failure";
            max = 3;
          };
          watch = {
            paths = [ (source "synapse_ppm_bridge") ];
            extensions = [
              "rs"
              "toml"
            ];
            ignore = [ "target" ];
          };
        };
      };
    };

    zros = {
      extends = [ "synapse" ];
      module = {
        imports = [
          zephyr
          taskModules.csyn
          taskModules.cubs2West
          taskModules.zephyr-libraries
        ];
        env.COGNIPILOT_PROFILE = "zros";
      };
    };

    ros2 = {
      extends = [ "synapse" ];
      module = {
        imports = [ taskModules.ros2 ];
        env.COGNIPILOT_PROFILE = "ros2";
      };
    };

    cubs2 = {
      extends = [ "synapse" ];
      module =
        { config, ... }:
        let
          healthPort = config.processes.electrode-ground-station.ports.health.value;
          telemetryPort = config.processes.electrode-ground-station.ports.telemetry.value;
          lanRequestPort = config.processes.electrode-ground-station.ports.lan-request.value;
          endpoint = "udp/127.0.0.1:${toString telemetryPort}";
        in
        {
          imports = [
            fastdyn
            modelicaTools
            zephyr
            qualisysModule
            taskModules.csyn
            taskModules.cubs2
            taskModules.electrode
            taskModules.fastdynRuntime
            taskModules.modelicaCore
            taskModules.modelicaCubs2
            taskModules.qualisys
            taskModules.zephyr-libraries
          ];

          env.COGNIPILOT_PROFILE = "cubs2";

          enterShell = ''
            echo "CUBS2 firmware:   devenv tasks run cubs2:firmware:build"
            echo "CUBS2 Modelica:   devenv tasks run cubs2:simulation:modelica:test"
            echo "CUBS2 SIL test:   devenv tasks run cubs2:simulation:sil:test"
            echo "CUBS2 BIL test:   devenv tasks run cubs2:simulation:bil:test"
            echo "CUBS2 deployment: devenv up"
            echo "CUBS2 guide:      docs/cubs2.md"
          '';

          tasks."electrode-ground-station:state" = {
            description = "Create mutable ground-station runtime state.";
            exec = "mkdir -p ${lib.escapeShellArg (config.env.DEVENV_STATE + "/ground-station")}";
            status = "test -d ${lib.escapeShellArg (config.env.DEVENV_STATE + "/ground-station")}";
            before = [ "devenv:processes:electrode-ground-station" ];
          };

          processes = {
            electrode-ground-station = {
              cwd = source "electrode_web";
              exec = ''
                exec cargo run --locked -p electrode-ground-station \
                  --config "paths=['${source "synapse_fbs"}/target/xtask/packages/rust']" -- \
                  --addr 127.0.0.1:${toString healthPort}
              '';
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
              watch = {
                paths = [ (source "electrode_web") ];
                extensions = [
                  "rs"
                  "toml"
                ];
                ignore = [
                  "node_modules"
                  "target"
                ];
              };
            };

            electrode-ppm-bridge = {
              cwd = source "electrode_web";
              exec = ''
                exec cargo run --locked -p electrode-ppm-bridge --bin electrode-ppm-bridge \
                  --config "paths=['${source "synapse_fbs"}/target/xtask/packages/rust']"
              '';
              after = [
                "electrode-web:build"
                "devenv:processes:electrode-ground-station@ready"
              ];
              env = {
                PPM_CHANNEL_MAP = "1,2,0,3,4";
                ZENOH_CONNECT = endpoint;
              };
              restart = {
                on = "on_failure";
                max = 3;
              };
              watch = {
                paths = [ (source "electrode_web") ];
                extensions = [
                  "rs"
                  "toml"
                ];
                ignore = [
                  "node_modules"
                  "target"
                ];
              };
            };

            electrode-fake-vehicle = {
              cwd = source "electrode_web";
              exec = ''
                exec cargo run --locked -p electrode-fake-sim \
                  --config "paths=['${source "synapse_fbs"}/target/xtask/packages/rust']" -- \
                  --role both \
                  --mode client \
                  --endpoint ${endpoint} \
                  --ws-endpoint ""
              '';
              after = [
                "electrode-web:build"
                "devenv:processes:electrode-ground-station@ready"
                "devenv:processes:electrode-ppm-bridge@started"
              ];
              start.enable = false;
              restart = {
                on = "on_failure";
                max = 3;
              };
              watch = {
                paths = [ (source "electrode_web") ];
                extensions = [
                  "rs"
                  "toml"
                ];
                ignore = [
                  "node_modules"
                  "target"
                ];
              };
            };

            synapse-qualisys-bridge = {
              after = lib.mkAfter [ "devenv:processes:electrode-ground-station@ready" ];
              start.enable = false;
              exec = lib.mkForce ''
                exec cargo run --locked --bin synapse-qualisys-bridge \
                  --config "paths=['${source "synapse_fbs"}/target/xtask/packages/rust']" -- \
                  --zenoh-mode client \
                  --zenoh-connect ${endpoint} \
                  --web-bind 127.0.0.1:${toString config.processes.synapse-qualisys-bridge.ports.dashboard.value}
              '';
            };
          };
        };
    };

    rdd2 = {
      extends = [ "synapse" ];
      module = {
        imports = [
          fastdyn
          modelicaTools
          zephyr
          taskModules.fastdynRuntime
          taskModules.modelicaCore
          taskModules.modelicaRdd2
          taskModules.rdd2
        ];
        env.COGNIPILOT_PROFILE = "rdd2";
        enterShell = ''
          echo "RDD2 firmware: devenv tasks run rdd2:firmware:build"
          echo "RDD2 Modelica: devenv tasks run rdd2:simulation:modelica:test"
          echo "RDD2 SIL test: devenv tasks run rdd2:simulation:sil:test"
          echo "RDD2 BIL test: devenv tasks run rdd2:simulation:bil:test"
          echo "RDD2 guide:    docs/rdd2.md"
        '';
      };
    };

    fastdyn.module = {
      imports = [
        fastdyn
        taskModules.fastdyn
      ];
      env.COGNIPILOT_PROFILE = "fastdyn";
    };

    release = {
      extends = [ "cubs2" ];
      module = {
        imports = [
          taskModules.cache
          taskModules.modelicaRdd2
          taskModules.ppm
          taskModules.rdd2
          taskModules.release
          taskModules.ros2
        ];
        env.COGNIPILOT_PROFILE = "release";
        packages = with pkgs; [
          clang-tools
          cargo-release
          cachix
          hyperfine
        ];
      };
    };

    workspace = {
      extends = [ "release" ];
      module = {
        imports = [
          fastdyn
          taskModules.ci
          taskModules.fastdyn
        ];
        env.COGNIPILOT_PROFILE = "workspace";
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
      module = {
        env.COGNIPILOT_PROFILE = "ci";
        packages = with pkgs; [
          clang-tools
          hyperfine
        ];
      };
    };
  };
}
