{
  description = "Tiny conventional package provider";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
    in
    {
      packages.${system} = {
        alpha-release =
          pkgs.linkFarm "alpha-release" [
            {
              name = "bin/alpha-runtime";
              path = pkgs.writeShellScript "alpha-runtime" ''
                printf 'alpha immutable release:%s\n' "$*"
              '';
            }
            {
              name = "share/alpha/config.json";
              path = pkgs.writeText "alpha-config.json" ''
                {"source":"immutable-release"}
              '';
            }
          ]
          // {
            version = "1.2.3";
          };
        beta-firmware = pkgs.writeText "beta-firmware" "beta immutable firmware\n" // {
          version = "4.5.6";
        };
      };
    };
}
