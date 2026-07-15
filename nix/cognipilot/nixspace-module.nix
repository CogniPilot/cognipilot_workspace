{ config, lib, ... }:

{
  imports = [ ../nixspace/index-module.nix ];

  nixspace.index = config.cognipilot.nixspaceIndex;

  perSystem =
    { config, pkgs, ... }:
    let
      nixspace = config.packages.nixspace;
      planEnvironment = {
        NIXSPACE_INDEX = "${config.packages.nixspace-index}/share/nixspace/index.json";
      }
      // lib.optionalAttrs (config.packages ? nixspace-host-plan) {
        NIXSPACE_HOST_PLAN = "${config.packages.nixspace-host-plan}/share/nixspace/host-plan.json";
      }
      // lib.optionalAttrs (config.packages ? nixspace-launch-plan) {
        NIXSPACE_LAUNCH_PLAN = "${config.packages.nixspace-launch-plan}/share/nixspace/launch-plan.json";
      }
      // lib.optionalAttrs (config.packages ? nixspace-source-plan) {
        NIXSPACE_SOURCE_PLAN = "${config.packages.nixspace-source-plan}/share/nixspace/source-plan.json";
      }
      // lib.optionalAttrs (config.packages ? nixspace-west-plan) {
        NIXSPACE_WEST_PLAN = "${config.packages.nixspace-west-plan}/share/nixspace/west-plan.json";
      }
      // lib.optionalAttrs (config.packages ? nixspace-resolution-plan) {
        NIXSPACE_RESOLUTION_PLAN = "${config.packages.nixspace-resolution-plan}/share/nixspace/resolution-plan.json";
      }
      // lib.optionalAttrs (config.packages ? nixspace-benchmark-plan) {
        NIXSPACE_BENCHMARK_PLAN = "${config.packages.nixspace-benchmark-plan}/share/nixspace/benchmark-plan.json";
      };
      workspaceClient = pkgs.writeShellScriptBin "ws" ''
        ${lib.concatStringsSep "\n" (
          lib.mapAttrsToList (name: value: "export ${name}=${lib.escapeShellArg value}") planEnvironment
        )}
        exec ${lib.getExe nixspace} "$@"
      '';
    in
    {
      # CogniPilot's terse command is only a flake alias. The derivation still
      # delegates to the generic `nixspace` binary and supplies only generated
      # plan locations selected by this composition root.
      packages.ws = workspaceClient;
      apps.ws = {
        type = "app";
        program = "${workspaceClient}/bin/ws";
      };
    };
}
