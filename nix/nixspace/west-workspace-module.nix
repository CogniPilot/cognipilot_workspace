{
  config,
  inputs ? { },
  lib,
  ...
}:

let
  cfg = config.nixspace.west;

  inherit (builtins)
    attrNames
    hashFile
    hashString
    isAttrs
    map
    pathExists
    toJSON
    ;
  inherit (lib)
    concatStringsSep
    filter
    mkIf
    mkOption
    types
    ;

  validId = value: builtins.match "^[a-z][a-z0-9]*([._-][a-z0-9]+)*$" value != null;
  validRelativePath =
    value:
    value != ""
    && !lib.hasPrefix "/" value
    && !lib.hasInfix "\n" value
    && !lib.hasInfix "\r" value
    && builtins.all (part: part != "" && part != "..") (lib.splitString "/" value);
  validRootRelativePath =
    value: validRelativePath value && builtins.all (part: part != ".") (lib.splitString "/" value);
  joinPath =
    base: relative:
    if relative == "." then lib.removeSuffix "/" base else "${lib.removeSuffix "/" base}/${relative}";

  sourceInputName = cfg.workspace.source.input;
  sourceInput =
    if builtins.hasAttr sourceInputName inputs then
      inputs.${sourceInputName}
    else
      throw "nixspace.west source input `${sourceInputName}` is not bound by the importing flake";
  sourceStorePath =
    if isAttrs sourceInput && sourceInput ? outPath then
      toString sourceInput.outPath
    else
      toString sourceInput;
  sourceRoot = joinPath sourceStorePath cfg.workspace.source.root;
  manifestStorePath = joinPath sourceRoot cfg.workspace.manifest.relativePath;
  sourceInfo =
    if isAttrs sourceInput && sourceInput ? sourceInfo && isAttrs sourceInput.sourceInfo then
      sourceInput.sourceInfo
    else if isAttrs sourceInput then
      sourceInput
    else
      { };

  overrideNames = attrNames cfg.localOverrides;
  invalidOverrideNames = filter (name: !validId name) overrideNames;
  invalidOverridePaths = filter (
    name: !validRelativePath cfg.localOverrides.${name}.source
  ) overrideNames;
  overrideRecords = map (project: {
    inherit project;
    inherit (cfg.localOverrides.${project}) required source zephyrModule;
  }) overrideNames;

  plan =
    if !validRelativePath cfg.workspace.source.root then
      throw "nixspace.west.workspace.source.root must be a safe relative path"
    else if !validRelativePath cfg.workspace.manifest.relativePath then
      throw "nixspace.west.workspace.manifest.relativePath must be a safe relative path"
    else if invalidOverrideNames != [ ] then
      throw "nixspace.west.localOverrides has invalid West project IDs: ${concatStringsSep ", " invalidOverrideNames}"
    else if invalidOverridePaths != [ ] then
      throw "nixspace.west.localOverrides has unsafe relative source paths: ${concatStringsSep ", " invalidOverridePaths}"
    else if !pathExists manifestStorePath then
      throw "West manifest resource `${cfg.workspace.manifest.resource}` is missing at ${manifestStorePath}"
    else
      let
        manifestSha256 = hashFile "sha256" manifestStorePath;
        sourceIdentity = {
          storePath = sourceStorePath;
          narHash = sourceInfo.narHash or null;
          rev = sourceInfo.rev or null;
        };
        contentKey = hashString "sha256" (toJSON {
          inherit manifestSha256 sourceIdentity;
          inherit (cfg.workspace.manifest) relativePath resource;
        });
        localPolicyId = hashString "sha256" (toJSON overrideRecords);
      in
      {
        apiVersion = "nixspace/v1";
        kind = "WestWorkspace";
        interfaceVersion = 3;
        product = {
          id = cfg.product.id;
          interfaceVersion = cfg.product.interfaceVersion;
        };
        workspaceRoot = cfg.workspaceRoot;
        workspace = {
          id = cfg.workspace.id;
          source = {
            input = sourceInputName;
            root = cfg.workspace.source.root;
            identity = sourceIdentity;
          };
          manifest = {
            resource = cfg.workspace.manifest.resource;
            relativePath = cfg.workspace.manifest.relativePath;
            storePath = manifestStorePath;
            sha256 = manifestSha256;
          };
          inherit contentKey;
        };
        cache = {
          layoutVersion = 2;
          namespace = cfg.cacheNamespace;
          retainedGenerations = cfg.retainedGenerations;
          root = {
            base = "platform-cache";
            path = cfg.cacheRootPath;
          };
          nativePathCache = cfg.nativePathCache;
          narrowUpdate = cfg.narrowUpdate;
          paths = {
            generations = "${cfg.cacheNamespace}/west/workspaces/${contentKey}/generations";
            generationGcRoot = "locked";
            current = "${cfg.cacheNamespace}/west/workspaces/${contentKey}/current.json";
            publicationLock = "${cfg.cacheNamespace}/west/locks/${contentKey}.lock";
          };
        };
        localView = {
          root = {
            base = "workspace";
            path = cfg.localViewRootPath;
          };
          overrides = overrideRecords;
          policyId = localPolicyId;
          paths = {
            generations = "${cfg.product.id}/${contentKey}/${localPolicyId}/generations";
            executionLock = "${cfg.product.id}/${contentKey}/${localPolicyId}/execution.lock";
          };
        };
      };
in
{
  options.nixspace.west = {
    enable = lib.mkEnableOption "a Nix-planned native-West product workspace";

    product = mkOption {
      description = "Identity of the importing product represented by this West plan.";
      type = types.submodule (_: {
        options = {
          id = mkOption {
            type = types.strMatching "^[a-z][a-z0-9]*([._-][a-z0-9]+)*$";
            description = "Stable product ID.";
          };
          interfaceVersion = mkOption {
            type = types.ints.positive;
            description = "Version of the importing product's static interface.";
          };
        };
      });
    };

    workspace = mkOption {
      description = "Exact Nix source binding and West manifest resource selected by the importing product.";
      type = types.submodule (_: {
        options = {
          id = mkOption {
            type = types.strMatching "^[a-z][a-z0-9]*([._-][a-z0-9]+)*$";
            description = "Stable ID for this West workspace.";
          };
          source = mkOption {
            type = types.submodule (_: {
              options = {
                input = mkOption {
                  type = types.str;
                  description = "Exact importing-flake input containing the manifest source.";
                };
                root = mkOption {
                  type = types.str;
                  default = ".";
                  description = "Source-input-relative root containing the manifest resource.";
                };
              };
            });
          };
          manifest = mkOption {
            type = types.submodule (_: {
              options = {
                resource = mkOption {
                  type = types.str;
                  description = "Opaque importing-product coordinate for the selected manifest resource.";
                };
                relativePath = mkOption {
                  type = types.str;
                  description = "Source-root-relative west.yml path.";
                };
              };
            });
          };
        };
      });
    };

    workspaceRoot = mkOption {
      type = types.str;
      default = ".";
      description = ''
        Runtime workspace root. Relative editable override bindings are
        resolved below this path by nixspace.
      '';
    };

    cacheNamespace = mkOption {
      type = types.strMatching "^[a-z][a-z0-9]*([._-][a-z0-9]+)*$";
      default = cfg.product.id;
      description = "Stable namespace for content-addressed West workspaces.";
    };

    cacheRootPath = mkOption {
      type = types.addCheck types.str validRootRelativePath;
      default = "nixspace";
      description = "Platform-cache-relative root selected for immutable native West workspaces.";
    };

    localViewRootPath = mkOption {
      type = types.addCheck types.str validRootRelativePath;
      default = ".nixspace/state/west/views";
      description = "Workspace-relative root selected for editable West views.";
    };

    nativePathCache = mkOption {
      type = types.bool;
      default = true;
      description = "Allow native West --path-cache reuse between immutable product closures.";
    };

    narrowUpdate = mkOption {
      type = types.bool;
      default = true;
      description = "Request native West's narrow update mode.";
    };

    retainedGenerations = mkOption {
      type = types.ints.positive;
      default = 2;
      description = ''
        Total number of published West generations retained when the user
        explicitly runs nixspace west gc. The selected generation is always
        retained.
      '';
    };

    localOverrides = mkOption {
      default = { };
      description = ''
        Product-owned editable bindings keyed by the exact West project name.
        This declares policy only; project paths and the manifest graph remain
        native West data.
      '';
      type = types.attrsOf (
        types.submodule (_: {
          options = {
            source = mkOption {
              type = types.str;
              description = "Workspace-root-relative editable checkout path.";
            };
            required = mkOption {
              type = types.bool;
              default = false;
              description = "Fail instead of using the immutable checkout when the editable path is absent.";
            };
            zephyrModule = mkOption {
              type = types.bool;
              default = false;
              description = "Expose this override through the extra Zephyr module query.";
            };
          };
        })
      );
    };
  };

  config = mkIf cfg.enable {
    flake.nixspaceWestPlan = plan;

    perSystem =
      { pkgs, system, ... }:
      let
        westEnvironment = pkgs.python3.withPackages (pythonPackages: [ pythonPackages.west ]);
        sealExpression = pkgs.writeText "nixspace-west-seal.nix" ''
          { source }:
          let
            builderPackage = builtins.storePath "${pkgs.coreutils}";
            imported = builtins.path {
              path = /. + source;
              name = "nixspace-west-${plan.workspace.id}-source";
            };
          in
          builtins.derivation {
            name = "nixspace-west-${plan.workspace.id}";
            system = "${system}";
            builder = "''${builderPackage}/bin/cp";
            args = [ "-a" imported (builtins.placeholder "out") ];
          }
        '';
        systemPlan = plan // {
          tools = {
            west = "${westEnvironment}/bin/west";
            store = {
              interfaceVersion = 2;
              seal = {
                argv = [
                  { literal = "${pkgs.nix}/bin/nix"; }
                  { literal = "build"; }
                  { literal = "--impure"; }
                  { literal = "--file"; }
                  { literal = "${sealExpression}"; }
                  { literal = "--argstr"; }
                  { literal = "source"; }
                  { parameter = "source"; }
                  { literal = "--out-link"; }
                  { parameter = "gc-root"; }
                  { literal = "--print-out-paths"; }
                ];
                output = "store-path";
              };
            };
            projectPathEnvironment = {
              interfaceVersion = 1;
              countVariable = "GIT_CONFIG_COUNT";
              keyVariablePrefix = "GIT_CONFIG_KEY_";
              valueVariablePrefix = "GIT_CONFIG_VALUE_";
              key = "safe.directory";
            };
          };
        };
        planPackage = pkgs.writeTextDir "share/nixspace/west-plan.json" (toJSON systemPlan);
      in
      {
        packages.nixspace-west-plan = planPackage;
        checks.nixspace-west-plan =
          pkgs.runCommand "nixspace-west-plan"
            {
              nativeBuildInputs = [ pkgs.jq ];
            }
            ''
              jq -e '
                .apiVersion == "nixspace/v1"
                and .kind == "WestWorkspace"
                and .interfaceVersion == 3
                and .cache.layoutVersion == 2
                and .cache.retainedGenerations == 2
                and (.workspace.contentKey | test("^[0-9a-f]{64}$"))
                and (.workspace.manifest.sha256 | test("^[0-9a-f]{64}$"))
                and (.workspace.manifest.storePath | startswith("${builtins.storeDir}/"))
                and (.tools.west | startswith("${builtins.storeDir}/"))
                and .tools.store.interfaceVersion == 2
                and .tools.store.seal.output == "store-path"
                and ([.tools.store.seal.argv[].parameter // empty] == ["source", "gc-root"])
                and (.tools.store.seal.argv[0].literal | startswith("${builtins.storeDir}/"))
                and .tools.projectPathEnvironment.interfaceVersion == 1
                and .tools.projectPathEnvironment.countVariable == "GIT_CONFIG_COUNT"
                and ((.cache.paths.generations | split("/")[0]) == .cache.namespace)
                and (.localView.paths.generations | endswith("/generations"))
              ' ${planPackage}/share/nixspace/west-plan.json >/dev/null
              touch "$out"
            '';
      };
  };
}
