{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    launches.demo = {
      description = "Unresolved requirements.";
      requiredArtifacts = [ "missing:default:artifact" ];
      requiredResources = [ "missing:resource" ];
    };
  };
}
