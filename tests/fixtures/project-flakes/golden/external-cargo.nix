{
  imports = [ ../../../../nix/cognipilot/flake-module.nix ];

  cognipilot.projects.example = {
    definition.origin = "external";
    preset = "cargo-v1";
  };
}
