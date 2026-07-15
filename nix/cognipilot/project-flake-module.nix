{
  config,
  lib,
  ...
}:

let
  rootConfig = config;
  normalizedIndex = rootConfig.cognipilot.validatedIndex;
  projectIds = builtins.attrNames normalizedIndex.projects;
  standaloneIndex =
    if builtins.length projectIds == 1 then
      rootConfig.cognipilot.nixspaceIndex
    else
      throw ''
        CogniPilot standalone project output requires exactly one selected project;
        found ${toString (builtins.length projectIds)}: ${
          if projectIds == [ ] then "<none>" else lib.concatStringsSep ", " projectIds
        }
      '';
in
{
  imports = [ ../nixspace/index-module.nix ];

  config = {
    # Standalone project roots are workspace providers with exactly one
    # selected project. The generic module owns the emitted protocol outputs.
    nixspace.index = standaloneIndex;
    flake.cognipilotIndex = normalizedIndex;

    perSystem =
      { config, pkgs, ... }:
      let
        nixspaceIndexPackage = config.packages.nixspace-index;
        showIndex = pkgs.writeShellScript "nixspace-show-index" ''
          exec ${pkgs.coreutils}/bin/cat ${nixspaceIndexPackage}/share/nixspace/index.json
        '';
      in
      {
        devShells.default = pkgs.mkShellNoCC {
          packages = [
            pkgs.jq
            pkgs.nixfmt-rfc-style
          ];
        };

        apps.show-index = {
          type = "app";
          program = "${showIndex}";
        };

        formatter = pkgs.nixfmt-rfc-style;
      };
  };
}
