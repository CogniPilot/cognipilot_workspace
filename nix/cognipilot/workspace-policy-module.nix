{ config, lib, ... }:

let
  cfg = config.cognipilot.workspacePolicy;
  evaluatePolicy = import ./workspace-policy.nix { inherit lib; };
  relativeFiles =
    if cfg.source == null then
      [ ]
    else
      map (path: lib.removePrefix "${toString cfg.source}/" (toString path)) (
        lib.filesystem.listFilesRecursive cfg.source
      );
  files = map (path: {
    inherit path;
    firstLine = builtins.head (lib.splitString "\n" (builtins.readFile (cfg.source + "/${path}")));
  }) relativeFiles;
  report = evaluatePolicy { inherit files; };
  validatedReport =
    if report.violations == [ ] then
      report
    else
      throw ''
        workspace control-plane policy violations:
        - ${lib.concatStringsSep "\n- " report.violations}
      '';
in
{
  options.cognipilot.workspacePolicy = {
    source = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = ''
        Git-filtered workspace source checked for forbidden legacy control-plane
        implementations. Project-native sources are separate flake inputs and
        are not constrained by this root policy.
      '';
    };
    report = lib.mkOption {
      type = lib.types.attrs;
      readOnly = true;
      description = "Validated workspace control-plane policy report.";
    };
  };

  config = lib.mkIf (cfg.source != null) {
    cognipilot.workspacePolicy.report = validatedReport;

    perSystem =
      { pkgs, ... }:
      {
        checks.cognipilot-workspace-policy = pkgs.writeText "cognipilot-workspace-policy.json" (
          builtins.toJSON config.cognipilot.workspacePolicy.report
        );
      };
  };
}
