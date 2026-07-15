{ pkgs }:

let
  lib = pkgs.lib;
  contract = ../../../nix/cognipilot/flake-module.nix;
  projectOutput = ../../../nix/cognipilot/project-flake-module.nix;
  project = {
    repositoryId = "repo";
    source.input = "source";
    preset = "cargo-v1";
  };
  evaluate =
    projects:
    builtins.tryEval (
      builtins.deepSeq
        (lib.evalModules {
          modules = [
            {
              options = {
                flake = lib.mkOption {
                  type = lib.types.attrs;
                  default = { };
                };
                perSystem = lib.mkOption {
                  type = lib.types.raw;
                };
              };
            }
            contract
            projectOutput
            { cognipilot.projects = projects; }
          ];
        }).config.flake.nixspaceIndex
        true
    );
  zero = evaluate { };
  one = evaluate { one = project; };
  multiple = evaluate {
    one = project;
    two = project;
  };
in
if !zero.success && one.success && !multiple.success then
  {
    zero-rejected = true;
    one-accepted = true;
    multiple-rejected = true;
  }
else
  throw "standalone project count checks failed"
