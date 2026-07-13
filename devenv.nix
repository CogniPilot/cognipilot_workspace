{
  config,
  lib,
  pkgs,
  ...
}:

let
  root = config.devenv.root;
  rawComponents = config.workspace.components;
  componentNames = builtins.attrNames rawComponents;
  unknownDependencies = lib.concatMap (
    name:
    map (dependency: "${name} -> ${dependency}") (
      builtins.filter (
        dependency: !(builtins.elem dependency componentNames)
      ) rawComponents.${name}.dependencies
    )
  ) componentNames;
  unknownDevenvProfiles = builtins.filter (profile: !(builtins.hasAttr profile config.profiles)) (
    lib.unique (
      builtins.filter (profile: profile != null) (
        map (name: rawComponents.${name}.devenvProfile) componentNames
      )
    )
  );
  components =
    if unknownDependencies != [ ] then
      throw "workspace components reference unknown dependencies: ${builtins.concatStringsSep ", " unknownDependencies}"
    else if unknownDevenvProfiles != [ ] then
      throw "workspace components reference unknown devenv profiles: ${builtins.concatStringsSep ", " unknownDevenvProfiles}"
    else
      rawComponents;
  workspaceProfileData = import ./nix/profiles.nix { inherit componentNames; };
  workspaceProfiles = workspaceProfileData.resolved;
  launch = import ./launch { inherit lib; };
  repositoryManifest = pkgs.writeText "cognipilot_workspace-repositories.json" (
    builtins.toJSON {
      schema = 1;
      profiles = workspaceProfiles;
      profileDefinitions = workspaceProfileData.definitions;
      components = lib.mapAttrs (name: component: {
        inherit (component)
          path
          dependencies
          repo
          devenvProfile
          ;
        # Snapshot optionality is derived only from default-profile membership.
        optional = !(builtins.elem name workspaceProfiles.default);
        tasks = {
          local = {
            build = component.local.build != null;
            test = component.local.test != null;
          };
          release = {
            build = component.release.build != null;
            test = component.release.test != null;
          };
        };
      }) components;
    }
  );
  launchManifest = pkgs.writeText "cognipilot_workspace-launches.json" (
    builtins.toJSON {
      schema = 1;
      profiles = launch.manifest;
    }
  );
  wsCompletions = pkgs.runCommand "cognipilot_workspace-completions" { } ''
    install -Dm444 ${./completions/ws.bash} \
      "$out/share/bash-completion/completions/ws"
    install -Dm444 ${./completions/_ws} \
      "$out/share/zsh/site-functions/_ws"
    install -Dm444 ${./completions/ws.fish} \
      "$out/share/fish/vendor_completions.d/ws.fish"
  '';
  python = pkgs.python3.withPackages (
    ps: with ps; [
      pip
      pyyaml
      west
    ]
  );
  zephyrPython = pkgs.python3.withPackages (
    ps: with ps; [
      anytree
      canopen
      colorama
      intelhex
      jinja2
      jsonschema
      junitparser
      natsort
      packaging
      patool
      ply
      psutil
      pyelftools
      pykwalify
      pylink-square
      pyserial
      pytest
      pyyaml
      requests
      reuse
      semver
      tabulate
      tqdm
      west
    ]
  );
in
{
  imports = [ ./nix/components ];

  name = "cognipilot_workspace";
  devenv.cliVersion = "2.1.2";

  # Keep the always-on shell deliberately small. Each component's flake owns
  # its language/compiler toolchain, so entering this workspace does not fetch
  # Rust, Node, Zephyr SDKs, ROS, or native build stacks pre-emptively.
  packages = with pkgs; [
    bashInteractive
    coreutils
    curl
    findutils
    git
    gnugrep
    gnused
    jq
    python
    (lib.getBin util-linux)
    which
    wsCompletions
  ];

  env = {
    COGNIPILOT_WORKSPACE_ROOT = root;
    COGNIPILOT_COMPONENTS_ROOT = "${root}/src";
    COGNIPILOT_LAUNCH_MANIFEST = launchManifest;
    COGNIPILOT_REPO_MANIFEST = repositoryManifest;
  };

  profiles = {
    # Small host-side Rust applications without their own flake can opt into a
    # pinned compiler and the native libraries needed by serialport/libudev.
    rust-serial-toolchain.module = {
      packages = with pkgs; [
        cargo
        pkg-config
        rustc
        rustfmt
        systemd.dev
      ];
    };

    # FastDyn is private and expensive to provision. Its compiler/QEMU stack is
    # only added when `ws build FastDyn` or `ws shell FastDyn` requests it.
    fastdyn-toolchain.module = {
      env = {
        COGNIPILOT_FASTDYN_PROFILE = "1";
        COGNIPILOT_FASTDYN_PYTHON = "${pkgs.python3}/bin/python3";
        LD_LIBRARY_PATH = lib.makeLibraryPath [
          pkgs.stdenv.cc.cc.lib
          pkgs.zlib
        ];
      };
      packages = with pkgs; [
        bison
        cargo
        cmake
        dtc
        expat
        flex
        gcc
        glib
        gnumake
        libcjson
        libfdt
        meson
        ninja
        pixman
        pkg-config
        python3
        rustc
        universal-ctags
        zlib
      ];
    };

    # Real module tests use Zephyr's native_sim platform. Keep these tools out
    # of the base shell unless a selected component declares this profile.
    zephyr-tests.module = {
      env.COGNIPILOT_ZEPHYR_PYTHON = "${zephyrPython}/bin/python";
      env.COGNIPILOT_ZEPHYR_WEST = "${zephyrPython}/bin/west";
      packages = with pkgs; [
        ccache
        clang-tools
        cmake
        dtc
        gcc
        gnumake
        gperf
        ninja
      ];
    };

    # Lint tooling is opt-in so developers on constrained connections do not
    # download it merely by entering the normal workspace shell.
    ci.module = {
      packages = with pkgs; [
        actionlint
        deadnix
        nixfmt
        ruff
        shellcheck
        shfmt
        statix
      ];
    };

  }
  # Launch profiles and their CLI metadata share one registry in launch/.
  // launch.profiles;

  scripts = {
    ws = {
      description = "CogniPilot workspace mode, build, test, and status frontend";
      exec = ''exec ${pkgs.bash}/bin/bash "${root}/scripts/ws" "$@"'';
    };

    csyn = {
      description = "Run the prebuilt local CSyn CLI";
      exec = ''
        binary="${root}/src/csyn/rust/target/debug/csyn"
        if [[ ! -x "$binary" ]]; then
          printf 'CSyn build artifact is missing or not executable: %s\n' "$binary" >&2
          printf 'run: ws build csyn\n' >&2
          exit 1
        fi
        exec "$binary" "$@"
      '';
    };

    rumoca = {
      description = "Run the prebuilt local Rumoca compiler";
      exec = ''
        binary="${root}/.devenv/state/results/rumoca/bin/rumoca"
        if [[ ! -x "$binary" ]]; then
          printf 'Rumoca build artifact is missing or not executable: %s\n' "$binary" >&2
          printf 'run: ws build rumoca\n' >&2
          exit 1
        fi
        exec "$binary" "$@"
      '';
    };

    workspace-west = {
      description = "Validate and materialize the shared pinned west workspace";
      exec = ''exec ${python}/bin/python "${root}/scripts/workspace-west.py" "$@"'';
    };

    workspace-flake-ref = {
      description = "Produce a filtered local or commit-pinned component flake reference";
      exec = ''exec ${pkgs.bash}/bin/bash "${root}/scripts/workspace-flake-ref" "$@"'';
    };

    west = {
      description = "Run west in the pinned shared CogniPilot workspace";
      exec = ''exec ${pkgs.bash}/bin/bash "${root}/scripts/west-workspace" "$@"'';
    };
  };

  tasks = import ./nix/tasks.nix {
    inherit
      config
      lib
      python
      root
      ;
  };

  enterShell = ''
    if [[ -n "''${BASH_VERSION:-}" ]]; then
      source "${root}/completions/ws.bash"
    fi
    if [[ -z "''${COGNIPILOT_NESTED_WEST:-}" ]]; then
      ws banner || true
    fi
  '';
}
