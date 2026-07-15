{ lib }:

{ files }:

let
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
  hasShellShebang =
    file:
    builtins.match "^#!.*(/|[[:space:]])(ash|bash|csh|dash|fish|ksh|mksh|nu|pwsh|sh|tcsh|yash|zsh)([[:space:]].*)?$" file.firstLine
    != null;
  hasPythonShebang =
    file:
    builtins.match "^#!.*(/|[[:space:]])(python([0-9]+([.][0-9]+)*)?|uv)([[:space:]].*)?$" file.firstLine
    != null;
  isShell =
    file: builtins.any (suffix: lib.hasSuffix suffix file.path) shellSuffixes || hasShellShebang file;
  isPython =
    file: lib.hasSuffix ".py" file.path || lib.hasSuffix ".pyw" file.path || hasPythonShebang file;
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
  violations = map (file: file.path) (
    builtins.filter (
      file:
      isPython file
      || (isShell file && !(builtins.elem file.path allowedShellFiles))
      || builtins.elem file.path forbiddenExact
      || builtins.any (prefix: lib.hasPrefix prefix file.path) forbiddenPrefixes
    ) files
  );
in
{
  schemaVersion = 1;
  compliant = violations == [ ];
  policy = {
    # Project-native Python is intentionally outside this workspace source and
    # remains owned by its separately selected project input.
    python = "project-native-under-src-only";
    workspaceSourcePython = "forbidden";
    projectNativePython = "separate-project-inputs";
    shell = "exact-bootstrap-allowlist";
    inherit allowedShellFiles;
    staticAuthority = "nix";
  };
  inherit violations;
}
