{ lib, ... }:

let
  manifestPath = ../../tools/nixspace/Cargo.toml;
  lockPath = ../../tools/nixspace/Cargo.lock;
  manifest = builtins.fromTOML (builtins.readFile manifestPath);
  lock = builtins.fromTOML (builtins.readFile lockPath);
  dependencyNames = builtins.attrNames manifest.dependencies;
  allowedDependencies = [
    "base64"
    "clap"
    "clap_complete"
    "fs4"
    "miette"
    "nix-base32"
    "serde"
    "serde_json"
  ];
  registryDependency =
    dependency:
    builtins.isString dependency
    || (dependency ? version && !(dependency ? path) && !(dependency ? git));
  lockedFs4 = builtins.filter (package: package.name == "fs4") lock.package;
  registryLockedClosure = builtins.all (
    package:
    package.name == "nixspace"
    || (package ? source && lib.hasPrefix "registry+https://github.com/rust-lang/crates.io-index" package.source)
  ) lock.package;
  standaloneManifest =
    !(manifest ? workspace)
    && manifest.package.name == "nixspace"
    &&
      manifest.bin == [
        {
          name = "nixspace";
          path = "src/main.rs";
        }
      ]
    && dependencyNames == allowedDependencies
    && builtins.all registryDependency (builtins.attrValues manifest.dependencies);
  standaloneLock =
    registryLockedClosure
    && lockedFs4 == [
      {
        name = "fs4";
        version = "1.1.0";
        source = "registry+https://github.com/rust-lang/crates.io-index";
        checksum = "7e72ed92b67c146290f88e9c89d60ca163ea417a446f61ffd7b72df3e7f1dfd5";
        dependencies = [
          "rustix"
          "windows-sys"
        ];
      }
    ];
in
assert standaloneManifest;
assert standaloneLock;
{
  perSystem =
    { pkgs, ... }:
    let
      nixspaceSource = lib.cleanSourceWith {
        name = "nixspace-source";
        src = ../../tools/nixspace;
        filter =
          path: type:
          let
            relative = lib.removePrefix "${toString ../../tools/nixspace}/" (toString path);
          in
          type != "directory" || relative != "target";
      };
      nixspace = pkgs.rustPlatform.buildRustPackage {
        pname = "nixspace";
        inherit (manifest.package) version;
        src = nixspaceSource;
        cargoLock.lockFile = lockPath;
        strictDeps = true;
        doCheck = true;
        nativeCheckInputs = [ pkgs.git ];
        meta = {
          description = manifest.package.description;
          license = lib.licenses.asl20;
          mainProgram = "nixspace";
        };
        passthru = {
          sourceScope = "tools/nixspace";
          normalizedInterfaceVersion = 2;
        };
      };
      # The module-level assertions above validate the independent manifest
      # and lock before any output exists. Building the package itself is the
      # corresponding Nix check; Cargo's package integration tests provide the
      # archive/installability proof without a second shell implementation.
      standaloneCheck = nixspace;
      completions = pkgs.runCommand "nixspace-completions" { nativeBuildInputs = [ nixspace ]; } ''
        mkdir -p \
          "$out/share/bash-completion/completions" \
          "$out/share/fish/vendor_completions.d" \
          "$out/share/zsh/site-functions"
        nixspace completion bash > "$out/share/bash-completion/completions/nixspace"
        nixspace completion fish > "$out/share/fish/vendor_completions.d/nixspace.fish"
        nixspace completion zsh > "$out/share/zsh/site-functions/_nixspace"
      '';
    in
    {
      packages.nixspace = nixspace;
      packages.nixspace-completions = completions;
      apps.nixspace = {
        type = "app";
        program = "${nixspace}/bin/nixspace";
      };
      checks.nixspace-standalone = standaloneCheck;
    };
}
