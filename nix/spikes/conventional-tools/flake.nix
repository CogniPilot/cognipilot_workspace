{
  description = "Build-free compatibility spikes for conventional Nix workspace tools";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/e7a3ca8092b61ff85b6a45bf863ea2b2d6a661b3";

    flake-parts.url = "github:hercules-ci/flake-parts/17c9d6cdfc60c64f4ee8d306f9bc0b4ccb51481e";
    flake-parts.inputs.nixpkgs-lib.follows = "nixpkgs";

    std.url = "github:divnix/std/4177882c378184b795fa97594b5effd062213891";

    zephyr-nix.url = "github:nix-community/zephyr-nix/6966fb1cbf2fdb494bea3062c5e8e7d44dd8ac9c";
    zephyr-nix.inputs.nixpkgs.follows = "nixpkgs";

    west2nix.url = "github:adisbladis/west2nix/f84670d66f881d9340b7d7626fbfe499438c134b";
    west2nix.inputs.nixpkgs.follows = "nixpkgs";
    west2nix.inputs.zephyr-nix.follows = "zephyr-nix";
  };

  outputs =
    inputs@{
      self,
      flake-parts,
      nixpkgs,
      std,
      west2nix,
      zephyr-nix,
      ...
    }:
    let
      system = "x86_64-linux";
      cubs2FrozenManifest = import ./cubs2-frozen-manifest.nix;
      westApi = west2nix.lib.mkWest2nix {
        pkgs = nixpkgs.legacyPackages.${system};
      };
      cubs2WestHook = westApi.mkWest2nixHook {
        manifest = cubs2FrozenManifest;
      };
    in
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        ./flake-parts-module.nix
        std.flakeModule
      ];
      systems = [ system ];

      std.grow = {
        cellsFrom = ./cells;
        cellBlocks = [ (std.blockTypes.data "projects") ];
      };
      std.pick.stdProjects = [ "workspace" "projects" ];

      flake.conventionalToolSpike = {
        pins = {
          flake-parts = flake-parts.rev;
          std = std.rev;
          west2nix = west2nix.rev;
          zephyr-nix = zephyr-nix.rev;
        };
        comparison = {
          flakeParts.projects = self.flakePartsProjects;
          std.projects = self.stdProjects;
          sameProjects = self.flakePartsProjects == self.stdProjects;
          flakeParts.devenvCoexistence = "native flake-parts module composition";
          std.devenvCoexistence = "possible through std.flakeModule, but Cells remain a second discovery hierarchy";
        };
        zephyrNix = {
          packageNames = builtins.attrNames zephyr-nix.packages.${system};
          requestedInterfaces = [
            "hosttools"
            "hosttools-nix"
            "pythonEnv"
            "sdk"
            "sdkFull"
          ];
          zephyrSourcePolicy = "follow the selected product's flake=false Zephyr fork";
        };
        west2nix = {
          libraryFunctionArguments = builtins.attrNames (builtins.functionArgs west2nix.lib.mkWest2nix);
          apiNames = builtins.attrNames westApi;
          cubs2 = {
            hookName = cubs2WestHook.name;
            projects = map (project: {
              inherit (project) name revision url;
              path = project.path or project.name;
            }) cubs2FrozenManifest.manifest.projects;
            projectNames = map (project: project.name) cubs2FrozenManifest.manifest.projects;
            projectPaths = map (project: project.path or project.name) cubs2FrozenManifest.manifest.projects;
            selfPath = cubs2FrozenManifest.manifest.self.path;
          };
        };
      };
    };
}
