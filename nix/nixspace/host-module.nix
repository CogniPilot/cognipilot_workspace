{
  config,
  lib,
  ...
}:

let
  cfg = config.nixspace.host;
  document = cfg.plan;
  validObservationName = value:
    builtins.isString value
    && builtins.stringLength value <= 128
    && builtins.match "[A-Za-z0-9._-]+" value != null;
  safeCachePath = value:
    builtins.isString value
    && value != ""
    && !(lib.hasInfix "\n" value)
    && !(lib.hasInfix "\r" value)
    && !(lib.hasInfix "/../" "/${value}/");
  publicCacheUri = value:
    let
      remainder = lib.removePrefix "https://" value;
    in
    builtins.isString value
    && lib.hasPrefix "https://" value
    && remainder != ""
    && !(lib.hasPrefix "/" remainder)
    && !(lib.any (needle: lib.hasInfix needle value) [
      "\n"
      "\r"
      "\t"
      " "
      "@"
      "?"
      "#"
    ]);
  validatedDocument =
    if document == null then
      throw "nixspace.host.plan must be set by the workspace root"
    else if (document.apiVersion or null) != "nixspace/v1" then
      throw "nixspace.host.plan.apiVersion must be `nixspace/v1`"
    else if (document.kind or null) != "Host" then
      throw "nixspace.host.plan.kind must be `Host`"
    else if (document.interfaceVersion or null) != 4 then
      throw "nixspace.host.plan.interfaceVersion must be 4"
    else if
      !(document ? nix)
      || !(document.nix ? minimumVersion)
      || !(document.nix ? settings)
    then
      throw "nixspace.host.plan must declare nix.minimumVersion and nix.settings"
    else if
      !(document ? readiness)
      || !(document.readiness ? storage)
      || !(document.readiness.storage ? path)
      || !(document.readiness.storage ? minimumAvailableBytes)
      || !(document.readiness ? daemon)
      || !(document.readiness.daemon ? when)
      || !(document.readiness.daemon ? probeArgv)
      || !(document.readiness ? requiredDocuments)
      || !(document.readiness ? sourceSelection)
      || !(document.readiness ? launch)
      || !(document.readiness ? cache)
      || !(document.readiness.cache ? coverageMode)
      || !(document.readiness.cache ? storeDirectory)
      || !(document.readiness.cache ? roots)
      || !(document.readiness.cache ? stores)
    then
      throw "nixspace.host.plan must declare the complete versioned readiness contract"
    else if
      !(builtins.isString document.readiness.storage.path)
      || document.readiness.storage.path == ""
      || !(builtins.isInt document.readiness.storage.minimumAvailableBytes)
      || document.readiness.storage.minimumAvailableBytes < 0
      || !(builtins.isList document.readiness.daemon.probeArgv)
      || !(builtins.all builtins.isString document.readiness.daemon.probeArgv)
      || (
        document.readiness.daemon.when != "never"
        && document.readiness.daemon.probeArgv == [ ]
      )
      || !(builtins.isString document.readiness.sourceSelection)
      || document.readiness.sourceSelection == ""
      || !(document.readiness.launch ? allowActiveSessions)
      || !(builtins.isBool document.readiness.launch.allowActiveSessions)
      || !(document.readiness.launch ? requireManagerSocket)
      || !(builtins.isBool document.readiness.launch.requireManagerSocket)
      || !(document.readiness.launch ? requireAvailableDeclaredPorts)
      || !(builtins.isBool document.readiness.launch.requireAvailableDeclaredPorts)
    then
      throw "nixspace.host.plan readiness contract contains invalid values"
    else if
      document.readiness.cache.coverageMode != "union"
      || !(builtins.isList document.readiness.cache.roots)
      || !(builtins.isString document.readiness.cache.storeDirectory)
      || !(lib.hasPrefix "/" document.readiness.cache.storeDirectory)
      || !(safeCachePath document.readiness.cache.storeDirectory)
      || !(builtins.all (
        root:
        builtins.isAttrs root
        && root ? name
        && validObservationName root.name
        && root ? path
        && safeCachePath root.path
      ) document.readiness.cache.roots)
      || builtins.length (map (root: root.name) document.readiness.cache.roots)
        != builtins.length (lib.unique (map (root: root.name) document.readiness.cache.roots))
      || !(builtins.isList document.readiness.cache.stores)
      || document.readiness.cache.stores == [ ]
      || !(builtins.all (
        store:
        builtins.isAttrs store
        && store ? name
        && validObservationName store.name
        && store ? uri
        && publicCacheUri store.uri
      ) document.readiness.cache.stores)
      || builtins.length (map (store: store.name) document.readiness.cache.stores)
        != builtins.length (lib.unique (map (store: store.name) document.readiness.cache.stores))
    then
      throw "nixspace.host.plan readiness.cache must use union coverage with uniquely named safe roots and at least one credential-free public HTTPS store"
    else if
      let
        settings = document.nix.settings;
        configuredSubstituters =
          settings."extra-substituters" or (settings.substituters or [ ]);
      in
      !(builtins.isList configuredSubstituters)
      || !(builtins.all (
        store: builtins.elem store.uri configuredSubstituters
      ) document.readiness.cache.stores)
    then
      throw "nixspace.host.plan readiness.cache stores must also be expected Nix substituters"
    else if
      !(builtins.elem document.readiness.daemon.when [
        "always"
        "daemon-mode"
        "never"
      ])
    then
      throw "nixspace.host.plan readiness.daemon.when is unsupported"
    else if
      !(builtins.isList document.readiness.requiredDocuments)
      || !(builtins.all (
        kind: builtins.elem kind [
          "index"
          "source"
          "launch"
          "resolution"
        ]
      ) document.readiness.requiredDocuments)
      || builtins.length document.readiness.requiredDocuments
        != builtins.length (lib.unique document.readiness.requiredDocuments)
    then
      throw "nixspace.host.plan readiness.requiredDocuments must contain unique supported document kinds"
    else
      document;
in
{
  imports = [ ./tool-module.nix ];

  options.nixspace.host.plan = lib.mkOption {
    type = lib.types.nullOr lib.types.attrs;
    default = null;
    description = ''
      Nix-generated host expectation document consumed by nixspace doctor and
      setup. Cache URLs, public keys, trust policy, required Nix features,
      storage thresholds, daemon probes, and workspace readiness policy remain
      workspace data rather than Rust constants.
    '';
  };

  config = {
    flake.nixspaceHostPlan = validatedDocument;

    perSystem =
      { config, pkgs, ... }:
      let
        planPackage = pkgs.writeTextDir "share/nixspace/host-plan.json" (
          builtins.toJSON validatedDocument
        );
        hostClient = pkgs.writeShellScriptBin "nixspace-host" ''
          export NIXSPACE_HOST_PLAN=${planPackage}/share/nixspace/host-plan.json
          ${lib.optionalString (config.packages ? nixspace-resolution-plan) ''
            export NIXSPACE_RESOLUTION_PLAN=${config.packages.nixspace-resolution-plan}/share/nixspace/resolution-plan.json
          ''}
          exec ${lib.getExe config.packages.nixspace} "$@"
        '';
      in
      {
        packages = {
          nixspace-host = hostClient;
          nixspace-host-plan = planPackage;
        };
        apps.nixspace-host = {
          type = "app";
          program = "${hostClient}/bin/nixspace-host";
        };
      };
  };
}
