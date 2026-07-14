{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
    launches.demo = {
      description = "Invalid process references.";
      processes.worker = {
        executable = "missing:worker";
        argv = [ { parameter = "missing"; } ];
        environment."bad-name".literal = "value";
      };
    };
  };
}
