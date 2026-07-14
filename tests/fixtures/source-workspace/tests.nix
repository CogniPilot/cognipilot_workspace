let
  pkgs = import <nixpkgs> { };
  lib = pkgs.lib;
  evaluated = lib.evalModules {
    specialArgs.inputs.self.outPath = ./.;
    modules = [
      {
        options = {
          flake.nixspaceSourcePlan = lib.mkOption {
            type = lib.types.attrs;
          };
          perSystem = lib.mkOption {
            type = lib.types.deferredModule;
          };
        };
      }
      ../project-flakes/golden/semantic-dag.nix
      ../../../nix/cognipilot/source-workspace-module.nix
      {
        cognipilot.sourceWorkspace.enable = true;
      }
    ];
  };
  plan = evaluated.config.flake.nixspaceSourcePlan;
in
assert plan.apiVersion == "nixspace/v1";
assert plan.kind == "SourceWorkspace";
assert plan.interfaceVersion == 1;
assert plan.workspaceRoot == ".";
assert plan.plans.all == [
  "codegen"
  "flight"
];
assert plan.plans.default == plan.plans.all;
assert plan.plans.packages.codegen == [ "codegen" ];
assert builtins.sort builtins.lessThan plan.plans.packages.flight == [
  "codegen"
  "flight"
];
assert plan.repositories.codegen.path == "src/codegen";
assert plan.repositories.codegen.git.url == "https://github.com/ExampleRobotics/codegen.git";
assert plan.repositories.codegen.git.branch == "main";
assert plan.repositories.codegen.git.clone.argv == [
  "git"
  "clone"
  "--origin"
  "origin"
  "--branch"
  "main"
  "--"
  "https://github.com/ExampleRobotics/codegen.git"
  "src/codegen"
];
assert plan.repositories.codegen.git.inspect.worktree.argv == [
  "git"
  "-C"
  "src/codegen"
  "rev-parse"
  "--show-toplevel"
];
assert plan.repositories.codegen.git.inspect.origin.argv == [
  "git"
  "-C"
  "src/codegen"
  "remote"
  "get-url"
  "origin"
];
assert plan.repositories.codegen.git.inspect.branch.argv == [
  "git"
  "-C"
  "src/codegen"
  "symbolic-ref"
  "--quiet"
  "--short"
  "HEAD"
];
assert plan.repositories.codegen.git.inspect.clean.argv == [
  "git"
  "-C"
  "src/codegen"
  "status"
  "--porcelain=v1"
  "--untracked-files=normal"
];
assert plan.repositories.flight.git.branch == "develop";
assert plan.repositories.flight.git.fastForwardCheck.argv == [
  "git"
  "-C"
  "src/flight"
  "merge-base"
  "--is-ancestor"
  "HEAD"
  "origin/develop"
];
assert plan.repositories.flight.git.fastForward.argv == [
  "git"
  "-C"
  "src/flight"
  "merge"
  "--ff-only"
  "origin/develop"
];
assert plan.repositories.flight.source.locked.rev == "2222222222222222222222222222222222222222";
{
  success = true;
  repositories = builtins.attrNames plan.repositories;
  flightPlan = builtins.sort builtins.lessThan plan.plans.packages.flight;
}
