{ lib }:

{
  index,
  nixspaceExecutable,
  taskStateRoot,
  workspaceRoot ? ".",
  sourceBindings ? { },
  toolProfiles ? { },
}:

let
  inherit (builtins)
    attrValues
    elemAt
    hasAttr
    listToAttrs
    map
    toJSON
    ;
  inherit (lib)
    concatLists
    count
    escapeShellArgs
    filterAttrs
    mapAttrs
    mapAttrsToList
    nameValuePair
    removePrefix
    removeSuffix
    splitString
    unique
    ;

  taskName =
    packageId: targetId: actionId:
    "${packageId}:${targetId}:${actionId}";

  joinPath =
    base: relative:
    let
      left = removeSuffix "/" base;
      right = removePrefix "./" relative;
    in
    if right == "." || right == "" then
      left
    else if left == "." || left == "" then
      right
    else
      "${left}/${right}";

  portableTaskStateRoot =
    let
      segments = splitString "/" taskStateRoot;
      portable = builtins.match "[A-Za-z0-9._-]+(/[A-Za-z0-9._-]+)*" taskStateRoot != null;
    in
    if
      portable
      && builtins.all (segment: segment != "." && segment != "..") segments
    then
      taskStateRoot
    else
      throw "taskStateRoot must be a portable workspace-relative path with no empty, `.` or `..` components";

  projects = attrValues index.projects;
  projectsByPackage = listToAttrs (map (project: nameValuePair project.packageId project) projects);

  sourceBase =
    project:
    if hasAttr project.source.input sourceBindings then
      sourceBindings.${project.source.input}
    else
      joinPath workspaceRoot "src/${project.repositoryId}";

  sourceDirectory = project: joinPath (sourceBase project) project.source.root;

  producerTask =
    reference:
    let
      coordinate = splitString ":" reference;
      packageId = elemAt coordinate 0;
      targetId = elemAt coordinate 1;
      producer = producerArtifact reference;
      producerAction = producer.artifact.producedBy;
    in
    taskName packageId targetId producerAction;

  producerArtifact =
    reference:
    let
      coordinate = splitString ":" reference;
      packageId = elemAt coordinate 0;
      targetId = elemAt coordinate 1;
      artifactId = elemAt coordinate 2;
      project = projectsByPackage.${packageId};
    in
    {
      inherit project;
      artifact = project.targets.${targetId}.artifacts.outputs.${artifactId};
    };

  artifactsForAction = target: actionId: {
    outputs = filterAttrs (_: artifact: artifact.producedBy == actionId) target.artifacts.outputs;
    inputs = mapAttrs (
      _: artifact:
      artifact
      // {
        consumedBy = [ actionId ];
      }
    ) (
      filterAttrs (_: artifact: builtins.elem actionId artifact.consumedBy) target.artifacts.inputs
    );
  };

  artifactDependencies =
    artifacts:
    unique (map (artifactInput: producerTask artifactInput.from) (attrValues artifacts.inputs));

  artifactInputPaths = artifacts:
    map (
      artifactInput:
      let
        producer = producerArtifact artifactInput.from;
      in
      joinPath (sourceDirectory producer.project) producer.artifact.path
    ) (attrValues artifacts.inputs);

  artifactEnvironmentPaths =
    taskCoordinate: actionEnvironment: artifacts:
    let
      bindings = map (
        artifactInput:
        let
          producer = producerArtifact artifactInput.from;
        in
        nameValuePair artifactInput.environment (
          joinPath (sourceDirectory producer.project) producer.artifact.path
        )
      ) (
        builtins.filter (artifactInput: artifactInput.environment != null) (
          attrValues artifacts.inputs
        )
      );
      bindingNames = map (binding: binding.name) bindings;
      duplicateBindings = builtins.filter (
        name: count (candidate: candidate == name) bindingNames > 1
      ) (unique bindingNames);
      actionCollisions = builtins.filter (name: hasAttr name actionEnvironment) (
        unique bindingNames
      );
    in
    if duplicateBindings != [ ] then
      throw ''
        task `${taskCoordinate}` has duplicate artifact environment bindings: ${
          builtins.concatStringsSep ", " duplicateBindings
        }
      ''
    else if actionCollisions != [ ] then
      throw ''
        task `${taskCoordinate}` action and artifact environments collide: ${
          builtins.concatStringsSep ", " actionCollisions
        }
      ''
    else
      listToAttrs bindings;

  resolveArgvSegment =
    artifacts: segment:
    if builtins.isString segment then
      segment
    else
      let
        artifactInput = artifacts.inputs.${segment.artifactInput};
        producer = producerArtifact artifactInput.from;
      in
      {
        path = joinPath (sourceDirectory producer.project) producer.artifact.path;
        prefix = segment.prefix or "";
        suffix = segment.suffix or "";
      };

  lockPath = lockId: joinPath portableTaskStateRoot "locks/${lockId}.lock";

  mkTask =
    project: targetId: target: actionId: action:
    let
      packageId = project.packageId;
      coordinate = taskName packageId targetId actionId;
      declaredDependencies = action.dependsOn;
      artifacts = artifactsForAction target actionId;
      toolProfileId = action.toolProfile or null;
      selectedToolProfile =
        if toolProfileId == null then
          {
            pathPrefixes = [ ];
            environment = { };
            environmentPaths = { };
          }
        else if hasAttr toolProfileId toolProfiles then
          toolProfiles.${toolProfileId}
        else
          throw "task `${coordinate}` references unresolved tool profile `${toolProfileId}`";
      profileEnvironment = selectedToolProfile.environment or { };
      profileEnvironmentPaths = selectedToolProfile.environmentPaths or { };
      actionEnvironment = action.environment;
      environmentCollisions = builtins.intersectAttrs profileEnvironment actionEnvironment;
      environment =
        if environmentCollisions != { } then
          throw ''
            task `${coordinate}` action and tool profile environments collide: ${
              builtins.concatStringsSep ", " (builtins.attrNames environmentCollisions)
            }
          ''
        else
          profileEnvironment // actionEnvironment;
      profilePathLiteralCollisions = builtins.intersectAttrs profileEnvironmentPaths environment;
      pathPrefixes = selectedToolProfile.pathPrefixes or [ ];
      after = unique (
        map (dependency: taskName packageId targetId dependency) declaredDependencies
        ++ artifactDependencies artifacts
      );
      cwd = sourceDirectory project;
      artifactPaths = artifactEnvironmentPaths coordinate environment artifacts;
      profileArtifactPathCollisions = builtins.intersectAttrs profileEnvironmentPaths artifactPaths;
      environmentPaths =
        if profilePathLiteralCollisions != { } then
          throw ''
            task `${coordinate}` declares tool-profile path and literal environment values for: ${
              builtins.concatStringsSep ", " (builtins.attrNames profilePathLiteralCollisions)
            }
          ''
        else if profileArtifactPathCollisions != { } then
          throw ''
            task `${coordinate}` tool-profile and artifact environment paths collide: ${
              builtins.concatStringsSep ", " (builtins.attrNames profileArtifactPathCollisions)
            }
          ''
        else
          profileEnvironmentPaths // artifactPaths;
      argv = map (resolveArgvSegment artifacts) action.argv;
      locks = map lockPath (builtins.sort builtins.lessThan action.requirements.exclusiveLocks);
      outputs = builtins.sort (left: right: left.coordinate < right.coordinate) (
        mapAttrsToList (
          artifactId: artifact: {
            coordinate = "${packageId}:${targetId}:${artifactId}";
            path = joinPath cwd artifact.path;
            inherit (artifact) kind contract;
            proof = {
              kind = "nix-nar-sha256";
              argvPrefix = [
                "nix"
                "hash"
                "path"
                "--type"
                "sha256"
                "--sri"
                "--"
              ];
            };
          }
        ) artifacts.outputs
      );
      # Use devenv's native content-hash task cache. It also tracks the
      # generated command path and restores the last successful JSON output;
      # these are the mutable project tree and exact consumed artifacts.
      execIfModified = unique ([ cwd ] ++ artifactInputPaths artifacts);
      taskIdentity = {
        inherit (action) adapter requirements;
        inherit argv environment environmentPaths locks outputs pathPrefixes toolProfileId;
        inherit packageId;
        inherit artifacts;
        inherit (target) variants;
        targetId = targetId;
        actionId = actionId;
        source = {
          inherit (project.source) input root visibility;
          coordinate = "${project.source.input}:${project.source.root}";
          localPath = cwd;
        };
      };
      generationIdentity = {
        apiVersion = "nixspace/v1";
        kind = "ActionTaskIdentity";
        interfaceVersion = 1;
        declaration = taskIdentity;
      };
      generation = {
        apiVersion = "nixspace/v1";
        kind = "ActionGenerationStore";
        interfaceVersion = 1;
        # The portable namespace is stable for one producer task while the
        # complete declaration identity below changes whenever its contract
        # changes. Readers can derive the pointer path from the task coordinate
        # and must still verify the active identity before consuming outputs.
        root = joinPath portableTaskStateRoot "devel/${builtins.hashString "sha256" coordinate}";
        identity = generationIdentity;
      };
      # Devenv hashes the normalized declaration and the exact publication
      # namespace. The identity intentionally excludes generation itself so
      # the typed record is finite and reusable by strict consumers.
      input = taskIdentity // { inherit generation; };
      result = {
        inherit packageId targetId actionId;
        task = coordinate;
        artifacts = artifacts.outputs;
      };
      plan = {
        apiVersion = "nixspace/v1";
        kind = "ActionTask";
        interfaceVersion = 2;
        inherit cwd argv environment environmentPaths generation locks outputs pathPrefixes result;
      };
    in
    nameValuePair coordinate {
      inherit after cwd execIfModified input;
      description = "CogniPilot ${action.kind}: ${packageId}/${targetId}";
      exec = escapeShellArgs [
        nixspaceExecutable
        "_run-task"
        "--plan-json"
        (toJSON plan)
      ];
    };

  targetTasks =
    project: targetId: target:
    mapAttrsToList (actionId: action: mkTask project targetId target actionId action) (
      target.actions
    );

  projectTasks =
    project:
    concatLists (
      mapAttrsToList (targetId: target: targetTasks project targetId target) project.targets
    );
in
listToAttrs (concatLists (map projectTasks projects))
