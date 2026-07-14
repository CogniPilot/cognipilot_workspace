{ inputs, cell }:
let
  Project = with inputs.std.yants; struct "workspace project" {
    kind = enum "project kind" [ "application" "source" ];
    inputs = attrs (struct "artifact input" {
      fromProject = string;
      output = string;
    });
    outputs = attrs (struct "artifact output" {
      type = enum "artifact type" [ "directory" "file" ];
    });
  };
in
builtins.mapAttrs (_: Project) (import (inputs.self + /fixture.nix))
