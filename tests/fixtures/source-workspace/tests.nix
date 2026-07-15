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
  transactionPathResult = value: builtins.tryEval (
    builtins.deepSeq (
      (lib.evalModules {
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
            cognipilot.sourceWorkspace = {
              enable = true;
              mutationLockPath = value;
            };
          }
        ];
      }).config.flake.nixspaceSourcePlan
    ) true
  );
  absoluteTransactionPath = transactionPathResult "/tmp/source.lock";
  driveTransactionPath = transactionPathResult "C:/source.lock";
  backslashTransactionPath = transactionPathResult "state\\source.lock";
in
assert plan.apiVersion == "nixspace/v1";
assert plan.kind == "SourceWorkspace";
assert plan.interfaceVersion == 3;
assert plan.workspaceRoot == ".";
assert plan.transaction == {
  mutationLock = ".nixspace/source-mutation.lock";
  journal = ".nixspace/source-update-transaction.json";
};
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
assert plan.repositories.codegen.git.inspect.head.argv == [
  "git"
  "-C"
  "src/codegen"
  "rev-parse"
  "--verify"
  "HEAD"
];
assert plan.repositories.codegen.git.inspect.target.argv == [
  "git"
  "-C"
  "src/codegen"
  "rev-parse"
  "--verify"
  "origin/main"
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
assert plan.repositories.flight.git.rollback.refUpdate.argv == [
  {
    kind = "literal";
    value = "git";
  }
  {
    kind = "literal";
    value = "-C";
  }
  {
    kind = "literal";
    value = "src/flight";
  }
  {
    kind = "literal";
    value = "update-ref";
  }
  {
    kind = "literal";
    value = "HEAD";
  }
  { kind = "old-head"; }
  { kind = "expected-current"; }
];
assert plan.repositories.flight.git.rollback.worktreeRestore.argv == [
  {
    kind = "literal";
    value = "git";
  }
  {
    kind = "literal";
    value = "-C";
  }
  {
    kind = "literal";
    value = "src/flight";
  }
  {
    kind = "literal";
    value = "read-tree";
  }
  {
    kind = "literal";
    value = "-m";
  }
  {
    kind = "literal";
    value = "-u";
  }
  { kind = "expected-current"; }
  { kind = "old-head"; }
];
assert plan.repositories.flight.git.rollback.refRestore.argv == [
  {
    kind = "literal";
    value = "git";
  }
  {
    kind = "literal";
    value = "-C";
  }
  {
    kind = "literal";
    value = "src/flight";
  }
  {
    kind = "literal";
    value = "update-ref";
  }
  {
    kind = "literal";
    value = "HEAD";
  }
  { kind = "expected-current"; }
  { kind = "old-head"; }
];
assert plan.repositories.flight.source.locked.rev == "2222222222222222222222222222222222222222";
assert !absoluteTransactionPath.success;
assert !driveTransactionPath.success;
assert !backslashTransactionPath.success;
{
  success = true;
  repositories = builtins.attrNames plan.repositories;
  flightPlan = builtins.sort builtins.lessThan plan.plans.packages.flight;
}
