{
  config,
  inputs ? { },
  lib,
  ...
}:

let
  rootConfig = config;
  cfg = rootConfig.cognipilot.product;
  normalizedIndex = rootConfig.cognipilot.validatedIndex;
  nixspaceIndex = rootConfig.cognipilot.nixspaceIndex;
  selectedProjects = rootConfig.cognipilot.projects;
  promotedProjects = lib.filterAttrs (
    _: project: builtins.elem project.source.visibility cfg.promotionVisibilities
  ) normalizedIndex.projects;

  productSourcePath =
    if !(inputs ? self) then
      null
    else if builtins.isAttrs inputs.self && inputs.self ? outPath then
      inputs.self.outPath
    else
      inputs.self;
  productLockPath = if productSourcePath == null then null else "${productSourcePath}/flake.lock";
  productLock =
    if productLockPath != null && builtins.pathExists productLockPath then
      builtins.fromJSON (builtins.readFile productLockPath)
    else
      null;

  rootLockInputs =
    if productLock == null then { } else productLock.nodes.${productLock.root}.inputs or { };

  lockNodeNameFor =
    inputName:
    let
      reference = rootLockInputs.${inputName} or null;
    in
    if builtins.isString reference then reference else null;

  selectedInputIdentity =
    inputName:
    let
      isSelf = inputName == "self";
      inputPresent = if isSelf then inputs ? self else builtins.hasAttr inputName inputs;
      inputValue =
        if !inputPresent then
          null
        else if isSelf then
          inputs.self
        else
          inputs.${inputName};
      inputAttrs = if builtins.isAttrs inputValue then inputValue else { };
      sourceInfo =
        if inputAttrs ? sourceInfo && builtins.isAttrs inputAttrs.sourceInfo then
          inputAttrs.sourceInfo
        else
          inputAttrs;
      nodeName = if isSelf then null else lockNodeNameFor inputName;
      node =
        if nodeName != null && productLock != null && builtins.hasAttr nodeName productLock.nodes then
          productLock.nodes.${nodeName}
        else
          null;
      locked = if node != null && node ? locked then node.locked else null;
      narHash = if locked != null && locked ? narHash then locked.narHash else sourceInfo.narHash or null;
      outPath =
        if sourceInfo ? outPath then
          toString sourceInfo.outPath
        else if inputAttrs ? outPath then
          toString inputAttrs.outPath
        else if inputValue != null && !builtins.isAttrs inputValue then
          toString inputValue
        else
          null;
      dirty = sourceInfo ? dirtyRev || sourceInfo ? dirtyShortRev || (sourceInfo.rev or null) == "dirty";
      pathIsRelative =
        locked == null || (locked.type or null) != "path" || !(lib.hasPrefix "/" (locked.path or "/"));
      lockIsExact = isSelf || locked != null;
      immutable =
        inputPresent
        && lockIsExact
        && narHash != null
        && outPath != null
        && lib.hasPrefix "/nix/store/" outPath
        && pathIsRelative
        && !dirty;
      revision =
        if locked != null && locked ? rev then
          {
            kind = "git-commit";
            value = locked.rev;
          }
        else if sourceInfo ? rev && sourceInfo.rev != null && sourceInfo.rev != "dirty" then
          {
            kind = "git-commit";
            value = sourceInfo.rev;
          }
        else if narHash != null then
          {
            # Path and archive inputs need not have a VCS revision.  Their NAR
            # hash is the exact source-tree revision selected by flake.lock.
            kind = "nix-nar";
            value = narHash;
          }
        else
          null;
      canonical =
        if locked == null then
          null
        else if (locked.type or null) == "github" then
          "https://github.com/${locked.owner}/${locked.repo}.git@${locked.rev}"
        else if (locked.type or null) == "gitlab" then
          "https://gitlab.com/${locked.owner}/${locked.repo}.git@${locked.rev}"
        else if (locked.type or null) == "git" then
          "${locked.url}@${locked.rev}"
        else if (locked.type or null) == "path" then
          locked.path
        else
          null;
    in
    {
      input = inputName;
      inherit
        canonical
        immutable
        narHash
        nodeName
        revision
        ;
      storePath = outPath;
      lock = locked;
      sourceInfo = lib.filterAttrs (_: value: value != null) {
        lastModified = sourceInfo.lastModified or null;
        lastModifiedDate = sourceInfo.lastModifiedDate or null;
        rev = sourceInfo.rev or null;
        shortRev = sourceInfo.shortRev or null;
      };
      status =
        if !inputPresent then
          "missing"
        else if dirty || !pathIsRelative then
          "mutable"
        else if !lockIsExact || narHash == null || outPath == null then
          "unlocked"
        else
          "locked";
    };

  releaseRecords = lib.concatLists (
    lib.mapAttrsToList (
      _projectId: project:
      lib.concatLists (
        lib.mapAttrsToList (
          targetId: target:
          lib.optional (target.release != null) {
            inherit targetId;
            inherit (project) packageId softwareVersion;
            visibility = project.source.visibility;
            inherit (target) release;
            coordinate = "${project.packageId}:${targetId}";
            # `--` cannot occur inside a valid CogniPilot ID, so this is
            # injective without requiring quoted flake attribute paths.
            packageName = "target-${project.packageId}--${targetId}";
          }
        ) project.targets
      )
    ) promotedProjects
  );

  selectedDefault =
    if cfg.defaultTarget == null then
      null
    else
      let
        coordinate = "${cfg.defaultTarget.packageId}:${cfg.defaultTarget.targetId}";
        matches = builtins.filter (record: record.coordinate == coordinate) releaseRecords;
      in
      if builtins.length matches == 1 then
        builtins.head matches
      else
        throw ''
          cognipilot.product.defaultTarget `${coordinate}` is not a selected
          deployable target with an immutable release output
        '';
in
{
  imports = [ ../nixspace/index-module.nix ];

  options.cognipilot.product = {
    name = lib.mkOption {
      type = lib.types.addCheck lib.types.str (
        value: builtins.match "^[a-z][a-z0-9]*([._-][a-z0-9]+)*$" value != null
      );
      default = "cognipilot";
      description = "Stable ID for the composed immutable product root.";
    };

    defaultTarget = lib.mkOption {
      type = lib.types.nullOr (
        lib.types.submodule (_: {
          options = {
            packageId = lib.mkOption {
              type = lib.types.str;
              description = "Canonical package ID of the natural default product target.";
            };
            targetId = lib.mkOption {
              type = lib.types.str;
              description = "Target ID of the natural default product target.";
            };
          };
        })
      );
      default = null;
      description = ''
        Explicit natural deployable target exported as packages.default.
        CI and aggregate workspace roots are never implicit defaults.
      '';
    };

    promotionVisibilities = lib.mkOption {
      type = lib.types.nonEmptyListOf (
        lib.types.enum [
          "public"
          "private"
        ]
      );
      default = [
        "public"
        "private"
      ];
      apply = value: lib.unique value;
      description = ''
        Source visibility classes included in immutable product roots,
        promotion metadata, SBOMs and attestations. Projects outside this
        projection remain available to editable development tooling without
        becoming public release dependencies.
      '';
    };
  };

  config = {
    nixspace.index = nixspaceIndex;
    flake.cognipilotIndex = normalizedIndex;

    perSystem =
      {
        config,
        pkgs,
        system,
        ...
      }:
      let
        resolveRelease =
          record:
          let
            providerName = record.release.provider;
            outputName = record.release.package;
            provider =
              if builtins.hasAttr providerName inputs then
                inputs.${providerName}
              else
                throw "release target `${record.coordinate}` references missing root flake input `${providerName}`";
            systemPackages =
              if provider ? packages && builtins.hasAttr system provider.packages then
                provider.packages.${system}
              else
                throw "release provider input `${providerName}` for target `${record.coordinate}` does not export packages.${system}";
          in
          if builtins.hasAttr outputName systemPackages then
            systemPackages.${outputName}
          else
            throw "release provider input `${providerName}` for target `${record.coordinate}` does not export packages.${system}.${outputName}";

        resolvedRecords = map (
          record:
          let
            derivation = resolveRelease record;
            providerVersion = derivation.version or null;
            declaredVersion = record.softwareVersion.value;
          in
          record
          // {
            inherit derivation declaredVersion providerVersion;
          }
        ) releaseRecords;

        releaseVersionErrors = lib.concatMap (
          record:
          lib.optional (record.providerVersion == null) (
            "release `${record.coordinate}` provider output does not declare a `version`"
          )
          ++
            lib.optional
              (record.softwareVersion.source == "literal" && record.declaredVersion != record.providerVersion)
              "release `${record.coordinate}` declares software version `${toString record.declaredVersion}` but provider output version is `${toString record.providerVersion}`"
        ) resolvedRecords;

        outputIdentity = derivation: {
          drvPath = derivation.drvPath;
          storePath = toString derivation;
        };

        resolvedReleaseFor =
          coordinate:
          let
            matches = builtins.filter (record: record.coordinate == coordinate) resolvedRecords;
          in
          if builtins.length matches == 1 then builtins.head matches else null;

        promotionProjects = lib.mapAttrsToList (
          projectId: project:
          let
            selectedProject = selectedProjects.${projectId};
            definitionInput =
              if selectedProject.definition.input == null then
                selectedProject.source.input
              else
                selectedProject.definition.input;
            sourceIdentity = selectedInputIdentity project.source.input;
            definitionIdentity = selectedInputIdentity definitionInput;
            targetRecords = lib.mapAttrsToList (
              targetId: target:
              let
                coordinate = "${project.packageId}:${targetId}";
                resolvedRelease = resolvedReleaseFor coordinate;
              in
              {
                id = targetId;
                inherit (target) variants;
                promotionStatus = if target.release == null then "not-released" else "deployable";
                release =
                  if target.release == null then
                    null
                  else
                    {
                      inherit (target) release;
                      providerIdentity = selectedInputIdentity target.release.provider;
                      version = resolvedRelease.providerVersion;
                      output = outputIdentity resolvedRelease.derivation;
                      system = system;
                    };
              }
            ) project.targets;
            adapters = builtins.sort builtins.lessThan (
              lib.unique (
                lib.concatLists (
                  lib.mapAttrsToList (
                    _: target: map (action: action.adapter) (builtins.attrValues target.actions)
                  ) project.targets
                )
              )
            );
          in
          {
            inherit
              adapters
              projectId
              ;
            packageId = project.packageId;
            softwareVersion = project.softwareVersion;
            inherit (project) owner license;
            schema = {
              projectInterfaceVersion = normalizedIndex.interfaceVersion;
            };
            deployability = project.deployability;
            promotionStatus =
              if project.deployability == "deployable" then
                "deployable"
              else if project.deployability == "qualification" then
                "qualification-only"
              else
                "local-only";
            source = {
              inherit (project.source) root visibility;
              identity = sourceIdentity;
            };
            definition = {
              inherit (selectedProject.definition) origin;
              identity = definitionIdentity;
            };
            targets = targetRecords;
          }
        ) promotedProjects;

        deployablePromotionProjects = builtins.filter (
          project: project.deployability == "deployable"
        ) promotionProjects;

        immutableIdentityErrors =
          context: identity:
          lib.optional (!identity.immutable) (
            "${context} input `${identity.input}` is ${identity.status}; deployable promotion requires an exact clean locked identity"
          );

        promotionErrors =
          lib.optional (productLock == null) (
            "product `${cfg.name}` has no committed flake.lock in its selected flake source"
          )
          ++ releaseVersionErrors
          ++ lib.optionals (deployablePromotionProjects != [ ]) (
            immutableIdentityErrors "product `${cfg.name}`" (selectedInputIdentity "self")
          )
          ++ lib.concatLists (
            map (
              project:
              immutableIdentityErrors "package `${project.packageId}` source" project.source.identity
              ++ immutableIdentityErrors "package `${project.packageId}` definition" project.definition.identity
              ++ lib.concatLists (
                map (
                  target:
                  if target.release == null then
                    [ ]
                  else
                    immutableIdentityErrors "release `${project.packageId}:${target.id}` provider" target.release.providerIdentity
                    ++ lib.optional (
                      !lib.hasPrefix "/nix/store/" target.release.output.drvPath
                      || !lib.hasPrefix "/nix/store/" target.release.output.storePath
                    ) "release `${project.packageId}:${target.id}` is not an immutable Nix store output"
                ) project.targets
              )
            ) deployablePromotionProjects
          );

        targetPackages = builtins.listToAttrs (
          map (record: {
            name = record.packageName;
            value = record.derivation;
          }) resolvedRecords
        );
        releaseLinks = map (record: {
          name = "${record.packageId}--${record.targetId}";
          path = record.derivation;
        }) resolvedRecords;
        publicReleaseLinks = map (record: {
          name = "${record.packageId}--${record.targetId}";
          path = record.derivation;
        }) (builtins.filter (record: record.visibility == "public") resolvedRecords);
        allReleaseOutputsArePublic = builtins.length publicReleaseLinks == builtins.length releaseLinks;
        publicSelectedInputLinks =
          lib.optional (inputs ? self) {
            name = "input-product-root";
            path = inputs.self;
          }
          ++ lib.concatMap (project: [
            {
              name = "input-${project.packageId}--source";
              path = project.source.identity.storePath;
            }
            {
              name = "input-${project.packageId}--definition";
              path = project.definition.identity.storePath;
            }
          ]) (builtins.filter (project: project.source.visibility == "public") promotionProjects);

        productName = cfg.name;
        productPackageName = "product-${productName}";
        nixspace = config.packages.nixspace;
        nixspaceIndexPackage = config.packages.nixspace-index;
        selectedLaunchCoordinates = rootConfig.cognipilot.devenvLaunches.launches or [ ];
        publicLaunchCheckNames = lib.concatMap (
          project:
          if project.source.visibility != "public" then
            [ ]
          else
            map (launchId: "launch-${project.packageId}--${launchId}-config") (
              builtins.filter (
                launchId:
                selectedLaunchCoordinates == [ ]
                || builtins.elem "${project.packageId}:${launchId}" selectedLaunchCoordinates
              ) (builtins.attrNames project.launches)
            )
        ) (builtins.attrValues normalizedIndex.projects);
        publicCheckNames = builtins.filter (name: builtins.hasAttr name config.checks) (
          [
            "cognipilot-compliance"
            "cognipilot-contract-tests"
            "cognipilot-promotion-attestation"
            "cognipilot-promotion-record"
            "cognipilot-promotion-sbom"
            "cognipilot-workspace-policy"
            "nixspace-interface"
            "nixspace-standalone"
            "nixspace-west-plan"
          ]
          ++ publicLaunchCheckNames
        );
        publicCheckLinks = map (name: {
          name = "check-${name}";
          path = config.checks.${name};
        }) publicCheckNames;
        productRoot = pkgs.linkFarm "${productName}-product-root" releaseLinks;
        # A public product can retain packages.workspace itself. Mixed-visibility
        # products instead get an equivalent aggregate containing only their
        # public promoted outputs; the public cache must never cross that
        # boundary merely to make the aggregate path available.
        publicWorkspace =
          if allReleaseOutputsArePublic then
            productRoot
          else
            pkgs.linkFarm "${productName}-public-workspace" publicReleaseLinks;
        workspace = productRoot;
        sccacheTools =
          pkgs.linkFarm "${productName}-sccache-tools" [
            {
              name = "cargo";
              path = pkgs.cargo;
            }
            {
              name = "jq";
              path = pkgs.jq;
            }
            {
              name = "rustc";
              path = pkgs.rustc;
            }
            {
              name = "sccache";
              path = pkgs.sccache;
            }
          ]
          // {
            sccacheVersion = pkgs.sccache.version;
          };
        publicCacheRoot = pkgs.linkFarm "${productName}-public-cache-root" (
          [
            {
              name = "workspace";
              path = publicWorkspace;
            }
            {
              name = "nixspace-index";
              path = nixspaceIndexPackage;
            }
            {
              name = "nixspace";
              path = nixspace;
            }
            {
              name = "nixspace-completions";
              path = config.packages.nixspace-completions;
            }
            {
              name = "sccache-tools";
              path = sccacheTools;
            }
            {
              # `./setup` realizes this exact convenience wrapper next.  Its
              # references also retain every generated plan used at runtime.
              name = "ws";
              path = config.packages.ws;
            }
          ]
          ++ lib.optional (config.packages ? nixspace-host) {
            # Products that import the generic host interface retain the exact
            # app wrapper `setup` realizes before `ws` exists. Pure release
            # consumers need not import or synthesize a host policy.
            name = "nixspace-host";
            path = config.packages.nixspace-host;
          }
          ++ lib.optional (config.packages ? nixspace-benchmark-plan) {
            name = "nixspace-benchmark-plan";
            path = config.packages.nixspace-benchmark-plan;
          }
          ++ lib.optional (config.packages ? nixspace-host-plan) {
            name = "nixspace-host-plan";
            path = config.packages.nixspace-host-plan;
          }
          ++ lib.optional (config.packages ? nixspace-launch-plan) {
            name = "nixspace-launch-plan";
            path = config.packages.nixspace-launch-plan;
          }
          ++ lib.optional (config.packages ? nixspace-resolution-plan) {
            name = "nixspace-resolution-plan";
            path = config.packages.nixspace-resolution-plan;
          }
          ++ lib.optional (config.packages ? nixspace-source-plan) {
            name = "nixspace-source-plan";
            path = config.packages.nixspace-source-plan;
          }
          ++ lib.optional (config.packages ? nixspace-west-plan) {
            name = "nixspace-west-plan";
            path = config.packages.nixspace-west-plan;
          }
          ++ publicSelectedInputLinks
          ++ publicReleaseLinks
          ++ publicCheckLinks
        );

        productSourceIdentity = selectedInputIdentity "self";
        nixpkgsIdentity = selectedInputIdentity "nixpkgs";
        targetOutputRecords = map (
          record:
          {
            inherit (record) coordinate packageId targetId;
            inherit (record.release) package provider;
            inherit system;
          }
          // outputIdentity record.derivation
        ) resolvedRecords;
        promotionOutputs = {
          workspace = outputIdentity workspace;
          product = if releaseRecords == [ ] then null else outputIdentity productRoot;
          targets = targetOutputRecords;
        };
        promotionSelection = {
          product = {
            id = productName;
            interfaceVersion = normalizedIndex.interfaceVersion;
            sourceIdentity = productSourceIdentity;
            inherit system;
          };
          packages = promotionProjects;
          outputs = promotionOutputs;
        };
        selectionDigest = builtins.hashString "sha256" (builtins.toJSON promotionSelection);
        builderIdentity = {
          # SLSA builder.id identifies the generic build platform trust
          # boundary, not one helper executed by it. Evaluation and realization
          # may use different host Nix versions, so the pure statement does not
          # invent a daemon version. The exact materializer remains a separately
          # proven builder dependency.
          id = "urn:nix:builder:${system}";
          version = {
            materializer = nixspace.version or "unknown";
          };
          builderDependencies = [
            {
              name = nixspace.pname or "nixspace";
              uri = "urn:nix:output:${builtins.baseNameOf (toString nixspace)}";
              digest.sha256 = null;
              annotations = {
                nixOutput = outputIdentity nixspace;
                inherit system;
                nixpkgs = nixpkgsIdentity;
              };
            }
          ];
        };

        spdxId = value: "SPDXRef-${builtins.replaceStrings [ "_" ] [ "-" ] value}";
        spdxTimestamp =
          value:
          if value != null && builtins.isString value && builtins.stringLength value == 14 then
            "${builtins.substring 0 4 value}-${builtins.substring 4 2 value}-${builtins.substring 6 2 value}T${builtins.substring 8 2 value}:${builtins.substring 10 2 value}:${builtins.substring 12 2 value}Z"
          else
            # A deterministic timestamp is preferable to introducing the
            # build clock into an otherwise reproducible software inventory.
            "1970-01-01T00:00:00Z";
        narHashHex =
          narHash:
          builtins.convertHash {
            hash = narHash;
            hashAlgo = "sha256";
            toHashFormat = "base16";
          };
        sourceLocator =
          identity:
          if (identity.lock.type or null) == "path" then
            "urn:nix:input:${identity.input}:sha256:${narHashHex identity.narHash}"
          else if identity.canonical != null then
            identity.canonical
          else if identity.revision != null then
            "urn:${identity.revision.kind}:${identity.revision.value}"
          else
            "NOASSERTION";
        spdxDocumentNamespace = "https://spdx.org/spdxdocs/${productName}-${system}-${selectionDigest}";
        spdxExternalReferenceType = name: "${spdxDocumentNamespace}#LocationRef-${name}";
        spdxPackageForProject =
          project:
          let
            version =
              if project.softwareVersion.value != null then
                project.softwareVersion.value
              else if project.source.identity.revision != null then
                project.source.identity.revision.value
              else
                "NOASSERTION";
            releaseOutputs = lib.concatMap (
              target:
              lib.optional (target.release != null) {
                referenceCategory = "OTHER";
                referenceType = spdxExternalReferenceType "nix-store-output";
                referenceLocator = target.release.output.storePath;
              }
            ) project.targets;
          in
          {
            SPDXID = spdxId "Package-${project.packageId}";
            name = project.packageId;
            versionInfo = version;
            downloadLocation = sourceLocator project.source.identity;
            filesAnalyzed = false;
            licenseConcluded = "NOASSERTION";
            licenseDeclared = if project.license.spdx == null then "NOASSERTION" else project.license.spdx;
            copyrightText = "NOASSERTION";
            supplier = if project.owner == null then "NOASSERTION" else "Organization: ${project.owner}";
            externalRefs = [
              {
                referenceCategory = "OTHER";
                referenceType = spdxExternalReferenceType "nix-input";
                referenceLocator = sourceLocator project.source.identity;
              }
            ]
            ++ releaseOutputs;
          };
        productSpdxId = spdxId "Product-${productName}";
        projectSpdxPackages = map spdxPackageForProject promotionProjects;
        spdxDocumentData = {
          spdxVersion = "SPDX-2.3";
          dataLicense = "CC0-1.0";
          SPDXID = "SPDXRef-DOCUMENT";
          name = "${productName}-${system}";
          documentNamespace = spdxDocumentNamespace;
          creationInfo = {
            created = spdxTimestamp (productSourceIdentity.sourceInfo.lastModifiedDate or null);
            creators = [ "Tool: nixspace-${builderIdentity.version.materializer}" ];
          };
          documentDescribes = [ productSpdxId ];
          packages = [
            {
              SPDXID = productSpdxId;
              name = productName;
              versionInfo =
                if productSourceIdentity.revision == null then
                  selectionDigest
                else
                  productSourceIdentity.revision.value;
              downloadLocation = sourceLocator productSourceIdentity;
              filesAnalyzed = false;
              licenseConcluded = "NOASSERTION";
              licenseDeclared = "NOASSERTION";
              copyrightText = "NOASSERTION";
              externalRefs = [
                {
                  referenceCategory = "OTHER";
                  referenceType = spdxExternalReferenceType "nix-store-output";
                  referenceLocator = promotionOutputs.workspace.storePath;
                }
              ];
            }
          ]
          ++ projectSpdxPackages;
          relationships = map (project: {
            spdxElementId = productSpdxId;
            relationshipType = "DEPENDS_ON";
            relatedSpdxElement = spdxId "Package-${project.packageId}";
          }) promotionProjects;
          annotations = [
            {
              annotationType = "OTHER";
              annotator = "Tool: nixspace";
              annotationDate = spdxTimestamp (productSourceIdentity.sourceInfo.lastModifiedDate or null);
              comment = "Nix product selection sha256:${selectionDigest}; exact source tree revisions and immutable outputs are declared by package external references.";
            }
          ];
        };
        promotionSbomPackage = pkgs.writeTextFile {
          name = "${productName}-sbom-spdx-json";
          destination = "/share/cognipilot/sbom.spdx.json";
          text = builtins.unsafeDiscardStringContext (builtins.toJSON spdxDocumentData);
          passthru.document = spdxDocumentData;
        };

        promotionClosureRoots = [
          workspace
          promotionSbomPackage
          nixspace
        ];

        promotionRecordData = promotionSelection // {
          schemaVersion = 2;
          inherit builderIdentity selectionDigest;
          documents.sbom = {
            mediaType = "application/spdx+json";
            format = "SPDX-2.3";
            output = outputIdentity promotionSbomPackage;
            path = "share/cognipilot/sbom.spdx.json";
          };
          immutableDependencyClosure = {
            roots = map outputIdentity promotionClosureRoots;
            storePaths = null;
          };
          allowedExternalSourceExceptions = [ ];
        };

        promotionRecordPlan = pkgs.writeText "${productName}-promotion-record-plan.json" (
          # Store paths in the promotion record are data. The closure is
          # selected explicitly through exportReferencesGraph; retaining Nix
          # string context here could accidentally add a private source to a
          # public build merely because its path is mentioned in JSON.
          builtins.unsafeDiscardStringContext (
            builtins.toJSON {
              apiVersion = "nixspace/v1";
              kind = "ClosureMaterialization";
              interfaceVersion = 2;
              proofBindings = lib.imap0 (index: _: {
                sourceOutputPointer = "/builderIdentity/builderDependencies/${toString index}/annotations/nixOutput";
                destinationDigestPointer = "/builderIdentity/builderDependencies/${toString index}/digest/sha256";
                transform = "nix-nar-sha256-to-hex";
              }) builderIdentity.builderDependencies;
              closureAttribute = "productClosure";
              closureRecordsPointer = "/immutableDependencyClosure/storePaths";
              document = promotionRecordData;
            }
          )
        );

        promotionRecordPackage =
          if promotionErrors != [ ] then
            throw ''
              CogniPilot promotion record violations:
              - ${lib.concatStringsSep "\n- " promotionErrors}
            ''
          else
            pkgs.stdenvNoCC.mkDerivation {
              name = "${productName}-promotion-record";
              dontUnpack = true;
              __structuredAttrs = true;
              exportReferencesGraph.productClosure = promotionClosureRoots;
              buildCommand = ''
                ${lib.getExe' nixspace "nixspace"} _materialize-closure \
                  --plan-file ${promotionRecordPlan} \
                  --structured-attrs "$NIX_ATTRS_JSON_FILE" \
                  --output "''${outputs[out]}/share/cognipilot/promotion-record.json"
              '';
              passthru.normalizedRecord = promotionRecordData;
            };

        attestedResource = name: output: {
          inherit name;
          # ClosureMaterialization v2 replaces this null with the exact NAR
          # SHA-256 in lowercase hex after proving `annotations.nixOutput`.
          digest.sha256 = null;
          annotations.nixOutput = output;
        };
        attestedSubjects = [
          (attestedResource "${productName}:workspace" promotionOutputs.workspace)
        ]
        ++ lib.optional (promotionOutputs.product != null) (
          attestedResource "${productName}:product" promotionOutputs.product
        )
        ++ map (output: attestedResource output.coordinate output) targetOutputRecords;
        resolvedInputDependency =
          name: identity:
          lib.optional (identity.narHash != null && identity.revision != null) {
            inherit name;
            uri = sourceLocator identity;
            digest = {
              nixNarSha256 = narHashHex identity.narHash;
            }
            // lib.optionalAttrs (identity.revision.kind == "git-commit") {
              gitCommit = identity.revision.value;
            };
            annotations.nix = {
              narHashSRI = identity.narHash;
              revision = identity.revision;
            };
          };
        resolvedSourceDependencies = lib.concatMap (
          project:
          resolvedInputDependency "${project.packageId}:source" project.source.identity
          ++ resolvedInputDependency "${project.packageId}:definition" project.definition.identity
          ++ lib.concatMap (
            target:
            lib.optionals (target.release != null) (
              resolvedInputDependency "${project.packageId}:${target.id}:release-provider" target.release.providerIdentity
            )
          ) project.targets
        ) promotionProjects;
        promotionAttestationRoots = [
          workspace
          promotionSbomPackage
          promotionRecordPackage
          nixspace
        ];
        promotionAttestationByproducts = [
          (attestedResource "${productName}:promotion-record" (outputIdentity promotionRecordPackage))
          (attestedResource "${productName}:spdx-sbom" (outputIdentity promotionSbomPackage))
        ];
        promotionAttestationData = {
          _type = "https://in-toto.io/Statement/v1";
          subject = attestedSubjects;
          predicateType = "https://slsa.dev/provenance/v1";
          predicate = {
            buildDefinition = {
              buildType = "https://nixos.org/manual/nix/stable/protocols/derivation-aterm";
              externalParameters = {
                product = productName;
                inherit system selectionDigest;
                flakeLock = productSourceIdentity;
              };
              internalParameters = {
                projectInterfaceVersion = normalizedIndex.interfaceVersion;
                packages = map (project: {
                  inherit (project) adapters packageId softwareVersion;
                }) promotionProjects;
              };
              resolvedDependencies = resolvedSourceDependencies;
            };
            runDetails = {
              builder = builderIdentity;
              metadata = {
                cognipilot_nixClosure = null;
              };
              byproducts = promotionAttestationByproducts;
            };
          };
        };
        promotionAttestationPlan = pkgs.writeText "${productName}-promotion-attestation-plan.json" (
          builtins.unsafeDiscardStringContext (
            builtins.toJSON {
              apiVersion = "nixspace/v1";
              kind = "ClosureMaterialization";
              interfaceVersion = 2;
              proofBindings =
                (lib.imap0 (index: _: {
                  sourceOutputPointer = "/predicate/runDetails/builder/builderDependencies/${toString index}/annotations/nixOutput";
                  destinationDigestPointer = "/predicate/runDetails/builder/builderDependencies/${toString index}/digest/sha256";
                  transform = "nix-nar-sha256-to-hex";
                }) builderIdentity.builderDependencies)
                ++ lib.imap0 (index: _: {
                  sourceOutputPointer = "/subject/${toString index}/annotations/nixOutput";
                  destinationDigestPointer = "/subject/${toString index}/digest/sha256";
                  transform = "nix-nar-sha256-to-hex";
                }) attestedSubjects
                ++ lib.imap0 (index: _: {
                  sourceOutputPointer = "/predicate/runDetails/byproducts/${toString index}/annotations/nixOutput";
                  destinationDigestPointer = "/predicate/runDetails/byproducts/${toString index}/digest/sha256";
                  transform = "nix-nar-sha256-to-hex";
                }) promotionAttestationByproducts;
              closureAttribute = "attestedClosure";
              closureRecordsPointer = "/predicate/runDetails/metadata/cognipilot_nixClosure";
              document = promotionAttestationData;
            }
          )
        );
        promotionAttestationPackage = pkgs.stdenvNoCC.mkDerivation {
          name = "${productName}-promotion-attestation";
          dontUnpack = true;
          __structuredAttrs = true;
          exportReferencesGraph.attestedClosure = promotionAttestationRoots;
          buildCommand = ''
            ${lib.getExe' nixspace "nixspace"} _materialize-closure \
              --plan-file ${promotionAttestationPlan} \
              --structured-attrs "$NIX_ATTRS_JSON_FILE" \
              --output "''${outputs[out]}/share/cognipilot/provenance.intoto.json"
          '';
          passthru.statement = promotionAttestationData;
        };

        showIndex = pkgs.writeShellScript "nixspace-show-index" ''
          exec ${pkgs.coreutils}/bin/cat ${nixspaceIndexPackage}/share/nixspace/index.json
        '';
      in
      {
        packages = {
          inherit workspace;
          public-workspace = publicWorkspace;
          public-cache-root = publicCacheRoot;
          sccache-tools = sccacheTools;
          promotion-record = promotionRecordPackage;
          promotion-sbom = promotionSbomPackage;
          promotion-attestation = promotionAttestationPackage;
        }
        // targetPackages
        // lib.optionalAttrs (releaseRecords != [ ]) {
          ${productPackageName} = productRoot;
        }
        // lib.optionalAttrs (selectedDefault != null) {
          default = resolveRelease selectedDefault;
        };

        checks = {
          cognipilot-promotion-record = promotionRecordPackage;
          cognipilot-promotion-sbom = promotionSbomPackage;
          cognipilot-promotion-attestation = promotionAttestationPackage;
        };

        apps.show-index = {
          type = "app";
          program = "${showIndex}";
        };

        formatter = pkgs.nixfmt;
      };
  };
}
