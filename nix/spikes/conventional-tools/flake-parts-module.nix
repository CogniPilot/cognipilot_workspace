{ config, lib, ... }:
let
  inherit (lib) mkOption types;

  inputType = types.submodule {
    options = {
      fromProject = mkOption { type = types.str; };
      output = mkOption { type = types.str; };
    };
  };
  outputType = types.submodule {
    options.type = mkOption {
      type = types.enum [ "directory" "file" ];
    };
  };
  projectType = types.submodule {
    options = {
      kind = mkOption {
        type = types.enum [ "application" "source" ];
      };
      inputs = mkOption {
        type = types.attrsOf inputType;
      };
      outputs = mkOption {
        type = types.attrsOf outputType;
      };
    };
  };
in
{
  options.fixture.projects = mkOption {
    type = types.attrsOf projectType;
  };

  config = {
    fixture.projects = import ./fixture.nix;
    flake.flakePartsProjects = config.fixture.projects;
  };
}
