{
  description = "CogniPilot integration definition for synapse_ppm_bridge";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    source = {
      url = "github:CogniPilot/synapse_ppm_bridge/5fb6919e100f899cc7a029d6f71c06d1c9b89b16";
      flake = false;
    };
  };

  outputs =
    { nixpkgs, source, ... }:
    let
      manifest = builtins.fromTOML (builtins.readFile "${source}/Cargo.toml");
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
    in
    {
      flakeModules.default = import ./module.nix {
        softwareVersion = manifest.package.version;
      };

      packages = nixpkgs.lib.genAttrs systems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            inherit (manifest.package) version;
            pname = manifest.package.name;
            src = source;

            cargoLock.lockFile = "${source}/Cargo.lock";

            strictDeps = true;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = nixpkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.udev ];

            meta = {
              description = "Synapse ManualControl to PPM encoder serial bridge";
              homepage = "https://github.com/CogniPilot/synapse_ppm_bridge";
              license = with nixpkgs.lib.licenses; [
                asl20
                mit
              ];
              mainProgram = "synapse-ppm-bridge";
              platforms = systems;
            };
          };
        }
      );
    };
}
