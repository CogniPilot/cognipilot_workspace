{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    repositoryId = "example";
    source.input = "example-source";
    preset = "west-v1";
    targets.default.variants.dimensions.board = {
      values = [ "native_sim;touch-pwned" ];
      default = "native_sim;touch-pwned";
    };
  };
}
