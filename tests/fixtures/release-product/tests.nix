{ pkgs }:

let
  inherit (pkgs) lib;
  system = pkgs.stdenv.hostPlatform.system;
  fixtureRoot = builtins.path {
    name = "release-product-fixture";
    path = ./.;
  };
  fakeNarHash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
  fakeRevision = "1111111111111111111111111111111111111111";

  lockedInput = name: path: {
    outPath = builtins.path { inherit name path; };
    narHash = fakeNarHash;
    rev = fakeRevision;
  };

  alphaRuntime = pkgs.writeShellScript "alpha-runtime" ''
    printf 'alpha immutable release:%s\n' "$*"
  '';
  alphaRelease =
    pkgs.linkFarm "alpha-release" [
      {
        name = "bin/alpha-runtime";
        path = alphaRuntime;
      }
      {
        name = "share/alpha/config.json";
        path = pkgs.writeText "alpha-config.json" ''
          {"source":"immutable-release"}
        '';
      }
    ]
    // {
      version = "1.2.3";
    };
  betaRelease = pkgs.writeText "beta-firmware" "beta immutable firmware\n" // {
    version = "4.5.6";
  };

  releaseProvider = (lockedInput "release-provider-input" ./provider) // {
    packages.${system} = {
      alpha-release = alphaRelease;
      beta-firmware = betaRelease;
    };
  };
  baseInputs = {
    self = {
      outPath = fixtureRoot;
      narHash = fakeNarHash;
      rev = fakeRevision;
      lastModifiedDate = "20260714000000";
    };
    nixpkgs = {
      outPath = pkgs.path;
      narHash = fakeNarHash;
      rev = fakeRevision;
    };
    alpha_source = lockedInput "alpha-source-input" ./sources/alpha;
    alpha_definition = lockedInput "alpha-definition-input" ./definitions/alpha;
    beta_source = lockedInput "beta-source-input" ./sources/beta;
    beta_definition = lockedInput "beta-definition-input" ./definitions/beta;
    release_provider = releaseProvider;
  };

  rootSupport = { lib, ... }: {
    options = {
      flake = lib.mkOption {
        type = lib.types.attrs;
        default = { };
      };
      perSystem = lib.mkOption { type = lib.types.deferredModule; };
    };
  };
  systemSupport = { lib, ... }: {
    options = {
      apps = lib.mkOption {
        type = lib.types.lazyAttrsOf lib.types.attrs;
        default = { };
      };
      checks = lib.mkOption {
        type = lib.types.lazyAttrsOf lib.types.package;
        default = { };
      };
      devShells = lib.mkOption {
        type = lib.types.lazyAttrsOf lib.types.package;
        default = { };
      };
      formatter = lib.mkOption {
        type = lib.types.nullOr lib.types.package;
        default = null;
      };
      packages = lib.mkOption {
        type = lib.types.lazyAttrsOf lib.types.package;
        default = { };
      };
    };
  };

  mkRoot =
    {
      inputs ? baseInputs,
      modules ? [ ],
    }:
    lib.evalModules {
      specialArgs = { inherit inputs; };
      modules = [
        rootSupport
        ../../../nix/cognipilot/flake-module.nix
        ../../../nix/cognipilot/product-flake-module.nix
        ../../../nix/cognipilot/resolution-module.nix
        ../../../nix/cognipilot/nixspace-module.nix
        ./project-module.nix
      ]
      ++ modules;
    };
  evalSystem =
    root:
    (lib.evalModules {
      specialArgs = { inherit pkgs system; };
      modules = [
        systemSupport
        root.config.perSystem
      ];
    }).config;

  fullRoot = mkRoot { };
  full = evalSystem fullRoot;
  publicRoot = mkRoot {
    modules = [
      { cognipilot.product.promotionVisibilities = lib.mkForce [ "public" ]; }
    ];
  };
  public = evalSystem publicRoot;
  noDefault = evalSystem (mkRoot {
    modules = [
      { cognipilot.product.defaultTarget = lib.mkForce null; }
    ];
  });

  record = full.packages.promotion-record.passthru.normalizedRecord;
  publicRecord = public.packages.promotion-record.passthru.normalizedRecord;
  sbom = full.packages.promotion-sbom.passthru.document;
  statement = full.packages.promotion-attestation.passthru.statement;
  resolution = full.packages.nixspace-resolution-plan.passthru.document;
  packageIds = document: map (package: package.packageId) document.packages;
  targetCoordinates = document: map (target: target.coordinate) document.outputs.targets;

  forcePromotion =
    inputs: modules:
    builtins.tryEval (
      builtins.deepSeq
        (evalSystem (mkRoot {
          inherit inputs modules;
        })).packages.promotion-record.drvPath
        true
    );
  missingProvider = forcePromotion baseInputs [
    {
      cognipilot.projects.alpha.targets.default.release.provider = lib.mkForce "missing_provider";
    }
  ];
  missingOutput = forcePromotion baseInputs [
    {
      cognipilot.projects.alpha.targets.default.release.package = lib.mkForce "missing-output";
    }
  ];
  dirtySourceInputs = baseInputs // {
    alpha_source = baseInputs.alpha_source // {
      dirtyRev = "dirty";
    };
  };
  dirtySource = forcePromotion dirtySourceInputs [ ];
  driftProvider = releaseProvider // {
    packages.${system} = releaseProvider.packages.${system} // {
      alpha-release = alphaRelease // {
        version = "9.9.9";
      };
    };
  };
  versionDrift = forcePromotion (baseInputs // { release_provider = driftProvider; }) [ ];

  tests = {
    releaseReferencesRemainSeparateFromSourceAuthority = {
      expr = {
        release = fullRoot.config.cognipilot.validatedIndex.projects.alpha.targets.default.release;
        source = fullRoot.config.cognipilot.validatedIndex.projects.alpha.source.input;
      };
      expected = {
        release = {
          provider = "release_provider";
          package = "alpha-release";
        };
        source = "alpha_source";
      };
    };
    namedRootsResolveExactProviderDerivations = {
      expr = {
        alpha = full.packages.target-alpha--default.drvPath;
        alphaProvider = alphaRelease.drvPath;
        beta = full.packages.target-beta--firmware.drvPath;
        betaProvider = betaRelease.drvPath;
        default = full.packages.default.drvPath;
        product = full.packages.product-release-fixture.drvPath;
        workspace = full.packages.workspace.drvPath;
        publicDiffers = full.packages.public-workspace.drvPath != full.packages.workspace.drvPath;
      };
      expected = {
        alpha = alphaRelease.drvPath;
        alphaProvider = alphaRelease.drvPath;
        beta = betaRelease.drvPath;
        betaProvider = betaRelease.drvPath;
        default = alphaRelease.drvPath;
        product = full.packages.workspace.drvPath;
        workspace = full.packages.workspace.drvPath;
        publicDiffers = true;
      };
    };
    promotionRecordIsLockedProductProjection = {
      expr = {
        inherit (record) schemaVersion;
        product = {
          inherit (record.product) id interfaceVersion system;
          immutable = record.product.sourceIdentity.immutable;
        };
        packages = packageIds record;
        targets = targetCoordinates record;
        alpha = {
          source = (builtins.elemAt record.packages 0).source.identity.input;
          sourceImmutable = (builtins.elemAt record.packages 0).source.identity.immutable;
          definition = (builtins.elemAt record.packages 0).definition.identity.input;
          definitionImmutable = (builtins.elemAt record.packages 0).definition.identity.immutable;
          providerImmutable =
            (builtins.elemAt (builtins.elemAt record.packages 0).targets 0).release.providerIdentity.immutable;
        };
        closureIsDeferred = record.immutableDependencyClosure.storePaths == null;
        checkSharesPackage =
          full.checks.cognipilot-promotion-record.drvPath == full.packages.promotion-record.drvPath;
      };
      expected = {
        schemaVersion = 2;
        product = {
          id = "release-fixture";
          interfaceVersion = 1;
          inherit system;
          immutable = true;
        };
        packages = [
          "alpha"
          "beta"
          "observer"
        ];
        targets = [
          "alpha:default"
          "beta:firmware"
        ];
        alpha = {
          source = "alpha_source";
          sourceImmutable = true;
          definition = "alpha_definition";
          definitionImmutable = true;
          providerImmutable = true;
        };
        closureIsDeferred = true;
        checkSharesPackage = true;
      };
    };
    publicProjectionExcludesPrivateProjectsBeforeIdentityResolution = {
      expr = {
        packages = packageIds publicRecord;
        targets = targetCoordinates publicRecord;
        containsPrivateText = lib.hasInfix "beta" (builtins.toJSON publicRecord);
        publicWorkspaceDiffers =
          public.packages.public-workspace.drvPath != full.packages.workspace.drvPath;
      };
      expected = {
        packages = [ "alpha" ];
        targets = [ "alpha:default" ];
        containsPrivateText = false;
        publicWorkspaceDiffers = true;
      };
    };
    lockedResolutionNeedsNoEditableCheckout = {
      expr = {
        selectedScope = resolution.packagePlans.alpha.selectedScope;
        selectedCandidates = resolution.packagePlans.alpha.commandScopes.locked.selectedCandidates;
        localArtifact = resolution.artifacts."alpha:default:runtime".candidates.local;
        lockedArtifact = resolution.artifacts."alpha:default:runtime".candidates.locked;
        lockedResource = resolution.resources."alpha/config".candidates.locked;
        lockedExecutable = resolution.executables."alpha/runtime".candidates.locked;
      };
      expected = {
        selectedScope = "locked";
        selectedCandidates.alpha = "locked";
        localArtifact = null;
        lockedArtifact = {
          kind = "nix-store";
          installable = ".#target-alpha--default";
          provider = "release_provider";
          package = "alpha-release";
          relativePath = "bin/alpha-runtime";
          storePath = toString alphaRelease;
          provenance = {
            kind = "locked-output";
            label = "LOCKED";
            provider = "release_provider";
            package = "alpha-release";
          };
        };
        lockedResource = {
          kind = "nix-store";
          installable = ".#target-alpha--default";
          provider = "release_provider";
          package = "alpha-release";
          relativePath = "share/alpha/config.json";
          storePath = toString alphaRelease;
          provenance = {
            kind = "locked-output";
            label = "LOCKED";
            provider = "release_provider";
            package = "alpha-release";
          };
        };
        lockedExecutable = {
          artifact = "alpha:default:runtime";
          relativePath = "bin/alpha-runtime";
        };
      };
    };
    promotionBuildersUseOnlyTypedNixspaceMaterialization = {
      expr = {
        recordMaterializers = builtins.length (
          lib.tail (lib.splitString "_materialize-closure" full.packages.promotion-record.buildCommand)
        );
        attestationMaterializers = builtins.length (
          lib.tail (lib.splitString "_materialize-closure" full.packages.promotion-attestation.buildCommand)
        );
        recordHasShellMaterializer =
          lib.hasInfix "jq" full.packages.promotion-record.buildCommand
          || lib.hasInfix "mkdir" full.packages.promotion-record.buildCommand;
        attestationHasShellMaterializer =
          lib.hasInfix "jq" full.packages.promotion-attestation.buildCommand
          || lib.hasInfix "mkdir" full.packages.promotion-attestation.buildCommand;
        recordNativeInputs = full.packages.promotion-record.nativeBuildInputs;
        attestationNativeInputs = full.packages.promotion-attestation.nativeBuildInputs;
      };
      expected = {
        recordMaterializers = 1;
        attestationMaterializers = 1;
        recordHasShellMaterializer = false;
        attestationHasShellMaterializer = false;
        recordNativeInputs = [ ];
        attestationNativeInputs = [ ];
      };
    };
    defaultExistsOnlyForExplicitNaturalTarget = {
      expr = {
        selected = full.packages.default.drvPath;
        alpha = alphaRelease.drvPath;
        absentWithoutSelection = !(noDefault.packages ? default);
      };
      expected = {
        selected = alphaRelease.drvPath;
        alpha = alphaRelease.drvPath;
        absentWithoutSelection = true;
      };
    };
    spdxAndAttestationAreDerivedFromTheSameSelection = {
      expr = {
        inherit (sbom) spdxVersion dataLicense documentDescribes;
        packageNames = map (package: package.name) sbom.packages;
        relationshipCount = builtins.length sbom.relationships;
        statementType = statement._type;
        inherit (statement) predicateType;
        selectionDigest = statement.predicate.buildDefinition.externalParameters.selectionDigest;
        expectedSelectionDigest = record.selectionDigest;
      };
      expected = {
        spdxVersion = "SPDX-2.3";
        dataLicense = "CC0-1.0";
        documentDescribes = [ "SPDXRef-Product-release-fixture" ];
        packageNames = [
          "release-fixture"
          "alpha"
          "beta"
          "observer"
        ];
        relationshipCount = 3;
        statementType = "https://in-toto.io/Statement/v1";
        predicateType = "https://slsa.dev/provenance/v1";
        selectionDigest = record.selectionDigest;
        expectedSelectionDigest = record.selectionDigest;
      };
    };
    invalidPromotionInputsFailClosed = {
      expr = {
        missingProvider = missingProvider.success;
        missingOutput = missingOutput.success;
        dirtySource = dirtySource.success;
        versionDrift = versionDrift.success;
      };
      expected = {
        missingProvider = false;
        missingOutput = false;
        dirtySource = false;
        versionDrift = false;
      };
    };
  };
  failures = lib.runTests tests;

  realized =
    pkgs.runCommand "release-product-contract"
      {
        nativeBuildInputs = [ pkgs.jq ];
      }
      ''
        record=${public.packages.promotion-record}/share/cognipilot/promotion-record.json
        sbom=${public.packages.promotion-sbom}/share/cognipilot/sbom.spdx.json
        attestation=${public.packages.promotion-attestation}/share/cognipilot/provenance.intoto.json
        public_root=${public.packages.public-cache-root}
        client=${full.packages.nixspace}/bin/nixspace
        index=${full.packages.nixspace-index}/share/nixspace/index.json
        resolution=${full.packages.nixspace-resolution-plan}/share/nixspace/resolution-plan.json

        jq -e '
          .schemaVersion == 2 and
          [.packages[].packageId] == ["alpha"] and
          [.outputs.targets[].coordinate] == ["alpha:default"] and
          (.immutableDependencyClosure.storePaths | length) > 0
        ' "$record" >/dev/null
        jq -e '
          .spdxVersion == "SPDX-2.3" and
          [.packages[].name] == ["release-fixture", "alpha"]
        ' "$sbom" >/dev/null
        jq -e '
          ._type == "https://in-toto.io/Statement/v1" and
          .predicateType == "https://slsa.dev/provenance/v1" and
          (.subject | length) > 0 and
          (.predicate.runDetails.metadata.cognipilot_nixClosure | length) > 0
        ' "$attestation" >/dev/null

        test -x ${public.packages.target-alpha--default}/bin/alpha-runtime
        test -f ${public.packages.target-alpha--default}/share/alpha/config.json
        test -e "$public_root/workspace/alpha--default"
        test ! -e "$public_root/workspace/beta--firmware"

        mkdir -p "$TMPDIR/editable-checkouts"
        sentinel=COGNIPILOT-LOCAL-STATE-MUST-NOT-ENTER-RELEASE-7d13e024
        printf '%s\n' "$sentinel" > "$TMPDIR/editable-checkouts/local-sentinel"
        ! grep -F "$sentinel" "$record" "$sbom" "$attestation"
        ! grep -F "$TMPDIR/editable-checkouts" "$record" "$sbom" "$attestation"

        common=(
          "$client"
          --workspace-root "$TMPDIR/editable-checkouts"
          --index "$index"
          --resolution-plan "$resolution"
        )
        "''${common[@]}" package prefix alpha --json \
          | jq -e --arg path ${lib.escapeShellArg (toString alphaRelease)} \
              '.selectedCandidate == "locked" and .path == $path' >/dev/null
        "''${common[@]}" resource alpha/config --json \
          | jq -e --arg path ${lib.escapeShellArg "${alphaRelease}/share/alpha/config.json"} \
              '.selectedCandidate == "locked" and .path == $path' >/dev/null
        "''${common[@]}" launch plan alpha/release --json \
          | jq -e '
              .requiredArtifacts == ["alpha:default:runtime"] and
              .requiredResources == ["alpha/config"] and
              .processes[0].executable == "alpha/runtime"
            ' >/dev/null
        "''${common[@]}" run --selection-root alpha alpha/runtime -- --from-release-launch \
          | grep -Fx 'alpha immutable release:--from-release-launch'
        touch "$out"
      '';
in
if failures == [ ] then
  {
    report = {
      suite = "release-product";
      assertionCount = builtins.length (builtins.attrNames tests);
      packageCount = builtins.length record.packages;
      publicPackageCount = builtins.length publicRecord.packages;
    };
    checks.release-product-realized = realized;
  }
else
  throw "release product contract failures: ${builtins.toJSON failures}"
