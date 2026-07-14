{
  actionRecords,
  artifactConsumerClaims,
  artifactInputRecords,
  artifactOutputRecords,
  executableRecords,
  graphArtifactRecords,
  graphDependencyRecords,
  graphTargetIndex,
  lib,
  normalizedProjects,
  packageIds,
  resourceRecords,
  staticLaunchPlans,
}:

let
  inherit (lib)
    all
    concatLists
    filter
    length
    mapAttrs
    mapAttrs'
    mapAttrsToList
    nameValuePair
    splitString
    unique
    ;

  coordinatePackage =
    coordinate:
    let
      slashParts = splitString "/" coordinate;
      colonParts = splitString ":" coordinate;
    in
    if builtins.length slashParts >= 2 then
      builtins.head slashParts
    else if builtins.length colonParts >= 2 then
      builtins.head colonParts
    else
      throw "resolution coordinate `${coordinate}` must identify a package";

  launchReferencedPackages =
    launchPlan:
    unique (
      map coordinatePackage (
        launchPlan.requiredArtifacts
        ++ launchPlan.requiredResources
        ++ concatLists (
          map (process: [
            process.executable
            process.artifact
          ]) launchPlan.processes
        )
      )
    );

  packageLaunchDependencyEdges = unique (
    concatLists (
      mapAttrsToList (
        _: launchPlan:
        let
          owner = coordinatePackage launchPlan.launch;
        in
        map (dependency: {
          from = dependency;
          to = owner;
        }) (filter (dependency: dependency != owner) (launchReferencedPackages launchPlan))
      ) staticLaunchPlans
    )
  );

  normalizedProjectByPackage = builtins.listToAttrs (
    mapAttrsToList (_: project: {
      name = project.packageId;
      value = project;
    }) normalizedProjects
  );

  packageDependencyEdges = unique (
    packageLaunchDependencyEdges
    ++ map (
      record:
      let
        producer = graphTargetIndex.${record.producerSchemaCoordinate};
        consumer = graphTargetIndex.${record.consumerSchemaCoordinate};
      in
      {
        from = producer.packageId;
        to = consumer.packageId;
      }
    ) graphDependencyRecords
  );
  packageArtifactDependencyEdges = unique (
    map (
      record:
      let
        producer = graphTargetIndex.${record.producerSchemaCoordinate};
        consumer = graphTargetIndex.${record.consumerSchemaCoordinate};
      in
      {
        from = producer.packageId;
        to = consumer.packageId;
      }
    ) graphArtifactRecords
  );
  directPackageDependencies =
    packageId:
    unique (map (edge: edge.from) (filter (edge: edge.to == packageId) packageDependencyEdges));
  directPackageReverseDependencies =
    packageId:
    unique (map (edge: edge.to) (filter (edge: edge.from == packageId) packageDependencyEdges));
  directPackageArtifactReverseDependencies =
    packageId:
    unique (map (edge: edge.to) (filter (edge: edge.from == packageId) packageArtifactDependencyEdges));
  packageClosure =
    adjacent: packageId:
    builtins.foldl' (reached: _: unique (reached ++ concatLists (map adjacent reached))) [ packageId ] (
      builtins.genList (_: null) (length packageIds)
    );
  packageDependencyClosure =
    packageId: builtins.sort builtins.lessThan (packageClosure directPackageDependencies packageId);
  packageReverseClosure =
    packageId:
    builtins.sort builtins.lessThan (
      filter (candidate: candidate != packageId) (
        packageClosure directPackageReverseDependencies packageId
      )
    );
  packageArtifactReverseClosure =
    packageId:
    builtins.sort builtins.lessThan (
      filter (candidate: candidate != packageId) (
        packageClosure directPackageArtifactReverseDependencies packageId
      )
    );

  releaseTargetsFor =
    project:
    filter (record: record.release != null) (
      mapAttrsToList (targetId: target: {
        inherit targetId;
        inherit (target) release;
      }) project.targets
    );
  primaryReleaseFor =
    project:
    let
      releases = releaseTargetsFor project;
      defaultRelease =
        if builtins.hasAttr "default" project.targets then project.targets.default.release else null;
    in
    if project.deployability != "deployable" then
      null
    else if defaultRelease != null then
      {
        targetId = "default";
        release = defaultRelease;
      }
    else if length releases == 1 then
      builtins.head releases
    else
      null;
  lockedReference = packageId: targetId: release: relativePath: {
    kind = "nix-output-reference";
    deployable = true;
    installable = ".#target-${packageId}--${targetId}";
    inherit (release) package provider;
    inherit relativePath targetId;
    provenance = {
      kind = "locked-output";
      label = "LOCKED";
      inherit (release) package provider;
    };
  };
  localPackageCandidate = project: {
    kind = "local-worktree";
    deployable = false;
    prefix = {
      kind = "source-relative";
      sourceInput = project.source.input;
      inherit (project) repositoryId;
      sourceRoot = project.source.root;
    };
    provenance = {
      kind = "local-git";
      cleanLabel = "LOCAL commit";
      dirtyLabel = "LOCAL dirty";
      sourceInput = project.source.input;
    };
  };
  lockedPackageCandidate =
    project:
    let
      release = primaryReleaseFor project;
    in
    if release == null then
      null
    else
      lockedReference project.packageId release.targetId release.release ".";

  resolutionPackages = mapAttrs' (
    _: project:
    nameValuePair project.packageId {
      inherit (project) packageId;
      candidates = {
        local = localPackageCandidate project;
        locked = lockedPackageCandidate project;
      };
    }
  ) normalizedProjects;

  artifactConsumersFor =
    coordinate:
    builtins.sort builtins.lessThan (
      unique (
        map (claim: claim.coordinate) (filter (claim: claim.artifact == coordinate) artifactConsumerClaims)
      )
    );
  resolutionArtifacts = builtins.listToAttrs (
    map (
      record:
      let
        project = normalizedProjectByPackage.${record.packageId};
        target = project.targets.${record.targetId};
        producerTask = "${record.packageId}:${record.targetId}:${record.producingAction}";
      in
      {
        name = record.coordinate;
        value = {
          inherit (record)
            artifactId
            coordinate
            packageId
            targetId
            ;
          inherit (record.artifact) contract kind;
          consumers = artifactConsumersFor record.coordinate;
          candidates = {
            local = {
              kind = "published-generation";
              relativePath = record.artifact.path;
              generation = {
                inherit producerTask;
                store = {
                  kind = "devenv-task-input";
                  field = "generation";
                };
                pointer = {
                  apiVersion = "nixspace/v1";
                  kind = "ActionGenerationPointer";
                  interfaceVersion = 1;
                  file = "current";
                  identity = {
                    kind = "action-task-input";
                    task = producerTask;
                  };
                };
              };
            };
            locked =
              if project.deployability == "deployable" && target.release != null then
                lockedReference record.packageId record.targetId target.release record.artifact.path
              else
                null;
          };
        };
      }
    ) artifactOutputRecords
  );

  resolutionResources = builtins.listToAttrs (
    map (
      record:
      let
        project = normalizedProjectByPackage.${record.packageId};
        release = primaryReleaseFor project;
      in
      {
        name = "${record.packageId}/${record.resourceId}";
        value = {
          coordinate = "${record.packageId}/${record.resourceId}";
          inherit (record) packageId resourceId;
          inherit (record.resource) kind;
          candidates = {
            local = {
              kind = "source-relative";
              sourceInput = project.source.input;
              inherit (project) repositoryId;
              sourceRoot = project.source.root;
              relativePath = record.resource.path;
            };
            locked =
              if release == null then
                null
              else
                lockedReference record.packageId release.targetId release.release record.resource.path;
          };
        };
      }
    ) resourceRecords
  );

  resolutionExecutables = builtins.listToAttrs (
    map (
      record:
      let
        artifact = resolutionArtifacts.${record.executable.from};
      in
      {
        name = "${record.packageId}/${record.executableId}";
        value = {
          coordinate = "${record.packageId}/${record.executableId}";
          inherit (record) executableId packageId;
          artifact = record.executable.from;
          inherit (record.executable) argv;
          candidates = mapAttrs (
            candidate: value:
            if value == null then
              null
            else
              {
                kind = "artifact-candidate";
                artifact = record.executable.from;
                inherit candidate;
              }
          ) artifact.candidates;
        };
      }
    ) executableRecords
  );

  resolutionActionBindings = builtins.listToAttrs (
    map (
      actionRecord:
      let
        selectedInputs = filter (
          inputRecord:
          inputRecord.consumer == actionRecord.target
          && builtins.elem actionRecord.actionId inputRecord.consumingActions
        ) artifactInputRecords;
      in
      {
        name = actionRecord.coordinate;
        value = {
          inherit (actionRecord) actionId packageId targetId;
          scope = "action";
          artifacts = builtins.listToAttrs (
            map (inputRecord: {
              name = inputRecord.inputId;
              value = {
                artifact = inputRecord.input.from;
                inherit (inputRecord.input) contract;
                environment =
                  if inputRecord.input.environment == null then
                    null
                  else
                    {
                      name = inputRecord.input.environment;
                      value = {
                        kind = "selected-artifact-path";
                        artifact = inputRecord.input.from;
                      };
                    };
              };
            }) selectedInputs
          );
        };
      }
    ) actionRecords
  );

  localCommandScope =
    packageId:
    let
      selected = packageDependencyClosure packageId;
      affected = packageReverseClosure packageId;
    in
    {
      commands = [
        "build"
        "env"
        "launch"
        "package-prefix"
        "resource"
        "run"
        "test"
      ];
      selectedCandidates = builtins.listToAttrs (
        map (selectedPackage: nameValuePair selectedPackage "local") selected
      );
      override = {
        refused = false;
        blockedCommands = [ ];
        affectedReverseClosure = affected;
        requiredRebuild = [ ];
        refusalReason = null;
      };
    };
  lockedCommandScope =
    packageId:
    let
      selected = packageDependencyClosure packageId;
      allLocked = all (
        selectedPackage: resolutionPackages.${selectedPackage}.candidates.locked != null
      ) selected;
    in
    if !allLocked then
      null
    else
      {
        commands = [
          "env"
          "launch"
          "package-prefix"
          "resource"
          "run"
        ];
        selectedCandidates = builtins.listToAttrs (
          map (selectedPackage: nameValuePair selectedPackage "locked") selected
        );
        override = {
          refused = false;
          blockedCommands = [ ];
          affectedReverseClosure = [ ];
          requiredRebuild = [ ];
          refusalReason = null;
        };
      };
  resolutionPackagePlans = builtins.listToAttrs (
    map (
      packageId:
      nameValuePair packageId {
        inherit packageId;
        selectedScope = "local";
        dependencyClosure = packageDependencyClosure packageId;
        reverseClosure = packageReverseClosure packageId;
        compiledReverseClosure = packageArtifactReverseClosure packageId;
        commandScopes = {
          local = localCommandScope packageId;
          locked = lockedCommandScope packageId;
        };
      }
    ) packageIds
  );
  staticResolutionTemplate = {
    apiVersion = "nixspace/v1";
    kind = "WorkspaceResolutionTemplate";
    interfaceVersion = 1;
    packagePlans = resolutionPackagePlans;
    packages = resolutionPackages;
    artifacts = resolutionArtifacts;
    resources = resolutionResources;
    executables = resolutionExecutables;
    actionBindings = resolutionActionBindings;
  };

in
staticResolutionTemplate
