{ lib }:

let
  binPrefixes = packages: map (package: "${lib.getBin package}/bin") packages;
  pkgConfigPath = packages:
    lib.concatStringsSep ":" [
      (lib.makeSearchPathOutput "dev" "lib/pkgconfig" packages)
      (lib.makeSearchPathOutput "out" "lib/pkgconfig" packages)
      (lib.makeSearchPathOutput "dev" "share/pkgconfig" packages)
      (lib.makeSearchPathOutput "out" "share/pkgconfig" packages)
    ];
  profile = tools: environment: {
    pathPrefixes = binPrefixes tools;
    inherit environment;
    environmentPaths = { };
  };
  rustLibudevProfile = pkgs:
    let
      nativeLibraries = lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.udev ];
    in
    profile [
      pkgs.cargo
      pkgs.rustc
      pkgs.pkg-config
    ] (lib.optionalAttrs (nativeLibraries != [ ]) {
      PKG_CONFIG_PATH = pkgConfigPath nativeLibraries;
    });
in
{
  # These are versioned, reusable action environments rather than package
  # shells.  Presets select them once; project definitions do not repeat
  # package lists or store paths.
  clang-tools-v1 = pkgs:
    profile [ pkgs.clang-tools ] { };

  cmake-v1 = pkgs:
    profile [
      pkgs.cmake
      pkgs.ninja
      pkgs.stdenv.cc
    ] { };

  # Cargo actions share one workspace cache locally; CI may select sccache's
  # conventional GitHub Actions backend without changing project definitions.
  rust-libudev-sccache-v1 = pkgs:
    (rustLibudevProfile pkgs)
    // {
      pathPrefixes = binPrefixes [
        pkgs.cargo
        pkgs.rustc
        pkgs.pkg-config
        pkgs.sccache
      ];
      environment = (rustLibudevProfile pkgs).environment // {
        CARGO_INCREMENTAL = "0";
        RUSTC_WRAPPER = lib.getExe pkgs.sccache;
      };
      # Resolve this through ActionTask.environmentPaths so one cache is shared
      # by Cargo actions across project working directories without embedding
      # the checkout's absolute path in Nix data.
      environmentPaths.SCCACHE_DIR = ".nixspace/state/sccache";
    };

  meson-glib-cjson-v1 = pkgs:
    let
      nativeLibraries = [
        pkgs.glib
        pkgs.cjson
      ];
    in
    profile [
      pkgs.meson
      pkgs.ninja
      pkgs.pkg-config
      pkgs.stdenv.cc
    ] {
      PKG_CONFIG_PATH = pkgConfigPath nativeLibraries;
    };

  # Opt-in profile for project-owned QEMU provisioning actions. Keeping the
  # expensive closure separate means ordinary Meson edits do not realize it.
  fastdyn-qemu-v1 = pkgs:
    let
      nativeLibraries = [
        pkgs.cjson
        pkgs.expat
        pkgs.glib
        pkgs.pixman
        pkgs.zlib
      ];
    in
    profile (with pkgs; [
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
      rustc
      universal-ctags
      zlib
    ]) {
      PYTHONPATH = "${pkgs.python3Packages.distlib}/${pkgs.python3.sitePackages}";
      LD_LIBRARY_PATH = lib.makeLibraryPath [
        pkgs.stdenv.cc.cc.lib
        pkgs.zlib
      ];
      PKG_CONFIG_PATH = pkgConfigPath nativeLibraries;
    };
}
