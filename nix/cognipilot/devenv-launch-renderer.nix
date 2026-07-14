{ lib }:

{
  index,
  launch,
  executableBindings ? { },
  workspaceRoot ? ".",
  sourceBindings ? { },
  environmentPrefix ? "COGNIPILOT_PARAM",
  runtimeClient ? "nixspace",
}:

let
  inherit (builtins)
    attrNames
    attrValues
    elemAt
    hasAttr
    listToAttrs
    map
    match
    ;
  inherit (lib)
    concatLists
    concatStringsSep
    escapeShellArg
    mapAttrs
    mapAttrs'
    mapAttrsToList
    nameValuePair
    optionalAttrs
    removeSuffix
    replaceStrings
    splitString
    ;

  fail = message: throw "CogniPilot devenv launch renderer: ${message}";

  require = condition: message: if condition then true else fail message;

  parseCoordinate =
    kind: coordinate:
    let
      parts = splitString ":" coordinate;
    in
    if builtins.length parts == 2 && builtins.all (part: part != "") parts then
      {
        packageId = elemAt parts 0;
        id = elemAt parts 1;
      }
    else
      fail "${kind} coordinate `${coordinate}` must use package:id form";

  workspaceCoordinate =
    kind: coordinate:
    let
      parsed = parseCoordinate kind coordinate;
    in
    "${parsed.packageId}/${parsed.id}";

  projectsByPackage = listToAttrs (
    map (project: nameValuePair project.packageId project) (attrValues index.projects)
  );

  projectFor =
    coordinate:
    let
      parsed = parseCoordinate "launch" coordinate;
    in
    if hasAttr parsed.packageId projectsByPackage then
      projectsByPackage.${parsed.packageId}
    else
      fail "launch `${coordinate}` refers to unknown package `${parsed.packageId}`";

  launchFor =
    coordinate:
    let
      parsed = parseCoordinate "launch" coordinate;
      project = projectFor coordinate;
    in
    if hasAttr parsed.id project.launches then
      project.launches.${parsed.id}
    else
      fail "unknown launch `${coordinate}`";

  executableFor =
    coordinate:
    let
      parsed = parseCoordinate "executable" coordinate;
    in
    if !hasAttr parsed.packageId projectsByPackage then
      fail "executable `${coordinate}` refers to unknown package `${parsed.packageId}`"
    else
      let
        project = projectsByPackage.${parsed.packageId};
      in
      if hasAttr parsed.id project.executables then
        project.executables.${parsed.id}
      else
        fail "unknown executable `${coordinate}`";

  joinPath =
    base: relative:
    if relative == "." then removeSuffix "/" base else "${removeSuffix "/" base}/${relative}";

  sourceDirectory =
    project:
    let
      sourceBase =
        if hasAttr project.source.input sourceBindings then
          sourceBindings.${project.source.input}
        else
          joinPath workspaceRoot "src/${project.repositoryId}";
    in
    joinPath sourceBase project.source.root;

  upperEnvironmentId = value: lib.toUpper (replaceStrings [ "-" ] [ "_" ] value);

  environmentName =
    source:
    let
      parsed = parseCoordinate "parameter owner" source.coordinate;
    in
    concatStringsSep "_" [
      environmentPrefix
      (upperEnvironmentId parsed.packageId)
      (upperEnvironmentId parsed.id)
      (upperEnvironmentId source.parameterId)
    ];

  parameterSource = coordinate: parameterId: parameter: {
    inherit coordinate parameter parameterId;
    environment = environmentName { inherit coordinate parameterId; };
  };

  parameterContract =
    source:
    let
      secret = source.parameter.type == "secret";
      checked = require (
        !secret || source.parameter.default == null
      ) "secret parameter `${source.coordinate}:${source.parameterId}` must not have a default";
    in
    assert checked;
    {
      inherit (source) coordinate parameterId;
      workspaceCoordinate = workspaceCoordinate "parameter owner" source.coordinate;
      inherit (source.parameter)
        access
        allocation
        allocationHost
        allocationTransport
        allowedRoots
        enumValues
        maximum
        minimum
        mustExist
        required
        type
        ;
      environment = source.environment;
      default = if secret then null else source.parameter.default;
      secret = secret;
    };

  runtimeReference = source: "$" + source.environment;

  renderValue =
    context: parameterSources: value:
    if value.literal != null then
      value.literal
    else if value.parameter == null then
      fail "${context} has neither a literal nor a parameter"
    else if !hasAttr value.parameter parameterSources then
      fail "${context} references unknown parameter `${value.parameter}`"
    else
      value.prefix + runtimeReference parameterSources.${value.parameter} + value.suffix;

  renderArgvValue =
    context: parameterSources: value:
    if value.literal != null then
      escapeShellArg value.literal
    else if value.parameter == null then
      fail "${context} has neither a literal nor a parameter"
    else if !hasAttr value.parameter parameterSources then
      fail "${context} references unknown parameter `${value.parameter}`"
    else
      let
        source = parameterSources.${value.parameter};
        checked = require (
          source.parameter.type != "secret"
        ) "${context} cannot place secret parameter `${value.parameter}` in argv";
      in
      assert checked;
      (if value.prefix == "" then "" else escapeShellArg value.prefix)
      + "\"$"
      + source.environment
      + "\""
      + (if value.suffix == "" then "" else escapeShellArg value.suffix);

  millisecondsToSeconds = value: builtins.div (value + 999) 1000;

  dependencySuffix =
    state:
    {
      started = "started";
      ready = "ready";
      completed = "completed";
    }
    .${state};

  dependencyCondition =
    state:
    {
      started = "process_started";
      ready = "process_healthy";
      succeeded = "process_completed_successfully";
      completed = "process_completed";
    }
    .${state};

  restartPolicy =
    policy:
    {
      never = "never";
      "on-failure" = "on_failure";
      always = "always";
    }
    .${policy};

  shutdownSignal =
    signal:
    {
      SIGINT = 2;
      SIGTERM = 15;
    }
    .${signal};

  renderReadiness =
    context: process: parameterSources:
    if process.readiness.kind != "endpoint" then
      null
    else
      let
        endpoint = process.endpoints.${process.readiness.endpoint};
        supported = builtins.elem endpoint.protocol [
          "tcp"
          "http"
          "https"
          "zenoh"
        ];
        checkedProtocol = require supported "${context} endpoint readiness protocol `${endpoint.protocol}` is unsupported by pinned devenv/process-compose";
        checkedPort = require (
          endpoint.portParameter != null
        ) "${context} endpoint readiness requires a port parameter";
        host =
          if endpoint.hostParameter == null then
            "127.0.0.1"
          else
            runtimeReference parameterSources.${endpoint.hostParameter};
        port =
          if endpoint.portParameter == null then
            ""
          else
            runtimeReference parameterSources.${endpoint.portParameter};
      in
      assert checkedProtocol && checkedPort;
      {
        # Runtime-valued ports cannot inhabit devenv's typed integer HTTP port.
        # Keep the typed readiness hook as one argv invocation; the standalone
        # runtime client owns host/port resolution and probing.
        exec = concatStringsSep " " (
          [
            "exec"
            (escapeShellArg runtimeClient)
            "_probe"
            (if endpoint.protocol == "http" then "http" else "tcp")
            "--host"
            (if endpoint.hostParameter == null then escapeShellArg host else ''"${host}"'')
            "--port"
            ''"${port}"''
          ]
          ++ lib.optionals (endpoint.protocol == "http") [
            "--path"
            (escapeShellArg (if endpoint.path == null then "/" else endpoint.path))
            "--expect-status"
            (toString (if endpoint.expectedStatus == null then 200 else endpoint.expectedStatus))
          ]
        );
        initial_delay = 0;
        period = 1;
        probe_timeout = millisecondsToSeconds process.readiness.timeoutMs;
        timeout = millisecondsToSeconds process.readiness.timeoutMs;
        success_threshold = 1;
        failure_threshold = 3;
      };

  mergeUnique =
    context: left: right:
    let
      collisions = builtins.filter (name: hasAttr name right) (attrNames left);
    in
    if collisions == [ ] then
      left // right
    else
      fail "${context} generated duplicate names: ${concatStringsSep ", " collisions}";

  expandLaunch =
    {
      coordinate,
      prefix ? "",
      forwardedParameters ? { },
      ancestry ? [ ],
    }:
    let
      checkedCycle = require (
        !builtins.elem coordinate ancestry
      ) "launch include cycle reaches `${coordinate}`";
      project = projectFor coordinate;
      launchRecord = launchFor coordinate;
      localParameterSources = mapAttrs (
        parameterId: parameter:
        forwardedParameters.${parameterId} or (parameterSource coordinate parameterId parameter)
      ) launchRecord.parameters;

      ownParameterContracts = mapAttrs' (
        _: source: nameValuePair source.environment (parameterContract source)
      ) localParameterSources;

      processName = processId: "${prefix}${processId}";

      renderProcess =
        processId: process:
        let
          context = "launch `${coordinate}` process `${processId}`";
          executable = executableFor process.executable;
          finalSignalChecked =
            require (process.shutdown.killSignal == "SIGKILL")
              "${context} requests final shutdown signal `${process.shutdown.killSignal}`; process-compose 1.116 has fixed SIGKILL escalation";
          executableArgv = map escapeShellArg executable.argv;
          processArgv = builtins.genList (
            index:
            renderArgvValue "${context} argv[${toString index}]" parameterSources (elemAt process.argv index)
          ) (builtins.length process.argv);
          invocation =
            if hasAttr process.executable executableBindings then
              [
                "exec"
                (escapeShellArg executableBindings.${process.executable})
              ]
            else
              [
                "exec"
                (escapeShellArg runtimeClient)
                "run"
                "--selection-root"
                (escapeShellArg parsedLaunch.packageId)
                (escapeShellArg (workspaceCoordinate "executable" process.executable))
                "--"
              ];
          command = concatStringsSep " " (invocation ++ executableArgv ++ processArgv);
          parameterSources = localParameterSources;
          ordinaryDependencies = lib.filterAttrs (_: state: state != "succeeded") process.dependencies;
          succeededDependencies = lib.filterAttrs (_: state: state == "succeeded") process.dependencies;
          readiness = renderReadiness context process parameterSources;
          sourceRoot = sourceDirectory project;
          processCompose = {
            availability = {
              backoff_seconds = millisecondsToSeconds process.restart.backoffMs;
            };
            shutdown = {
              signal = shutdownSignal process.shutdown.signal;
              timeout_seconds = millisecondsToSeconds process.shutdown.timeoutMs;
            };
          }
          // optionalAttrs (succeededDependencies != { }) {
            depends_on = mapAttrs' (
              dependency: state:
              nameValuePair (processName dependency) {
                condition = dependencyCondition state;
              }
            ) succeededDependencies;
          };
        in
        assert finalSignalChecked;
        nameValuePair (processName processId) {
          exec = command;
          env =
            mergeUnique "${context} environment"
              (mapAttrs (
                name: value: renderValue "${context} environment `${name}`" parameterSources value
              ) process.environment)
              (mapAttrs (_: binding: "$NIXSPACE_SESSION_DIR/${binding.path}") launchRecord.sessionEnvironment);
          cwd =
            if process.workingDirectory == null then null else joinPath sourceRoot process.workingDirectory;
          after = map (
            dependency:
            "devenv:processes:${processName dependency}@${dependencySuffix ordinaryDependencies.${dependency}}"
          ) (attrNames ordinaryDependencies);
          ready = readiness;
          restart = {
            on = restartPolicy process.restart.policy;
            max = if process.restart.maxAttempts == 0 then null else process.restart.maxAttempts;
            window = null;
          };
          process-compose = processCompose;
        };

      ownProcesses = mapAttrs' renderProcess launchRecord.processes;
      ownProcessPolicies = mapAttrs' (
        processId: process:
        nameValuePair (processName processId) {
          inherit (process)
            onExit
            onReadinessLoss
            required
            ;
        }
      ) launchRecord.processes;

      # Port ownership remains Nix data.  The runtime client may test these
      # exact defaults, but it never guesses a workspace port or host.
      ownDeclaredPorts = concatLists (
        mapAttrsToList (
          processId: process:
          concatLists (
            mapAttrsToList (
              endpointId: endpoint:
              if endpoint.portParameter == null then
                [ ]
              else
                let
                  portSource = localParameterSources.${endpoint.portParameter};
                  port =
                    if portSource.parameter.allocation == "automatic" then null else portSource.parameter.default;
                  host =
                    if endpoint.hostParameter == null then
                      "127.0.0.1"
                    else
                      localParameterSources.${endpoint.hostParameter}.parameter.default;
                in
                [
                  {
                    launch = workspaceCoordinate "launch" coordinate;
                    process = processName processId;
                    endpoint = endpointId;
                    inherit (endpoint) protocol;
                    transport = if endpoint.protocol == "udp" then "udp" else "tcp";
                    inherit host port;
                    hostEnvironment =
                      if endpoint.hostParameter == null then
                        null
                      else
                        localParameterSources.${endpoint.hostParameter}.environment;
                    portEnvironment = portSource.environment;
                  }
                ]
            ) process.endpoints
          )
        ) launchRecord.processes
      );

      included = mapAttrs (
        includeId: include:
        let
          includedLaunch = launchFor include.launch;
          includedForwarding = mapAttrs (
            includedParameter: parentParameter:
            if hasAttr parentParameter localParameterSources then
              localParameterSources.${parentParameter}
            else
              fail "launch `${coordinate}` include `${includeId}` forwards unknown parameter `${parentParameter}`"
          ) include.parameters;
          unknownForwarding = builtins.filter (name: !hasAttr name includedLaunch.parameters) (
            attrNames includedForwarding
          );
          checkedForwarding =
            require (unknownForwarding == [ ])
              "launch `${coordinate}` include `${includeId}` forwards unknown included parameters: ${concatStringsSep ", " unknownForwarding}";
        in
        assert checkedForwarding;
        expandLaunch {
          coordinate = include.launch;
          prefix = "${prefix}${includeId}--";
          forwardedParameters = includedForwarding;
          ancestry = ancestry ++ [ coordinate ];
        }
      ) launchRecord.includes;

      includedProcesses = builtins.foldl' (
        result: expansion: mergeUnique "launch `${coordinate}`" result expansion.processes
      ) { } (attrValues included);
      includedParameters = builtins.foldl' (result: expansion: result // expansion.parameters) { } (
        attrValues included
      );
      includedProcessPolicies = builtins.foldl' (
        result: expansion:
        mergeUnique "launch `${coordinate}` runtime policies" result expansion.processPolicies
      ) { } (attrValues included);
      includedSessionEnvironment = builtins.foldl' (
        result: expansion:
        mergeUnique "launch `${coordinate}` session environment" result expansion.sessionEnvironment
      ) { } (attrValues included);
      includedDeclaredPorts = concatLists (
        map (expansion: expansion.declaredPorts) (attrValues included)
      );
    in
    assert checkedCycle;
    {
      processes = mergeUnique "launch `${coordinate}`" ownProcesses includedProcesses;
      parameters = ownParameterContracts // includedParameters;
      processPolicies =
        mergeUnique "launch `${coordinate}` runtime policies" ownProcessPolicies
          includedProcessPolicies;
      sessionEnvironment =
        mergeUnique "launch `${coordinate}` session environment" launchRecord.sessionEnvironment
          includedSessionEnvironment;
      declaredPorts = ownDeclaredPorts ++ includedDeclaredPorts;
    };

  parsedLaunch = parseCoordinate "launch" launch;
  validInterface = require (
    index.interfaceVersion == 1
  ) "index interface version `${toString index.interfaceVersion}` is unsupported; expected 1";
  validPrefix = require (
    match "[A-Z_][A-Z0-9_]*" environmentPrefix != null
  ) "environment prefix `${environmentPrefix}` must be an uppercase environment identifier";
  invalidExecutableBindings = builtins.filter (
    coordinate: match "/nix/store/.+" executableBindings.${coordinate} == null
  ) (attrNames executableBindings);
  validExecutableBindings =
    require (invalidExecutableBindings == [ ])
      "executable binding overrides must be immutable /nix/store paths: ${concatStringsSep ", " invalidExecutableBindings}";
  rendered = expandLaunch { coordinate = launch; };
in
assert validInterface && validPrefix && validExecutableBindings;
{
  schemaVersion = 1;
  coordinate = launch;
  workspaceCoordinate = workspaceCoordinate "launch" launch;
  outputName = "launch-${parsedLaunch.packageId}--${parsedLaunch.id}";
  inherit (rendered) parameters processes;
  runtime = {
    inherit (rendered) declaredPorts processPolicies sessionEnvironment;
    responsibilities = [
      "parameter-defaults-and-validation"
      "secret-resolution"
      "port-allocation"
      "session-lifecycle"
    ];
  };
}
