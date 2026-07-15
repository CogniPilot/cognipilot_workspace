let
  pkgs = import <nixpkgs> { };
  lib = pkgs.lib;
  root = ../../..;

  inputs.demo_source = builtins.path {
    path = ./source;
    name = "nixspace-west-plan-demo-source";
  };

  support.options = {
    flake = lib.mkOption {
      type = lib.types.attrs;
      default = { };
    };
    perSystem = lib.mkOption { type = lib.types.raw; };
  };

  modules = [
    support
    (root + /nix/nixspace/west-workspace-module.nix)
  ];

  evaluate =
    extra:
    lib.evalModules {
      specialArgs = { inherit inputs; };
      modules = modules ++ [ extra ];
    };

  base = {
    nixspace.west = {
      enable = true;
      product = {
        id = "demo-product";
        interfaceVersion = 3;
      };
      workspace = {
        id = "demo-app";
        source = {
          input = "demo_source";
          root = ".";
        };
        manifest = {
          resource = "demo-app:west-manifest";
          relativePath = "west.yml";
        };
      };
      localOverrides = {
        shared_module = {
          source = "src/shared_module";
          zephyrModule = true;
        };
        model_data.source = "src/model_data";
      };
    };
  };

  evaluated = evaluate base;
  plan = evaluated.config.flake.nixspaceWestPlan;

  unsafe = evaluate (
    lib.recursiveUpdate base {
      nixspace.west.localOverrides.shared_module.source = "../outside";
    }
  );
  unsafeResult = builtins.tryEval (builtins.deepSeq unsafe.config.flake.nixspaceWestPlan true);
  injectedConfig = evaluate (
    lib.recursiveUpdate base {
      nixspace.west.workspace.manifest.relativePath = "west.yml\n[other]";
    }
  );
  injectedConfigResult = builtins.tryEval (
    builtins.deepSeq injectedConfig.config.flake.nixspaceWestPlan true
  );
in
assert plan.apiVersion == "nixspace/v1";
assert plan.kind == "WestWorkspace";
assert plan.interfaceVersion == 2;
assert
  plan.product == {
    id = "demo-product";
    interfaceVersion = 3;
  };
assert plan.workspace.id == "demo-app";
assert plan.workspace.source.input == "demo_source";
assert plan.workspace.source.root == ".";
assert plan.workspace.manifest.resource == "demo-app:west-manifest";
assert plan.workspace.manifest.relativePath == "west.yml";
assert lib.hasPrefix "/nix/store/" plan.workspace.manifest.storePath;
assert builtins.stringLength plan.workspace.contentKey == 64;
assert builtins.stringLength plan.workspace.manifest.sha256 == 64;
assert !(plan.workspace ? projects);
assert
  map (override: override.project) plan.localView.overrides == [
    "model_data"
    "shared_module"
  ];
assert
  plan.cache == {
    layoutVersion = 2;
    namespace = "demo-product";
    root = {
      base = "platform-cache";
      path = "nixspace";
    };
    narrowUpdate = true;
    nativePathCache = true;
    paths = {
      generations = "demo-product/west/workspaces/${plan.workspace.contentKey}/generations";
      generationGcRoot = "locked";
      current = "demo-product/west/workspaces/${plan.workspace.contentKey}/current.json";
      publicationLock = "demo-product/west/locks/${plan.workspace.contentKey}.lock";
    };
  };
assert
  plan.localView.paths == {
    generations = "demo-product/${plan.workspace.contentKey}/${plan.localView.policyId}/generations";
    executionLock = "demo-product/${plan.workspace.contentKey}/${plan.localView.policyId}/execution.lock";
  };
assert
  plan.localView.root == {
    base = "workspace";
    path = ".nixspace/state/west/views";
  };
assert !unsafeResult.success;
assert !injectedConfigResult.success;
{
  success = true;
  inherit (plan) apiVersion kind;
  interfaceVersion = plan.interfaceVersion;
  manifestResource = plan.workspace.manifest.resource;
  projectGraphDuplicated = plan.workspace ? projects;
}
