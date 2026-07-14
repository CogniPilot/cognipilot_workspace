{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    customActions.invalid = {
      kind = "other";
      argv = [
        "consume"
        { artifactInput = "missing"; }
      ];
    };
  };
}
