{
  count,
  lib,
  module,
}:

let
  projects = builtins.listToAttrs (
    builtins.genList (index: {
      name = "project${toString index}";
      value = {
        preset = "cargo-locked-v1";
        source.visibility = "public";
      };
    }) count
  );
  evaluated = lib.evalModules {
    modules = [
      module
      { cognipilot.projects = projects; }
    ];
  };
in
evaluated.config.cognipilot.validatedIndex
