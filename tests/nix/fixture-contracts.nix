{ inputs, pkgs }:

let
  suites = {
    static-interface = import ../cognipilot-static-interface.nix { inherit pkgs; };
    devenv-launch = import ../fixtures/devenv-launch/tests.nix { inherit pkgs; };
    devenv-launch-pinned = import ../fixtures/devenv-launch/pinned-devenv.nix {
      inherit pkgs;
      devenvSource = inputs.devenv.outPath;
    };
    devenv-workspace = import ../fixtures/devenv-workspace/tests.nix {
      inherit pkgs;
      devenvSource = inputs.devenv.outPath;
    };
    generic-interface = import ../fixtures/nixspace-generic/tests.nix { inherit pkgs; };
    host-secrets = import ../fixtures/nixspace-host-secrets/tests.nix { inherit pkgs; };
    project-output-cardinality = import ../fixtures/project-output/standalone-count-tests.nix {
      inherit pkgs;
    };
    source-workspace = import ../fixtures/source-workspace/tests.nix { inherit pkgs; };
    west-workspace = import ../fixtures/west-plan/tests.nix { inherit pkgs; };
  };
in
builtins.deepSeq suites {
  suite = "fixture-contracts";
  suiteCount = builtins.length (builtins.attrNames suites);
  names = builtins.attrNames suites;
  projectOutputCardinality = suites.project-output-cardinality;
}
