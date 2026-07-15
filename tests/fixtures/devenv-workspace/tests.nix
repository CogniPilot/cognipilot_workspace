{ pkgs, devenvSource }:

let
  lib = pkgs.lib;
  pinnedModules = devenvSource + "/src/modules";

  rootSupport =
    { lib, ... }:
    {
      options = {
        flake.nixspaceIndex = lib.mkOption {
          type = lib.types.attrs;
        };
        nixspace.west.enable = lib.mkOption {
          type = lib.types.bool;
          default = false;
        };
        perSystem = lib.mkOption {
          type = lib.types.deferredModule;
        };
      };
    };

  mkRoot =
    {
      hostPlanEnabled ? false,
      sourcePlanEnabled ? false,
      westEnabled ? false,
    }:
    lib.evalModules {
      modules = [
        rootSupport
        ../project-flakes/golden/launch-ir.nix
        ../../../nix/cognipilot/devenv-task-module.nix
        ../../../nix/cognipilot/devenv-launch-module.nix
        ../../../nix/cognipilot/devenv-workspace-module.nix
        ../../../nix/cognipilot/nixspace-module.nix
        {
          cognipilot.devenvWorkspace = {
            enable = true;
            name = "fixture-workspace";
            workspaceRoot = "/workspace";
          };
          cognipilot.devenvLaunches.executableBindings = {
            "app:router" = "/nix/store/cognipilot-test-router/bin/router";
            "app:monitor" = "/nix/store/cognipilot-test-monitor/bin/monitor";
          };
          nixspace.west.enable = westEnabled;
          perSystem =
            { pkgs, ... }:
            {
              packages =
                lib.optionalAttrs westEnabled {
                  nixspace-west-plan = pkgs.writeTextDir "share/nixspace/west-plan.json" "{}";
                }
                // lib.optionalAttrs hostPlanEnabled {
                  nixspace-host-plan = pkgs.writeTextDir "share/nixspace/host-plan.json" "{}";
                }
                // lib.optionalAttrs sourcePlanEnabled {
                  nixspace-source-plan = pkgs.writeTextDir "share/nixspace/source-plan.json" "{}";
                };
            };
        }
      ];
    };

  perSystemSupport =
    { config, lib, ... }:
    let
      mkDevenvEval =
        modules:
        lib.evalModules {
          class = "devenv";
          specialArgs = {
            inherit pkgs;
            inputs = { };
          };
          modules = [
            (pinnedModules + "/top-level.nix")
            {
              _module.args.pkgs = pkgs;
              # This fixture validates the upstream module contract, not
              # Devenv's private package resolver.  Supplying a package keeps
              # evaluation pure; production binds the exact package exported
              # by the pinned Devenv flake input.
              task.package = pkgs.writeShellScriptBin "devenv-tasks" "exit 0";
              devenv = {
                flakesIntegration = true;
                root = "/workspace";
                warnOnNewVersion = false;
              };
            }
          ]
          ++ modules;
        };
      devenvType = (mkDevenvEval config.devenv.modules).type;
    in
    {
      options = {
        apps = lib.mkOption {
          type = lib.types.lazyAttrsOf lib.types.attrs;
          default = { };
        };
        checks = lib.mkOption {
          type = lib.types.lazyAttrsOf lib.types.package;
          default = { };
        };
        devShells = lib.mkOption {
          type = lib.types.lazyAttrsOf lib.types.package;
          default = { };
        };
        formatter = lib.mkOption {
          type = lib.types.nullOr lib.types.package;
          default = null;
        };
        packages = lib.mkOption {
          type = lib.types.lazyAttrsOf lib.types.package;
          default = { };
        };
        devenv.modules = lib.mkOption {
          type = lib.types.listOf lib.types.deferredModule;
          default = [ ];
        };
        devenv.shells = lib.mkOption {
          type = lib.types.lazyAttrsOf devenvType;
          default = { };
        };
      };

      config.devShells = lib.mapAttrs (_: shell: shell.shell) config.devenv.shells;
    };

  evalPerSystem =
    root:
    lib.evalModules {
      specialArgs = { inherit pkgs; };
      modules = [
        perSystemSupport
        root.config.perSystem
      ];
    };

  currentRoot = mkRoot { westEnabled = true; };
  current = (evalPerSystem currentRoot).config;
  withPlans =
    (evalPerSystem (mkRoot {
      hostPlanEnabled = true;
      sourcePlanEnabled = true;
    })).config;
  invalidIndex =
    overrides:
    builtins.tryEval (
      builtins.deepSeq
        (lib.evalModules {
          modules = [
            rootSupport
            ../../../nix/nixspace/index-module.nix
            {
              nixspace.index = currentRoot.config.cognipilot.validatedIndex // overrides;
            }
          ];
        }).config.flake.nixspaceIndex
        true
    );

  nonInternalTasks = shell: lib.filterAttrs (name: _: !(lib.hasPrefix "devenv:" name)) shell.tasks;
  defaultShell = current.devenv.shells.default;
  routerShell = current.devenv.shells."launch-app--router";
  stackShell = current.devenv.shells."launch-app--stack";
  checks = {
    pinned-module-version = defaultShell.devenv.latestVersion == "2.1.2";
    flake-integration-task-runner =
      defaultShell.devenv.cli.version == null
      && defaultShell.task.package != null
      && defaultShell.env.DEVENV_TASKS == ""
      && lib.hasPrefix "/nix/store/" (toString defaultShell.task.config)
      && lib.hasPrefix "/nix/store/" (toString defaultShell.procfileScript);
    minimal-default-shell =
      builtins.elem current.packages.nixspace defaultShell.packages
      && builtins.elem current.packages.nixspace-completions defaultShell.packages
      && builtins.elem pkgs.git defaultShell.packages;
    every-launch-gets-client =
      builtins.elem current.packages.nixspace routerShell.packages
      && builtins.elem current.packages.nixspace-completions routerShell.packages
      && builtins.elem current.packages.nixspace stackShell.packages
      && builtins.elem current.packages.nixspace-completions stackShell.packages;
    public-caches =
      builtins.elem "cognipilot" defaultShell.cachix.pull
      && builtins.elem "devenv" defaultShell.cachix.pull;
    generated-index-is-authoritative =
      lib.hasPrefix "/nix/store/" defaultShell.env.NIXSPACE_INDEX
      && current.packages.nixspace-index.passthru.document == currentRoot.config.flake.nixspaceIndex;
    generic-index-envelope =
      currentRoot.config.flake.nixspaceIndex.apiVersion == "nixspace/v1"
      && currentRoot.config.flake.nixspaceIndex.kind == "Workspace"
      && currentRoot.config.flake.nixspaceIndex.interfaceVersion == 2;
    generic-runtime-environment =
      defaultShell.env.NIXSPACE_WORKSPACE_ROOT == "/workspace"
      && !(defaultShell.env ? NIXSPACE_HOST_PLAN)
      && !(defaultShell.env ? NIXSPACE_SOURCE_PLAN)
      && !(defaultShell.env ? COGNIPILOT_PROJECT_INDEX)
      && !(defaultShell.env ? COGNIPILOT_WORKSPACE_ROOT);
    generated-source-and-host-plans-are-bound =
      withPlans.devenv.shells.default.env.NIXSPACE_HOST_PLAN
      == "${withPlans.packages.nixspace-host-plan}/share/nixspace/host-plan.json"
      &&
        withPlans.devenv.shells.default.env.NIXSPACE_SOURCE_PLAN
        == "${withPlans.packages.nixspace-source-plan}/share/nixspace/source-plan.json";
    generated-west-plan-is-bound =
      defaultShell.env.NIXSPACE_WEST_PLAN
      == "${current.packages.nixspace-west-plan}/share/nixspace/west-plan.json";
    generated-shells =
      builtins.attrNames current.devenv.shells == [
        "default"
        "launch-app--router"
        "launch-app--stack"
      ];
    generated-apps =
      builtins.attrNames current.apps == [
        "launch-app--router"
        "launch-app--stack"
        "nixspace"
        "ws"
      ];
    upstream-process-manager =
      routerShell.process.manager.implementation == "process-compose"
      &&
        builtins.attrNames routerShell.processes == [
          "monitor"
          "router"
        ]
      &&
        builtins.attrNames stackShell.processes == [
          "base--monitor"
          "base--router"
        ];
    apps-use-upstream-launcher =
      current.apps."launch-app--router".program == toString routerShell.procfileScript
      && current.apps."launch-app--stack".program == toString stackShell.procfileScript;
    workspace-unconditionally-enables-generated-tasks = builtins.hasAttr "app:default:build" (
      nonInternalTasks defaultShell
    );
    generic-index-rejects-wrong-api = !(invalidIndex { apiVersion = "other/v1"; }).success;
    generic-index-rejects-wrong-kind = !(invalidIndex { kind = "Other"; }).success;
    generic-index-rejects-wrong-interface = !(invalidIndex { interfaceVersion = 1; }).success;
  };
  failures = builtins.attrNames (lib.filterAttrs (_: passed: !passed) checks);
in
if failures == [ ] then
  checks
else
  throw "devenv workspace integration checks failed: ${lib.concatStringsSep ", " failures}"
