{
  inputs,
  pkgs,
  system,
}:

let
  portableSuites = {
    devenv-launch = import ../fixtures/devenv-launch/tests.nix { inherit pkgs; };
    static-interface = import ../cognipilot-static-interface.nix { inherit pkgs; };
    generic-interface = import ../fixtures/nixspace-generic/tests.nix { inherit pkgs; };
    host-secrets = import ../fixtures/nixspace-host-secrets/tests.nix { inherit pkgs; };
    project-output-cardinality = import ../fixtures/project-output/standalone-count-tests.nix {
      inherit pkgs;
    };
    source-workspace = import ../fixtures/source-workspace/tests.nix { inherit pkgs; };
    west-workspace = import ../fixtures/west-plan/tests.nix { inherit pkgs; };
  };
  upstreamDevenvSuites = {
    devenv-launch-pinned = import ../fixtures/devenv-launch/pinned-devenv.nix {
      inherit pkgs;
      devenvSource = inputs.devenv.outPath;
    };
    devenv-workspace = import ../fixtures/devenv-workspace/tests.nix {
      inherit pkgs;
      devenvSource = inputs.devenv.outPath;
    };
  };
  # Devenv's upstream module implementation performs native IFD while defining
  # its option types. Its process-manager contract is target-independent, so
  # evaluate that integration once on the reference Linux system. The pure
  # CogniPilot modules and fixtures remain checked for every flake system.
  checksUpstreamDevenv = system == "x86_64-linux";
  suites = portableSuites // (if checksUpstreamDevenv then upstreamDevenvSuites else { });
in
builtins.deepSeq suites {
  suite = "fixture-contracts";
  inherit checksUpstreamDevenv;
  suiteCount = builtins.length (builtins.attrNames suites);
  names = builtins.attrNames suites;
  projectOutputCardinality = portableSuites.project-output-cardinality;
}
