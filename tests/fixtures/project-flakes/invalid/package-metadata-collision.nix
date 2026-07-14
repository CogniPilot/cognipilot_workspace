{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects = {
    first = {
      packageId = "shared";
      repositoryId = "first";
      source.input = "first-source";
      preset = "cargo-v1";
    };
    second = {
      aliases = [ "shared" ];
      repositoryId = "second";
      source.input = "second-source";
      preset = "cmake-v1";
    };
  };
}
