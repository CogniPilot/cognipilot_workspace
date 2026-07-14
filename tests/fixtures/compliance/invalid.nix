{
  imports = [
    ../../../nix/cognipilot/flake-module.nix
    ../../../nix/cognipilot/compliance-flake-module.nix
  ];

  cognipilot.projects.legacy-flight = {
    packageId = "legacy-flight-control";
    lifecycle = "deprecated";
    deployability = "local-only";
    repositoryId = "legacy-flight-control";
    source.input = "legacy-flight-control-source";
    preset = "cargo-v1";
    customActions.package = {
      kind = "other";
      argv = [ "package-flight-control" ];
    };
  };
}
