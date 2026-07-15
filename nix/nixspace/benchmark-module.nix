{
  flake-parts-lib,
  lib,
  ...
}:

let
  inherit (flake-parts-lib) mkPerSystemOption;
  inherit (lib) mkIf mkOption types;
  maxSamples = 1000;
  safePath =
    value:
    let
      segments = builtins.filter (segment: segment != "") (lib.splitString "/" value);
    in
    value == "."
    || (
      value != "" && segments != [ ] && builtins.all (segment: segment != "." && segment != "..") segments
    );
  safeStateRoot =
    value:
    value != "."
    && safePath value
    && !(lib.hasPrefix "/" value)
    && builtins.match "[A-Za-z]:.*" value == null;
  safeId = value: builtins.match "[A-Za-z0-9._-]+" value != null && value != "." && value != "..";
  validEnvironment =
    environment:
    builtins.all (name: name != "" && !(lib.hasInfix "=" name)) (builtins.attrNames environment);

  gateType = types.submodule {
    options = {
      p50Milliseconds = mkOption {
        type = types.nullOr types.ints.unsigned;
        default = null;
      };
      p95Milliseconds = mkOption {
        type = types.nullOr types.ints.unsigned;
        default = null;
      };
    };
  };
  commandType = types.submodule {
    options = {
      argv = mkOption { type = types.listOf types.str; };
      cwd = mkOption {
        type = types.str;
        default = ".";
      };
      environment = mkOption {
        type = types.attrsOf types.str;
        default = { };
      };
      inheritEnvironment = mkOption {
        type = types.bool;
        default = false;
      };
      timeoutMilliseconds = mkOption {
        type = types.ints.positive;
        default = 5000;
      };
      expectedExitCodes = mkOption {
        type = types.listOf (types.ints.between (-2147483648) 2147483647);
        default = [ 0 ];
      };
    };
  };
  caseType = types.submodule {
    options = {
      description = mkOption { type = types.nonEmptyStr; };
      context = mkOption {
        type = types.attrsOf types.str;
        default = { };
      };
      setup = mkOption {
        type = types.listOf commandType;
        default = [ ];
        description = "Exact unmeasured commands run once before all samples.";
      };
      beforeEach = mkOption {
        type = types.listOf commandType;
        default = [ ];
        description = "Exact unmeasured commands run before every sample.";
      };
      measure = mkOption {
        type = types.listOf commandType;
        description = "Ordered nonempty commands whose aggregate duration is measured.";
      };
      afterEach = mkOption {
        type = types.listOf commandType;
        default = [ ];
        description = "Exact unmeasured cleanup commands attempted after every sample.";
      };
      teardown = mkOption {
        type = types.listOf commandType;
        default = [ ];
        description = "Exact unmeasured cleanup commands attempted once after all samples.";
      };
      warmupSamples = mkOption {
        type = types.ints.between 0 maxSamples;
        default = 1;
      };
      measuredSamples = mkOption {
        type = types.ints.between 1 maxSamples;
        default = 7;
      };
      gates = mkOption {
        type = gateType;
        default = { };
      };
    };
  };
in
{
  imports = [ ./tool-module.nix ];

  options.perSystem = mkPerSystemOption (
    { config, pkgs, ... }:
    let
      cfg = config.nixspace.benchmark;
      invalidCaseIds = builtins.filter (id: !(safeId id)) (builtins.attrNames cfg.cases);
      duplicateDefaultCaseIds =
        builtins.length cfg.defaultCases != builtins.length (lib.unique cfg.defaultCases);
      unknownDefaultCaseIds = builtins.filter (id: !(builtins.hasAttr id cfg.cases)) cfg.defaultCases;
      commandValid =
        command:
        command.argv != [ ]
        && builtins.head command.argv != ""
        && safePath command.cwd
        && command.expectedExitCodes != [ ]
        &&
          builtins.length command.expectedExitCodes == builtins.length (lib.unique command.expectedExitCodes)
        && validEnvironment command.environment;
      invalidCases = lib.filterAttrs (
        _: case:
        case.measure == [ ]
        || !(builtins.all commandValid (
          case.setup ++ case.beforeEach ++ case.measure ++ case.afterEach ++ case.teardown
        ))
      ) cfg.cases;
      document = {
        apiVersion = "nixspace/v1";
        kind = "BenchmarkPlan";
        interfaceVersion = 4;
        limits.maxSamplesPerPhase = maxSamples;
        inherit (cfg)
          cases
          context
          defaultCases
          id
          reference
          stateRoot
          ;
      };
      validatedDocument =
        if !(safeStateRoot cfg.stateRoot) then
          throw "nixspace benchmark stateRoot must be a safe non-dot relative path"
        else if cfg.cases == { } then
          throw "nixspace benchmark plan must declare at least one case"
        else if invalidCaseIds != [ ] then
          throw "nixspace benchmark case IDs are invalid: ${lib.concatStringsSep ", " invalidCaseIds}"
        else if cfg.defaultCases == [ ] then
          throw "nixspace benchmark defaultCases must declare at least one case ID"
        else if duplicateDefaultCaseIds then
          throw "nixspace benchmark defaultCases must contain unique case IDs"
        else if unknownDefaultCaseIds != [ ] then
          throw "nixspace benchmark defaultCases reference unknown cases: ${lib.concatStringsSep ", " unknownDefaultCaseIds}"
        else if invalidCases != { } then
          throw "nixspace benchmark cases must have a nonempty measure phase and valid lifecycle commands: ${lib.concatStringsSep ", " (builtins.attrNames invalidCases)}"
        else
          document;
      planPackage =
        (pkgs.writeTextDir "share/nixspace/benchmark-plan.json" (builtins.toJSON validatedDocument))
        .overrideAttrs
          (_: {
            passthru.document = validatedDocument;
          });
    in
    {
      options.nixspace.benchmark = {
        enable = mkOption {
          type = types.bool;
          default = false;
        };
        id = mkOption { type = types.nonEmptyStr; };
        reference = {
          name = mkOption { type = types.nonEmptyStr; };
          class = mkOption { type = types.nonEmptyStr; };
        };
        context = mkOption {
          type = types.attrsOf types.str;
          default = { };
        };
        stateRoot = mkOption {
          type = types.str;
          default = ".nixspace/state/benchmarks";
        };
        cases = mkOption {
          type = types.attrsOf caseType;
          default = { };
        };
        defaultCases = mkOption {
          type = types.listOf types.str;
          default = builtins.attrNames cfg.cases;
          description = "Ordered nonempty default benchmark case selection.";
        };
      };

      config = mkIf cfg.enable { packages.nixspace-benchmark-plan = planPackage; };
    }
  );
}
