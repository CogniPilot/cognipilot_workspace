{ lib }:

let
  definitions = {
    launch-common = {
      public = false;
      description = "Shared process-manager configuration.";
      processes = [ ];
      profile.module = import ./common.nix;
    };

    simulation = {
      public = true;
      description = "Fake Synapse vehicle publisher.";
      processes = [ "simulation" ];
      profile = {
        extends = [ "launch-common" ];
        module = import ./simulation.nix;
      };
    };

    ground-station = {
      public = true;
      description = "Electrode Ground Station daemon and web UI.";
      processes = [ "ground-station" ];
      profile = {
        extends = [ "launch-common" ];
        module = import ./ground-station.nix;
      };
    };

    mocap = {
      public = true;
      description = "Synapse Qualisys bridge and dashboard.";
      processes = [ "mocap" ];
      profile = {
        extends = [ "launch-common" ];
        module = import ./mocap.nix;
      };
    };

    simulation-stack = {
      public = true;
      description = "Simulation, ground station, and mocap bridge.";
      processes = [
        "simulation"
        "ground-station"
        "mocap"
      ];
      profile.extends = [
        "simulation"
        "ground-station"
        "mocap"
      ];
    };
  };
  publicDefinitions = lib.filterAttrs (_: definition: definition.public) definitions;
in
{
  profiles = lib.mapAttrs (_: definition: definition.profile) definitions;
  manifest = lib.mapAttrs (_: definition: {
    inherit (definition) description processes;
  }) publicDefinitions;
}
