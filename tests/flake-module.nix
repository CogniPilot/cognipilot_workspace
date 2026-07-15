{
  config,
  inputs,
  lib,
  ...
}:

let
  rootConfig = config;
in
{
  perSystem =
    {
      config,
      pkgs,
      system,
      ...
    }:
    let
      reports = {
        fixtures = import ./nix/fixture-contracts.nix {
          inherit inputs pkgs system;
        };
        modules = import ./nix/module-contracts.nix { inherit pkgs; };
        root = import ./nix/root-contracts.nix {
          inherit
            inputs
            pkgs
            rootConfig
            system
            ;
          systemConfig = config;
        };
        tasks = import ./nix/task-contracts.nix { inherit pkgs; };
      };
      reportChecks = lib.mapAttrs' (name: report: {
        name = "cognipilot-contract-${name}";
        value = pkgs.writeText "cognipilot-contract-${name}.json" (
          builtins.toJSON (builtins.deepSeq report report)
        );
      }) reports;
      actionlint =
        pkgs.runCommand "cognipilot-actionlint"
          {
            nativeBuildInputs = [ pkgs.actionlint ];
            source = inputs.self;
          }
          ''
            cd "$source"
            actionlint .github/workflows/*.yml
            touch "$out"
          '';
      bootstrapSyntax =
        pkgs.runCommand "cognipilot-bootstrap-syntax"
          {
            source = inputs.self;
          }
          ''
            ${pkgs.runtimeShell} -n "$source/setup"
            ${pkgs.runtimeShell} -n "$source/ws"
            touch "$out"
          '';
      componentChecks = reportChecks // {
        cognipilot-contract-github-actions = actionlint;
        cognipilot-contract-bootstrap-shell = bootstrapSyntax;
      };
      aggregate = pkgs.linkFarm "cognipilot-nix-rust-contract-tests" (
        lib.mapAttrsToList (name: path: { inherit name path; }) componentChecks
      );
    in
    {
      checks = componentChecks // {
        cognipilot-contract-tests = aggregate;
      };
    };
}
