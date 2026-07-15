{ pkgs }:

let
  inherit (pkgs) lib;
  evaluate = import ../../../nix/cognipilot/workspace-policy.nix { inherit lib; };
  file = path: firstLine: { inherit path firstLine; };
  workspaceFiles = [
    (file ".envrc" "#!/usr/bin/env bash")
    (file "setup" "#!/bin/sh")
    (file "ws" "#!/bin/sh")
    (file "README.md" "# Nix workspace")
    (file "nix/workspace.nix" "{ lib, ... }:")
    (file "tools/nixspace/src/main.rs" "use clap::Parser;")
  ];
  projectInputFiles = [
    (file "src/native_tool.py" "from project import native_api")
  ];
  allowed = evaluate { files = workspaceFiles; };
  rootPython = evaluate {
    files = workspaceFiles ++ [ (file "workspace.py" "raise SystemExit") ];
  };
  srcPython = evaluate {
    files = workspaceFiles ++ [ (file "src/workspace.py" "raise SystemExit") ];
  };
  windowsPython = evaluate {
    files = workspaceFiles ++ [ (file "src/workspace.pyw" "raise SystemExit") ];
  };
  shebangPython = evaluate {
    files = workspaceFiles ++ [ (file "src/workspace-tool" "#!/usr/bin/env -S python3 -I") ];
  };
  rogueShell = evaluate {
    files = workspaceFiles ++ [ (file "scripts/hidden" "#!/usr/bin/env bash") ];
  };

  tests = {
    workspaceSourceAllowsOnlyNixRustAndExactBootstrapShell = {
      expr = allowed;
      expected = {
        schemaVersion = 1;
        compliant = true;
        policy = {
          python = "project-native-under-src-only";
          workspaceSourcePython = "forbidden";
          projectNativePython = "separate-project-inputs";
          shell = "exact-bootstrap-allowlist";
          allowedShellFiles = [
            ".envrc"
            "setup"
            "ws"
          ];
          staticAuthority = "nix";
        };
        violations = [ ];
      };
    };
    trackedPythonCannotHideUnderSrc = {
      expr = {
        root = rootPython.violations;
        src = srcPython.violations;
        windows = windowsPython.violations;
        shebang = shebangPython.violations;
      };
      expected = {
        root = [ "workspace.py" ];
        src = [ "src/workspace.py" ];
        windows = [ "src/workspace.pyw" ];
        shebang = [ "src/workspace-tool" ];
      };
    };
    projectNativePythonStaysOutsideWorkspacePolicyInput = {
      expr = {
        workspaceCompliant = allowed.compliant;
        projectInput = projectInputFiles;
        projectInputScanned = builtins.any (
          projectFile: builtins.elem projectFile.path allowed.violations
        ) projectInputFiles;
      };
      expected = {
        workspaceCompliant = true;
        projectInput = [
          {
            path = "src/native_tool.py";
            firstLine = "from project import native_api";
          }
        ];
        projectInputScanned = false;
      };
    };
    shellOrchestrationStillFailsClosed = {
      expr = rogueShell.violations;
      expected = [ "scripts/hidden" ];
    };
  };
  failures = lib.runTests tests;
in
if failures == [ ] then
  {
    suite = "workspace-policy";
    assertionCount = builtins.length (builtins.attrNames tests);
    rejectedPythonCount = 4;
  }
else
  throw "workspace policy contract failures: ${builtins.toJSON failures}"
