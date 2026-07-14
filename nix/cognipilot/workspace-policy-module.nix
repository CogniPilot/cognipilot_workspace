{ config, lib, ... }:

let
  cfg = config.cognipilot.workspacePolicy;
  relativeFiles =
    if cfg.source == null then
      [ ]
    else
      map (
        path: lib.removePrefix "${toString cfg.source}/" (toString path)
      ) (lib.filesystem.listFilesRecursive cfg.source);
  allowedPython = path: lib.hasPrefix "tests/" path;
  allowedShellFiles = [
    ".envrc"
    "setup"
    "ws"
  ];
  shellSuffixes = [
    ".ash"
    ".bash"
    ".bat"
    ".cmd"
    ".csh"
    ".dash"
    ".fish"
    ".ksh"
    ".mksh"
    ".nu"
    ".ps1"
    ".sh"
    ".tcsh"
    ".yash"
    ".zsh"
  ];
  firstLine = path:
    builtins.head (
      lib.splitString "\n" (builtins.readFile (cfg.source + "/${path}"))
    );
  hasShellShebang = path:
    builtins.match "^#!.*(/|[[:space:]])(ash|bash|csh|dash|fish|ksh|mksh|nu|pwsh|sh|tcsh|yash|zsh)([[:space:]].*)?$" (firstLine path)
    != null;
  hasPythonShebang = path:
    builtins.match "^#!.*(/|[[:space:]])(python([0-9]+([.][0-9]+)*)?|uv)([[:space:]].*)?$" (firstLine path)
    != null;
  isShell = path:
    builtins.any (suffix: lib.hasSuffix suffix path) shellSuffixes
    || hasShellShebang path;
  isPython = path:
    lib.hasSuffix ".py" path
    || lib.hasSuffix ".pyw" path
    || hasPythonShebang path;
  forbiddenExact = [
    "devenv.lock"
    "devenv.nix"
    "devenv.yaml"
    "nix/components/default.nix"
    "nix/products.nix"
    "nix/tasks.nix"
    "workspace.lock.json"
  ];
  forbiddenPrefixes = [
    "completions/"
    "launch/"
    "scripts/"
  ];
  violations = builtins.filter (
    path:
    (isPython path && !(allowedPython path))
    || (isShell path && !(builtins.elem path allowedShellFiles))
    || builtins.elem path forbiddenExact
    || builtins.any (prefix: lib.hasPrefix prefix path) forbiddenPrefixes
  ) relativeFiles;
  report = {
    schemaVersion = 1;
    compliant = violations == [ ];
    policy = {
      python = "tests-only";
      shell = "exact-bootstrap-allowlist";
      inherit allowedShellFiles;
      staticAuthority = "nix";
    };
    inherit violations;
  };
  validatedReport =
    if violations == [ ] then
      report
    else
      throw ''
        workspace control-plane policy violations:
        - ${lib.concatStringsSep "\n- " violations}
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
        checks.cognipilot-workspace-policy = pkgs.writeText
          "cognipilot-workspace-policy.json"
          (builtins.toJSON config.cognipilot.workspacePolicy.report);
      };
  };
}
