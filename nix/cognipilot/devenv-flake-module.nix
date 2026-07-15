{
  flake-parts-lib,
  inputs,
  lib,
  ...
}:

let
  # Devenv's flake-parts module currently exports every process/test helper
  # and both implicit container derivations as packages.  Those outputs are
  # deprecated or require optional inputs even when no container was
  # requested.  Devenv does not expose an option to disable them, so compose
  # its supported mkEval API directly and export only the devShell contract.
  devenv = inputs.devenv;
in
{
  options.perSystem = flake-parts-lib.mkPerSystemOption (
    {
      config,
      pkgs,
      system,
      ...
    }:
    let
      evaluatedShellType =
        (devenv.lib.mkEval {
          inherit inputs lib pkgs;
          inherit (config.devenv) modules;
        }).type;

      # A checked-in flake must evaluate without an impure host path.  Pure
      # consumers use the immutable selected source; direnv overrides the
      # tiny root file with the editable checkout's absolute path.
      configuredRoot =
        if inputs ? devenv-root then
          lib.removeSuffix "\n" (builtins.readFile inputs.devenv-root.outPath)
        else
          "";
      runtimeRoot = builtins.getEnv "NIXSPACE_WORKSPACE_ROOT";
      workspaceRoot =
        # The generic client supplies this only to the explicitly impure
        # bootstrap invocation selected by the Nix-generated ActionRunner.
        # Pure flake consumers see an empty environment and retain the locked
        # source fallback below.
        if runtimeRoot != "" then
          runtimeRoot
        else if configuredRoot != "" then
          configuredRoot
        else if inputs ? self then
          toString inputs.self.outPath
        else
          throw "the Devenv flake integration requires either `devenv-root` or `self`";
    in
    {
      options.devenv = {
        modules = lib.mkOption {
          type = lib.types.listOf lib.types.deferredModule;
          default = [ ];
          description = "Modules imported into every Devenv shell.";
        };

        shells = lib.mkOption {
          type = lib.types.lazyAttrsOf evaluatedShellType;
          default = { };
          description = "Named Devenv configurations exported only as devShells.";
        };
      };

      config = {
        devenv.modules = lib.mkBefore [
          {
            devenv.root = workspaceRoot;
            # Devenv 2.1.2 declares implicit `shell` and `processes`
            # containers even when this integration exports no containers.
            # Their upstream copyToRoot default evaluates an unfiltered
            # builtins.path over the `self` module argument.  Explicitly bind
            # the unused container roots to the canonical Git-filtered flake
            # source so both containers share it and no invocation mode can
            # re-import a mutable workspace through the container default.
            # Project sources remain independent locked inputs; editable
            # actions continue to use their runtime checkout paths.
            containers.shell.copyToRoot = lib.mkForce [ inputs.self.outPath ];
            containers.processes.copyToRoot = lib.mkForce [ inputs.self.outPath ];
            # Devenv's fallback resolves devenv-tasks by fetching and importing
            # its own locked Nixpkgs during evaluation.  The flake input already
            # exports the exact package for every supported system, so bind it
            # explicitly and keep flake/check evaluation free of native IFD.
            task.package = devenv.packages.${system}.devenv-tasks;
          }
        ];
        devShells = lib.mapAttrs (_: evaluated: evaluated.shell) config.devenv.shells;
      };
    }
  );
}
