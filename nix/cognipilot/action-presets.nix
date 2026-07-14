{ lib }:

let
  inProjectShell = command: [
    "nix"
    "develop"
    "--no-pure-eval"
    "."
    "-c"
  ] ++ command;
  # Run from the materialized West view, but obtain every compiler, Python
  # module, and native tool from the manifest project's own locked dev shell.
  # `manifest` is the native West project identity written into the generated
  # local-view configuration; nixspace remains responsible only for creating
  # that view and crossing the process boundary.
  inWestManifestShellAt = cwd: command:
    let
      depth = builtins.length (lib.splitString "/" cwd);
      manifest =
        if cwd == "." then
          "./manifest#default"
        else
          "${lib.concatStrings (lib.replicate depth "../")}manifest#default";
    in
    [
      "nixspace"
      "west"
      "run"
      "--cwd"
      cwd
      "--"
      "nix"
      "develop"
      "--no-pure-eval"
      manifest
      "-c"
    ] ++ command;
  inWestManifestShell = inWestManifestShellAt ".";
  action = value: {
    dependsOn = [ ];
    environment = { };
    toolProfile = null;
  } // value;
  nixBuild = output: outLink: [
    "nix"
    "build"
    "--no-pure-eval"
    ".#${output}"
    "--out-link"
    outLink
  ];
  nixRun = app: [
    "nix"
    "run"
    "--no-pure-eval"
    ".#${app}"
  ];
  synapseCargoPatch = {
    artifactInput = "synapse-rust";
    prefix = ''patch.crates-io.synapse_fbs.path="'';
    suffix = ''"'';
  };
  cargoWithSynapse = project: command:
    command
    ++ lib.optionals (
      builtins.hasAttr "synapse-rust" (
        lib.attrByPath [ "targets" "default" "artifacts" "inputs" ] { } project
      )
    ) [
      "--config"
      synapseCargoPatch
    ];
  westProjectPath = project:
    if project.repositoryId == "cerebri_modules" then
      "modules/lib/cerebri_lockstep"
    else
      "modules/lib/${project.repositoryId}";
  twisterActions = project:
    let
      projectPath = westProjectPath project;
      twister = output: inWestManifestShell [
        "west"
        "twister"
        "-T"
        "${projectPath}/tests"
        "-p"
        "native_sim/native/64"
        "--force-platform"
        "--outdir"
        "${projectPath}/build/twister/${output}"
        "--no-clean"
      ];
    in
    {
      build = action {
        kind = "build";
        adapter = "twister-v1";
        environment = {
          NIX_HARDENING_ENABLE = "";
          ZEPHYR_TOOLCHAIN_VARIANT = "host";
        };
        argv = twister "build" ++ [ "--build-only" ];
      };
      test = action {
        kind = "test";
        adapter = "twister-v1";
        dependsOn = [ "build" ];
        environment = {
          NIX_HARDENING_ENABLE = "";
          ZEPHYR_TOOLCHAIN_VARIANT = "host";
        };
        argv = twister "test";
      };
    };
in
{
  cargo-v1 = {
    build = action {
      kind = "build";
      adapter = "cargo-v1";
      argv = inProjectShell [
        "cargo"
        "build"
        "--workspace"
      ];
    };
    test = action {
      kind = "test";
      adapter = "cargo-v1";
      dependsOn = [ "build" ];
      argv = inProjectShell [
        "cargo"
        "test"
        "--workspace"
      ];
    };
  };

  cargo-xtask-v1 = {
    build = action {
      kind = "build";
      adapter = "cargo-xtask-v1";
      argv = inProjectShell [
        "cargo"
        "run"
        "--locked"
        "--manifest-path"
        "xtask/Cargo.toml"
        "--"
        "build"
        "--release-name"
        "local"
      ];
    };
    test = action {
      kind = "test";
      adapter = "cargo-xtask-v1";
      dependsOn = [ "build" ];
      argv = inProjectShell [
        "cargo"
        "run"
        "--locked"
        "--manifest-path"
        "xtask/Cargo.toml"
        "--"
        "check"
      ];
    };
  };

  # Conventional flake-native editable workflow: realize the default package,
  # then execute the default app from the same locked project flake.
  nix-flake-app-v1 = {
    build = action {
      kind = "build";
      adapter = "nix-flake-package-v1";
      argv = nixBuild "default" "result-default";
    };
    test = action {
      kind = "test";
      adapter = "nix-flake-app-v1";
      dependsOn = [ "build" ];
      argv = nixRun "default";
    };
  };

  # Flakes that publish packages and checks, but intentionally no runnable app.
  nix-flake-check-v1 = {
    build = action {
      kind = "build";
      adapter = "nix-flake-package-v1";
      argv = nixBuild "default" "result-default";
    };
    test = action {
      kind = "test";
      adapter = "nix-flake-check-v1";
      dependsOn = [ "build" ];
      argv = [
        "nix"
        "flake"
        "check"
        "--no-pure-eval"
        "."
      ];
    };
  };

  # A project-owned flake app remains the authority for the exact ROS/colcon
  # environment and commands. The preset only realizes and invokes it.
  colcon-v1 = {
    build = action {
      kind = "build";
      adapter = "colcon-v1";
      argv = nixBuild "ci" "result-ci";
    };
    test = action {
      kind = "test";
      adapter = "colcon-v1";
      dependsOn = [ "build" ];
      argv = nixRun "ci";
    };
  };

  cargo-locked-v1 = {
    build = action {
      kind = "build";
      adapter = "cargo-locked-v1";
      toolProfile = "rust-libudev-sccache-v1";
      argv = [
        "cargo"
        "build"
        "--locked"
      ];
    };
    test = action {
      kind = "test";
      adapter = "cargo-locked-v1";
      toolProfile = "rust-libudev-sccache-v1";
      dependsOn = [ "build" ];
      argv = [
        "cargo"
        "test"
        "--locked"
      ];
    };
  };

  cargo-rust-manifest-v1 = project: {
    build = action {
      kind = "build";
      adapter = "cargo-locked-v1";
      argv = cargoWithSynapse project (inProjectShell [
        "cargo"
        "build"
        "--locked"
        "--manifest-path"
        "rust/Cargo.toml"
      ]);
    };
    test = action {
      kind = "test";
      adapter = "cargo-locked-v1";
      dependsOn = [ "build" ];
      argv = cargoWithSynapse project (inProjectShell [
        "cargo"
        "test"
        "--locked"
        "--manifest-path"
        "rust/Cargo.toml"
      ]);
    };
    qualification = action {
      kind = "test";
      adapter = "twister-v1";
      dependsOn = [ "test" ];
      environment = {
        NIX_HARDENING_ENABLE = "";
        ZEPHYR_TOOLCHAIN_VARIANT = "host";
      };
      argv = inWestManifestShell [
        "west"
        "twister"
        "-T"
        "modules/lib/csyn/zephyr/tests"
        "-p"
        "native_sim/native/64"
        "--force-platform"
        "--outdir"
        "modules/lib/csyn/build/twister/qualification"
        "--no-clean"
      ];
    };
  };

  synapse-qualisys-v1 = project: {
    build = action {
      kind = "build";
      adapter = "cargo-locked-v1";
      argv = cargoWithSynapse project (inProjectShell [
        "cargo"
        "build"
        "--locked"
        "--bin"
        "synapse-qualisys-bridge"
      ]);
    };
    test = action {
      kind = "test";
      adapter = "cargo-locked-v1";
      dependsOn = [ "build" ];
      argv = cargoWithSynapse project (inProjectShell [
        "cargo"
        "test"
        "--locked"
      ]);
    };
    qualification = action {
      kind = "test";
      adapter = "playwright-v1";
      dependsOn = [ "test" ];
      environment.BRIDGE_BIN = "target/debug/synapse-qualisys-bridge";
      argv = inProjectShell [
        "playwright"
        "test"
        "--config"
        "tests/e2e/playwright.config.js"
      ];
    };
  };

  npm-v1 = {
    build = action {
      kind = "build";
      adapter = "npm-v1";
      argv = inProjectShell [
        "npm"
        "run"
        "build"
      ];
    };
    test = action {
      kind = "test";
      adapter = "npm-v1";
      dependsOn = [ "build" ];
      argv = inProjectShell [
        "npm"
        "test"
      ];
    };
  };

  cmake-v1 = {
    configure = action {
      kind = "generate";
      adapter = "cmake-v1";
      toolProfile = "cmake-v1";
      argv = [
        "cmake"
        "-S"
        "."
        "-B"
        "build"
        "-G"
        "Ninja"
      ];
    };
    build = action {
      kind = "build";
      adapter = "cmake-v1";
      toolProfile = "cmake-v1";
      dependsOn = [ "configure" ];
      argv = [
        "cmake"
        "--build"
        "build"
      ];
    };
    test = action {
      kind = "test";
      adapter = "cmake-v1";
      toolProfile = "cmake-v1";
      dependsOn = [ "build" ];
      argv = [
        "ctest"
        "--test-dir"
        "build"
        "--output-on-failure"
      ];
    };
  };

  meson-v1 = {
    configure = action {
      kind = "generate";
      adapter = "meson-v1";
      toolProfile = "meson-glib-cjson-v1";
      argv = [
        "meson"
        "setup"
        "build"
      ];
    };
    build = action {
      kind = "build";
      adapter = "meson-v1";
      toolProfile = "meson-glib-cjson-v1";
      dependsOn = [ "configure" ];
      argv = [
        "meson"
        "compile"
        "-C"
        "build"
      ];
    };
  };

  cargo-npm-v1 = project:
    let
      inputs = lib.attrByPath [ "targets" "default" "artifacts" "inputs" ] { } project;
      hasSynapseRust = builtins.hasAttr "synapse-rust" inputs;
      hasSynapseJavascript = builtins.hasAttr "synapse-javascript" inputs;
      hasRumocaJavascript = builtins.hasAttr "rumoca-javascript" inputs;
      cargoLocalPath = {
        artifactInput = "synapse-rust";
        prefix = ''paths=["'';
        suffix = ''"]'';
      };
      npmBuildDependency =
        if hasRumocaJavascript then
          "npm-bind-rumoca"
        else if hasSynapseJavascript then
          "npm-bind-synapse"
        else
          "npm-install";
    in
  ({
    npm-install = action {
      kind = "build";
      adapter = "npm-v1";
      argv = inProjectShell [
        "npm"
        "ci"
      ];
    };
    cargo-build = action {
      kind = "build";
      adapter = "cargo-v1";
      dependsOn = [ "npm-install" ];
      argv = inProjectShell [
        "cargo"
        "build"
        "--locked"
        "--workspace"
      ] ++ lib.optionals hasSynapseRust [
        "--config"
        cargoLocalPath
      ];
    };
    npm-build = action {
      kind = "build";
      adapter = "npm-v1";
      dependsOn = [ npmBuildDependency ];
      argv = inProjectShell [
        "npm"
        "run"
        "build"
      ];
    };
    npm-test = action {
      kind = "test";
      adapter = "npm-v1";
      dependsOn = [ "npm-build" ];
      argv = inProjectShell [
        "npm"
        "run"
        "ci"
      ];
    };
    cargo-test = action {
      kind = "test";
      adapter = "cargo-v1";
      dependsOn = [
        "cargo-build"
        "npm-test"
      ];
      argv = inProjectShell [
        "cargo"
        "test"
        "--locked"
        "--workspace"
      ] ++ lib.optionals hasSynapseRust [
        "--config"
        cargoLocalPath
      ];
    };
  }
  // lib.optionalAttrs hasSynapseJavascript {
    npm-bind-synapse = action {
      kind = "build";
      adapter = "npm-v1";
      dependsOn = [ "npm-install" ];
      argv = inProjectShell [
        "npm"
        "install"
        "--no-save"
        "--package-lock=false"
        {
          artifactInput = "synapse-javascript";
        }
      ];
    };
  }
  // lib.optionalAttrs hasRumocaJavascript {
    npm-bind-rumoca = action {
      kind = "build";
      adapter = "npm-v1";
      dependsOn = [
        (if hasSynapseJavascript then "npm-bind-synapse" else "npm-install")
      ];
      argv = inProjectShell [
        "npm"
        "install"
        "--workspace"
        "apps/web"
        "--no-save"
        "--package-lock=false"
        {
          artifactInput = "rumoca-javascript";
        }
      ];
    };
  });

  rumoca-v1 = {
    compiler-build = action {
      kind = "build";
      adapter = "nix-flake-package-v1";
      argv = [
        "nix"
        "build"
        "--no-pure-eval"
        ".#rumoca"
        "--out-link"
        "result-rumoca"
      ];
    };
    python-build = action {
      kind = "build";
      adapter = "nix-flake-package-v1";
      argv = [
        "nix"
        "build"
        "--no-pure-eval"
        ".#rumoca-python-env"
        "--out-link"
        "result-rumoca-python"
      ];
    };
    javascript-build = action {
      kind = "build";
      adapter = "npm-v1";
      argv = inProjectShell [
        "npm"
        "--prefix"
        "packages/rumoca"
        "run"
        "build:dev"
      ];
    };
    test = action {
      kind = "test";
      adapter = "nix-flake-check-v1";
      argv = [
        "nix"
        "flake"
        "check"
        "--no-pure-eval"
        "."
      ];
    };
  };

  west-v1.build = action {
    kind = "build";
    adapter = "west-v1";
    argv = inProjectShell [
      "west"
      "build"
    ];
  };

  # Some repositories intentionally export only Zephyr metadata. An empty
  # preset represents that honestly instead of fabricating a build command.
  resource-only-v1 = { };

  rdd2-v1 = project:
    let
      inputs = project.targets.default.artifacts.inputs;
      hasRumoca = builtins.hasAttr "rumoca-compiler" inputs;
      hasSynapse = builtins.hasAttr "synapse-c" inputs;
    in
  {
    prepare = action {
      kind = "generate";
      adapter = "nix-flake-app-v1";
      argv = nixRun "west-update";
    };
    build = action {
      kind = "build";
      adapter = "nix-flake-app-v1";
      dependsOn = [ "prepare" ];
      environment.ZEPHYR_TOOLCHAIN_VARIANT = "host";
      argv = nixRun "build-native-sim" ++ [
        "--"
        "-p"
        "auto"
        "--"
      ]
      ++ lib.optionals hasRumoca [
        "-DRDD2_RUMOCA_VERSION=workspace"
        {
          artifactInput = "rumoca-compiler";
          prefix = "-DRDD2_RUMOCA_EXECUTABLE=";
        }
      ]
      ++ lib.optional hasSynapse {
        artifactInput = "synapse-c";
        prefix = "-DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C=";
      };
    };
  };

  zephyr-native-sim-v1 = project:
    let
      app = project.repositoryId;
      board = project.targets.default.variants.dimensions.board.default;
      boardSlug = lib.replaceStrings [ "/" ] [ "_" ] board;
      buildDirectory = "${app}/build-${boardSlug}_sil";
      hasSynapseC = builtins.hasAttr "synapse-c" project.targets.default.artifacts.inputs;
    in
  {
    build = action {
      kind = "build";
      adapter = "nixspace-west-v1";
      environment.ZEPHYR_TOOLCHAIN_VARIANT = "host";
      argv = [
        "nixspace"
        "west"
        "exec"
        "build"
        "-b"
        board
        "-d"
        buildDirectory
        app
        "-p"
        "auto"
        "--"
        "-DEXTRA_CONF_FILE=${app}/boards/native_sim_sil.conf"
      ] ++ lib.optional hasSynapseC {
        artifactInput = "synapse-c";
        prefix = "-DFETCHCONTENT_SOURCE_DIR_SYNAPSE_FBS_C=";
      };
    };
    test = action {
      kind = "test";
      adapter = "nixspace-west-app-v1";
      dependsOn = [ "build" ];
      argv = [
        "nixspace"
        "west"
        "run"
        "--"
        "env"
        "CUBS2_WORKSPACE_ROOT=."
        "nix"
        "run"
        "--no-pure-eval"
        "./${app}#native-sim-64-sil-test"
      ];
    };
  };

  twister-v1 = twisterActions;

  zros-v1 = project:
    let
      twister = twisterActions project;
      projectPath = westProjectPath project;
    in
    twister
    // {
      format = action {
        kind = "test";
        adapter = "zros-format-v1";
        toolProfile = "clang-tools-v1";
        argv = inWestManifestShellAt projectPath [
          "python3"
          "scripts/format.py"
          "--check"
        ];
      };
      build = twister.build // {
        dependsOn = [ "format" ];
      };
    };
}
