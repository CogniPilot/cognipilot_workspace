{ devenvInput }:

let
  devenvRevision = devenvInput.rev or (throw "the locked devenv input must expose its revision");
  devenvManifest = builtins.fromTOML (builtins.readFile (devenvInput.outPath + "/Cargo.toml"));
  devenvVersion = devenvManifest.workspace.package.version;
  devenvInstallable = "github:cachix/devenv/${devenvRevision}";
  substituters = [
    "https://cache.nixos.org"
    "https://devenv.cachix.org"
    "https://cachix.cachix.org"
    "https://cognipilot.cachix.org"
    "https://ros.cachix.org"
  ];
  publicKeys = [
    "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
    "devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw="
    "cachix.cachix.org-1:eWNHQldwUO7G2VkjpnjDbWwy4KQ/HNxht7H4SSoMckM="
    "cognipilot.cachix.org-1:TSm+h3sAVYP0B+D+1uG5hvgI/4JFAS3LFvv/yhTwvK8="
    "ros.cachix.org-1:dSyZxI8geDCJrwgvCOHDoAfOm5sV1wCPjBkKL+38Rvo="
  ];
in
{
  inherit publicKeys substituters;

  devenvPin = {
    revision = devenvRevision;
    version = devenvVersion;
    installable = devenvInstallable;
  };

  # cache.nixos.org and its signing key are Nix defaults. Keep them explicit in
  # the managed Host contract, but do not redundantly request them as flake
  # extras (which would make every consumer accept configuration it has
  # already inherited from Nix).
  flakeNixConfig = {
    extra-substituters = builtins.tail substituters;
    extra-trusted-public-keys = builtins.tail publicKeys;
  };

  hostPlan = {
    apiVersion = "nixspace/v1";
    kind = "Host";
    interfaceVersion = 4;
    nix = {
      minimumVersion = "2.18";
      settings = {
        accept-flake-config = true;
        # Share byte-identical regular files between store paths.  Source
        # boundaries still prevent mutable build trees from entering the
        # store; this is a second, host-level defense for legitimate overlap.
        auto-optimise-store = true;
        experimental-features = [
          "nix-command"
          "flakes"
        ];
        extra-substituters = substituters;
        extra-trusted-substituters = substituters;
        extra-trusted-public-keys = publicKeys;
        # This workspace is intended for developer machines whose local users
        # are all trusted to invoke Nix builds. Nix documents trusted users as
        # effectively root-capable; keeping this declaration visible in the
        # generated plan makes that security choice auditable.
        trusted-users = [ "*" ];
      };
    };
    readiness = {
      storage = {
        path = ".";
        minimumAvailableBytes = 5368709120;
      };
      daemon = {
        when = "daemon-mode";
        probeArgv = [
          "nix"
          "store"
          "ping"
          "--store"
          "daemon"
        ];
      };
      requiredDocuments = [
        "index"
        "source"
        "launch"
        "resolution"
      ];
      sourceSelection = "default";
      launch = {
        allowActiveSessions = true;
        requireManagerSocket = true;
        requireAvailableDeclaredPorts = true;
      };
      cache = {
        coverageMode = "union";
        storeDirectory = builtins.storeDir;
        # `nix build --out-link result-public-cache-root
        # .#public-cache-root` realizes this path. Doctor skips the observation
        # when the out-link is absent, so routine diagnosis never evaluates a
        # flake, realizes a package, or contacts a cache.
        roots = [
          {
            name = "public-cache-root";
            path = "result-public-cache-root";
          }
        ];
        stores = [
          {
            name = "nixos";
            uri = "https://cache.nixos.org";
          }
          {
            name = "cognipilot";
            uri = "https://cognipilot.cachix.org";
          }
        ];
      };
    };
    tools.devenv = {
      executable = "devenv";
      expectedVersion = devenvVersion;
      versionArgv = [
        "devenv"
        "version"
      ];
      installArgv = [
        "nix"
        "profile"
        "add"
        "--accept-flake-config"
        devenvInstallable
      ];
    };
  };
}
