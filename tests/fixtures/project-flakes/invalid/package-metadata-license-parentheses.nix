{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    license.spdx = "MIT OR (Apache-2.0";
    repositoryId = "example";
    source.input = "example-source";
    preset = "cargo-v1";
  };
}
