{
  config,
  inputs ? { },
  lib,
  ...
}:

let
  rootConfig = config;
  cfg = rootConfig.cognipilot.resolution;
  template = rootConfig.cognipilot.validatedIndex.resolutionTemplate;

  configuredWorkspace = lib.attrByPath [ "cognipilot" "devenvWorkspace" ] { } rootConfig;
  sourceWorkspace = lib.attrByPath [ "cognipilot" "sourceWorkspace" ] { } rootConfig;
  workspaceRoot =
    if cfg.workspaceRoot != null then cfg.workspaceRoot else configuredWorkspace.workspaceRoot or ".";
  taskStateRoot =
    if cfg.taskStateRoot != null then
      cfg.taskStateRoot
    else
      configuredWorkspace.taskStateRoot or ".nixspace/state/tasks";
  sourceBindings =
    (sourceWorkspace.paths or { }) // (configuredWorkspace.sourceBindings or { }) // cfg.sourceBindings;

  joinPath =
    left: right:
    if right == "." || right == "" then
      left
    else if left == "." || left == "" then
      right
    else
      "${lib.removeSuffix "/" left}/${lib.removePrefix "./" right}";
  sourceBase =
    candidate:
    if builtins.hasAttr candidate.sourceInput sourceBindings then
      sourceBindings.${candidate.sourceInput}
    else
      joinPath workspaceRoot "src/${candidate.repositoryId}";
  sourceRoot = candidate: joinPath (sourceBase candidate) candidate.sourceRoot;
  sourcePath = candidate: joinPath (sourceRoot candidate) (candidate.relativePath or ".");
  provenanceInspection = candidate: {
    workingDirectory = sourceRoot candidate;
    dirty = {
      argv = [
        "git"
        "status"
        "--porcelain=v1"
        "--untracked-files=normal"
        "--"
        "."
      ];
      cleanWhen = "stdout-empty";
    };
    revision.argv = [
      "git"
      "rev-parse"
      "HEAD"
    ];
  };
  portableTaskStateRootType = lib.types.addCheck lib.types.str (
    value:
    let
      segments = lib.splitString "/" value;
    in
    builtins.match "[A-Za-z0-9._-]+(/[A-Za-z0-9._-]+)*" value != null
    && lib.all (segment: segment != "." && segment != "..") segments
  );
in
{
  imports = [ ../nixspace/tool-module.nix ];

  options.cognipilot.resolution = {
    workspaceRoot = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = ''
        Runtime workspace root embedded in the realized resolution plan.
        The shared devenv workspace value is used when this is unset.
      '';
    };

    taskStateRoot = lib.mkOption {
      type = lib.types.nullOr portableTaskStateRootType;
      default = null;
      description = ''
        Runtime task-state root containing ActionGenerationStore pointers.
        The shared devenv task-state value is used when this is unset.
      '';
    };

    sourceBindings = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = { };
      description = ''
        Root-owned source-path exceptions keyed by source input. Source
        workspace and devenv bindings are inherited before these overrides.
      '';
    };

    selectedScopes = lib.mkOption {
      type = lib.types.attrsOf (
        lib.types.enum [
          "local"
          "locked"
        ]
      );
      default = { };
      description = ''
        Explicit selected command scope by canonical package ID. Unlisted
        packages select the local command scope; no candidate fallback occurs.
      '';
    };

    candidateSelections = lib.mkOption {
      type = lib.types.attrsOf (
        lib.types.attrsOf (
          lib.types.enum [
            "local"
            "locked"
          ]
        )
      );
      default = { };
      description = ''
        Optional exact candidate selection by root package and dependency
        package. Keys must equal the root plan's complete dependency closure.
        A selected candidate must exist; no missing entry or fallback is
        accepted.
      '';
    };
  };

  config.perSystem =
    {
      config,
      pkgs,
      system,
      ...
    }:
    let
      generatedTasks = lib.attrByPath [ "devenv" "shells" "default" "tasks" ] { } config;

      releaseOutput =
        reference:
        if reference == null || !(builtins.hasAttr reference.provider inputs) then
          null
        else
          let
            provider = inputs.${reference.provider};
            systemPackages =
              if provider ? packages && builtins.hasAttr system provider.packages then
                provider.packages.${system}
              else
                { };
          in
          if builtins.hasAttr reference.package systemPackages then
            systemPackages.${reference.package}
          else
            null;
      concreteLocked =
        reference:
        let
          output = releaseOutput reference;
        in
        if output == null then
          null
        else
          reference
          // {
            kind = "nix-store";
            storePath = toString output;
          };

      concretePackage =
        _: package:
        let
          local = package.candidates.local;
          prefix = local.prefix;
        in
        package
        // {
          candidates = {
            local = local // {
              prefix = prefix // {
                workspacePath = sourceRoot prefix;
              };
              provenance = local.provenance // {
                inspection = provenanceInspection prefix;
              };
            };
            locked = concreteLocked package.candidates.locked;
          };
        };
      packages = lib.mapAttrs concretePackage template.packages;

      # A concrete resolution document has one Nix-selected scope for every
      # package root.  Work out those exact selections before materializing
      # artifact candidates so a release-only composition does not need an
      # editable ActionTask generation that no selected command can consume.
      # Selecting a local candidate remains strict: its generation is required
      # below and there is no locked fallback.
      requestedSelectionsFor =
        packageId: plan:
        let
          selectedScope = cfg.selectedScopes.${packageId} or plan.selectedScope;
          selected = plan.commandScopes.${selectedScope};
        in
        if selected == null then
          null
        else
          cfg.candidateSelections.${packageId} or selected.selectedCandidates;
      requestedSelections = lib.mapAttrs requestedSelectionsFor template.packagePlans;
      selectedLocally =
        packageId:
        lib.any (
          selections: selections != null && (selections.${packageId} or null) == "local"
        ) (builtins.attrValues requestedSelections);

      concreteArtifact =
        _: artifact:
        let
          taskName = artifact.candidates.local.generation.producerTask;
          task = generatedTasks.${taskName} or null;
          generation = if task == null then null else task.input.generation or null;
          localRequired = selectedLocally artifact.packageId;
          matchingOutputs =
            if task == null then
              [ ]
            else
              lib.filter (output: output.coordinate == artifact.coordinate) (task.input.outputs or [ ]);
        in
        if generation == null && localRequired then
          throw "resolution artifact `${artifact.coordinate}` has no ActionTask v3 generation for `${taskName}`"
        else if generation != null && builtins.length matchingOutputs != 1 then
          throw "resolution artifact `${artifact.coordinate}` requires exactly one matching ActionTask output"
        else
          artifact
          // {
            candidates = {
              local =
                if generation == null then
                  null
                else
                  artifact.candidates.local
                  // {
                    workspacePath = (builtins.head matchingOutputs).path;
                    generation = {
                      producerTask = taskName;
                      layout = generation.layout;
                      store = {
                        kind = "workspace-relative";
                        workspacePath = generation.root;
                      };
                      pointer = {
                        apiVersion = "nixspace/v1";
                        kind = "ActionGenerationPointer";
                        interfaceVersion = 1;
                        identity = generation.identity;
                      };
                    };
                  };
              locked = concreteLocked artifact.candidates.locked;
            };
          };
      artifacts = lib.mapAttrs concreteArtifact template.artifacts;

      concreteResource =
        _: resource:
        let
          local = resource.candidates.local;
        in
        resource
        // {
          candidates = {
            local = local // {
              workspacePath = sourcePath local;
            };
            locked = concreteLocked resource.candidates.locked;
          };
        };
      resources = lib.mapAttrs concreteResource template.resources;

      concreteExecutable =
        _: executable:
        executable
        // {
          candidates = lib.mapAttrs (
            candidate: value:
            if value == null || artifacts.${executable.artifact}.candidates.${candidate} == null then
              null
            else
              value
          ) executable.candidates;
        };
      executables = lib.mapAttrs concreteExecutable template.executables;

      concretePackagePlan =
        packageId: plan:
        let
          lockedSelections = plan.commandScopes.locked;
          lockedAvailable =
            lockedSelections != null
            && lib.all (selectedPackage: packages.${selectedPackage}.candidates.locked != null) (
              builtins.attrNames lockedSelections.selectedCandidates
            );
          commandScopes = plan.commandScopes // {
            locked = if lockedAvailable then lockedSelections else null;
          };
          selectedScope = cfg.selectedScopes.${packageId} or plan.selectedScope;
          selected = commandScopes.${selectedScope};
          requestedSelections = cfg.candidateSelections.${packageId} or null;
          expectedPackages = builtins.sort builtins.lessThan plan.dependencyClosure;
          requestedPackages =
            if requestedSelections == null then
              [ ]
            else
              builtins.sort builtins.lessThan (builtins.attrNames requestedSelections);
          candidateSelections =
            if requestedSelections == null then selected.selectedCandidates else requestedSelections;
          missingCandidates = lib.filter (
            selectedPackage:
            packages.${selectedPackage}.candidates.${candidateSelections.${selectedPackage}} == null
          ) expectedPackages;
          lockedReverseDependents = lib.unique (
            lib.concatMap (
              selectedPackage:
              if candidateSelections.${selectedPackage} != "local" then
                [ ]
              else
                lib.filter (
                  reversePackage:
                  builtins.hasAttr reversePackage candidateSelections
                  && candidateSelections.${reversePackage} == "locked"
                ) template.packagePlans.${selectedPackage}.compiledReverseClosure
            ) expectedPackages
          );
          selectedKinds = lib.unique (builtins.attrValues candidateSelections);
          mixedSelection = builtins.length selectedKinds > 1;
          rootIsLocal = candidateSelections.${packageId} == "local";
          hasLockedDependency = lib.any (
            selectedPackage: selectedPackage != packageId && candidateSelections.${selectedPackage} == "locked"
          ) expectedPackages;
          unsafeLocalRuntime = rootIsLocal && hasLockedDependency;
          blockedCommands = lib.unique (
            (lib.optionals mixedSelection [
              "build"
              "test"
            ])
            ++ (lib.optionals unsafeLocalRuntime [
              "launch"
              "run"
            ])
            ++ (lib.optionals (lockedReverseDependents != [ ]) selected.commands)
          );
          refusalReasons =
            lib.optional mixedSelection "mixed local/locked selections cannot build or test because generated devenv tasks currently bind local producer paths"
            ++ lib.optional unsafeLocalRuntime "a local root with locked dependencies cannot run or launch because its ActionGeneration identity is not bound to mixed resolution selections"
            ++
              lib.optional (lockedReverseDependents != [ ])
                "local compiled overrides have locked reverse dependents (${lib.concatStringsSep ", " (builtins.sort builtins.lessThan lockedReverseDependents)}); select the affected reverse closure locally";
          refused = blockedCommands != [ ];
          refusalReason = if !refused then null else lib.concatStringsSep "; " refusalReasons;
          selectedOverride = selected.override // {
            inherit refused refusalReason;
            inherit blockedCommands;
            requiredRebuild = lockedReverseDependents;
          };
          selectedRecord = selected // {
            selectedCandidates = candidateSelections;
            override = selectedOverride;
          };
          finalCommandScopes = commandScopes // {
            ${selectedScope} = selectedRecord;
          };
        in
        if selected == null then
          throw "resolution plan `${packageId}` explicitly selects unavailable `${selectedScope}` candidates"
        else if requestedSelections != null && requestedPackages != expectedPackages then
          throw "resolution plan `${packageId}` candidateSelections must name exactly: ${lib.concatStringsSep ", " expectedPackages}"
        else if missingCandidates != [ ] then
          throw "resolution plan `${packageId}` selected missing candidates for: ${lib.concatStringsSep ", " missingCandidates}"
        else
          plan
          // {
            commandScopes = finalCommandScopes;
            inherit selectedScope;
          };
      packagePlans = lib.mapAttrs concretePackagePlan template.packagePlans;

      document = {
        apiVersion = "nixspace/v1";
        kind = "WorkspaceResolution";
        interfaceVersion = 2;
        roots = {
          workspace = workspaceRoot;
          taskState = taskStateRoot;
        };
        inherit
          artifacts
          executables
          packagePlans
          packages
          resources
          ;
        inherit (template) actionBindings;
      };
      planJson = builtins.unsafeDiscardStringContext (builtins.toJSON document);
      resolutionPackage =
        assert builtins.getContext planJson == { };
        (pkgs.writeTextDir "share/nixspace/resolution-plan.json" planJson).overrideAttrs (_: {
          passthru = { inherit document; };
        });
    in
    {
      packages.nixspace-resolution-plan = resolutionPackage;
    };
}
