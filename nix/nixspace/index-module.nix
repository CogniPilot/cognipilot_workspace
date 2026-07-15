{
  config,
  lib,
  ...
}:

let
  cfg = config.nixspace;
  document =
    if cfg.index == null then
      throw "nixspace.index must be set by a workspace provider"
    else
      cfg.index
      // lib.optionalAttrs (!(cfg.index ? apiVersion)) {
        apiVersion = "nixspace/v1";
      }
      // lib.optionalAttrs (!(cfg.index ? kind)) {
        kind = "Workspace";
      };
  validatedDocument =
    if document.apiVersion != "nixspace/v1" then
      throw "nixspace.index.apiVersion must be `nixspace/v1`"
    else if document.kind != "Workspace" then
      throw "nixspace.index.kind must be `Workspace`"
    else if (document.interfaceVersion or null) != 2 then
      throw "nixspace.index.interfaceVersion must be 2"
    else
      document;
in
{
  imports = [ ./tool-module.nix ];

  options.nixspace.index = lib.mkOption {
    type = lib.types.nullOr lib.types.attrs;
    default = null;
    description = ''
      Provider-generated static workspace document. This generic module adds
      the versioned envelope when absent and publishes the normalized index.
    '';
  };

  config = {
    flake.nixspaceIndex = validatedDocument;

    perSystem =
      { config, pkgs, ... }:
      let
        indexPackage = pkgs.writeTextDir "share/nixspace/index.json" (
          builtins.toJSON validatedDocument
        );
      in
      {
        packages.nixspace-index = indexPackage;

        checks.nixspace-interface =
          pkgs.runCommand "nixspace-interface"
            {
              nativeBuildInputs = [
                config.packages.nixspace
                pkgs.jq
              ];
            }
            ''
              nixspace --index ${indexPackage}/share/nixspace/index.json package list --json \
                | jq -e '.interfaceVersion == 2 and .count == (.packages | length)' >/dev/null
              nixspace --index ${indexPackage}/share/nixspace/index.json graph --json \
                | jq -e '(.nodes | type) == "array" and (.edges | type) == "array"' >/dev/null
              touch "$out"
            '';
      };
  };
}
