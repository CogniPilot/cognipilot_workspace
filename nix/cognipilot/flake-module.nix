{ config, lib, ... }:

let
  inherit (lib)
    all
    attrNames
    concatLists
    concatStringsSep
    filter
    hasPrefix
    length
    mapAttrs
    mapAttrs'
    mapAttrsToList
    mkOption
    optional
    splitString
    types
    unique
    ;

  idPattern = "^[a-z][a-z0-9]*([._-][a-z0-9]+)*$";
  validId = value: builtins.match idPattern value != null;
  validRelativePath =
    value:
    value != ""
    && !hasPrefix "/" value
    && all (part: part != "" && part != "..") (splitString "/" value);
  validRuntimePath = value: value != "" && all (part: part != "..") (splitString "/" value);
  validEnvironmentName = value: builtins.match "^[A-Z_][A-Z0-9_]*$" value != null;
  validVariantValue = value: builtins.match "^[A-Za-z0-9][A-Za-z0-9._/+:-]*$" value != null;
  nonBlank = value: value != "" && builtins.match "[[:space:]]*" value == null;

  spdxOperators = [
    "AND"
    "OR"
    "WITH"
  ];
  validSpdxId =
    token:
    builtins.match "^[A-Za-z0-9][A-Za-z0-9.+-]*$" token != null && !(builtins.elem token spdxOperators);
  validSpdxExpression =
    expression:
    let
      tokens = filter (token: token != "") (
        splitString " " (
          builtins.replaceStrings
            [
              "("
              ")"
            ]
            [
              " ( "
              " ) "
            ]
            expression
        )
      );
      parsed =
        builtins.foldl'
          (
            state: token:
            if !state.valid then
              state
            else if state.expectOperand then
              if token == "(" then
                state // { depth = state.depth + 1; }
              else if validSpdxId token then
                state // { expectOperand = false; }
              else
                state // { valid = false; }
            else if token == ")" && state.depth > 0 then
              state // { depth = state.depth - 1; }
            else if builtins.elem token spdxOperators then
              state // { expectOperand = true; }
            else
              state // { valid = false; }
          )
          {
            valid = true;
            expectOperand = true;
            depth = 0;
          }
          tokens;
    in
    expression != ""
    && builtins.match "[-A-Za-z0-9.+() ]+" expression != null
    && parsed.valid
    && !parsed.expectOperand
    && parsed.depth == 0;

  presetActions = import ./action-presets.nix { inherit lib; };
  actionToolProfiles = import ./action-tool-profiles.nix { inherit lib; };

  actionArtifactArgvSegmentType = types.submodule (_: {
    options = {
      artifactInput = mkOption {
        type = types.str;
        description = "Artifact input ID whose producer path becomes this argv entry.";
      };
      prefix = mkOption {
        type = types.str;
        default = "";
        description = "Literal text prepended to the resolved artifact path in the same argv entry.";
      };
      suffix = mkOption {
        type = types.str;
        default = "";
        description = "Literal text appended to the resolved artifact path in the same argv entry.";
      };
    };
  });

  actionArgvSegmentType = types.oneOf [
    types.str
    actionArtifactArgvSegmentType
  ];

  customActionType = types.submodule (_: {
    options = {
      kind = mkOption {
        type = types.enum [
          "build"
          "generate"
          "test"
          "other"
        ];
        description = "Semantic action class.";
      };
      argv = mkOption {
        type = types.listOf actionArgvSegmentType;
        description = ''
          Executable and arguments without shell interpolation. An artifact
          segment resolves one declared artifact input to one argv entry;
          prefix and suffix remain literal data. Declaring argv makes this
          action a counted bespoke adapter.
        '';
      };
      environment = mkOption {
        type = types.attrsOf types.str;
        default = { };
        description = ''
          Exact static environment entries passed to the action process.
          Artifact-derived paths are declared on artifact inputs instead of
          encoded as string interpolation here.
        '';
      };
      toolProfile = mkOption {
        type = types.nullOr (types.enum (attrNames actionToolProfiles));
        default = null;
        description = ''
          Versioned Nix tool profile resolved to exact store-backed process
          inputs by the generated task layer.
        '';
      };
      dependsOn = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = "Action IDs in the same project that must finish first.";
      };
    };
  });

  actionRequirementType = types.submodule (_: {
    options = {
      cpu = mkOption {
        type = types.nullOr types.ints.positive;
        default = null;
        description = "Minimum logical CPU count requested by the action.";
      };
      memoryMiB = mkOption {
        type = types.nullOr types.ints.positive;
        default = null;
        description = "Minimum memory requested by the action in MiB.";
      };
      exclusiveLocks = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = "Named resources that cannot be shared while the action runs.";
      };
    };
  });

  resourceExportType = types.submodule (_: {
    options = {
      kind = mkOption {
        type = types.enum [
          "configuration"
          "data"
          "model"
          "schema"
          "plugin"
        ];
        description = "Semantic resource class.";
      };
      path = mkOption {
        type = types.str;
        description = "Relative source path exported by this package.";
      };
    };
  });

  executableExportType = types.submodule (_: {
    options = {
      from = mkOption {
        type = types.str;
        description = "Executable artifact coordinate in package:target:artifact form.";
      };
      argv = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = "Default arguments represented as argv entries, never shell text.";
      };
    };
  });

  softwareVersionType = types.submodule (_: {
    options = {
      source = mkOption {
        type = types.enum [
          "native"
          "literal"
          "file"
        ];
        default = "native";
        description = "Authoritative source of the package software version.";
      };
      value = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Literal software version when source is `literal`.";
      };
      file = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Relative version-file path when source is `file`.";
      };
    };
  });

  launchValueType = types.submodule (_: {
    options = {
      literal = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Literal argv or environment value.";
      };
      parameter = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Launch parameter substituted as one argv/environment value.";
      };
      prefix = mkOption {
        type = types.str;
        default = "";
        description = "Static prefix joined to a scalar parameter value.";
      };
      suffix = mkOption {
        type = types.str;
        default = "";
        description = "Static suffix joined to a scalar parameter value.";
      };
    };
  });

  launchParameterType = types.submodule (_: {
    options = {
      type = mkOption {
        type = types.enum [
          "string"
          "integer"
          "float"
          "boolean"
          "enum"
          "host"
          "url"
          "port"
          "duration"
          "path"
          "secret"
        ];
        description = "Runtime parameter type.";
      };
      description = mkOption {
        type = types.str;
        default = "";
        description = "Human-readable parameter purpose.";
      };
      required = mkOption {
        type = types.bool;
        default = false;
        description = "Require an explicit runtime value.";
      };
      default = mkOption {
        type = types.nullOr (
          types.oneOf [
            types.str
            types.int
            types.float
            types.bool
          ]
        );
        default = null;
        description = "JSON-scalar package default.";
      };
      enumValues = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = "Accepted values when type is `enum`.";
      };
      minimum = mkOption {
        type = types.nullOr types.number;
        default = null;
        description = "Optional inclusive numeric lower bound.";
      };
      maximum = mkOption {
        type = types.nullOr types.number;
        default = null;
        description = "Optional inclusive numeric upper bound.";
      };
      allowedRoots = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = "Allowed roots for path parameters.";
      };
      mustExist = mkOption {
        type = types.bool;
        default = false;
        description = "Require a path value to exist during preflight.";
      };
      access = mkOption {
        type = types.enum [
          "read"
          "write"
          "read-write"
        ];
        default = "read";
        description = "Required path access checked during preflight.";
      };
      allocation = mkOption {
        type = types.enum [
          "fixed"
          "automatic"
        ];
        default = "fixed";
        description = "Whether a port uses its declared value or is allocated per launch session.";
      };
      allocationHost = mkOption {
        type = types.str;
        default = "127.0.0.1";
        description = "Nix-selected bind host used while allocating an automatic port.";
      };
      allocationTransport = mkOption {
        type = types.enum [
          "tcp"
          "udp"
        ];
        default = "tcp";
        description = "Transport reserved while allocating an automatic port.";
      };
    };
  });

  launchEndpointType = types.submodule (_: {
    options = {
      protocol = mkOption {
        type = types.enum [
          "tcp"
          "udp"
          "http"
          "https"
          "zenoh"
          "other"
        ];
        description = "Published endpoint protocol.";
      };
      hostParameter = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Optional string/host parameter supplying the bound host.";
      };
      portParameter = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Optional port parameter supplying the bound port.";
      };
      path = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Absolute HTTP request path used by endpoint readiness.";
      };
      expectedStatus = mkOption {
        type = types.nullOr (types.ints.between 100 599);
        default = null;
        description = "Exact HTTP response status required by endpoint readiness.";
      };
    };
  });

  launchSessionEnvironmentType = types.submodule (_: {
    options = {
      base = mkOption {
        type = types.enum [ "session" ];
        default = "session";
        description = "Runtime path base selected by the launch client.";
      };
      path = mkOption {
        type = types.str;
        description = "Safe relative path below the launch session directory.";
      };
      create = mkOption {
        type = types.enum [
          "none"
          "parent"
          "directory"
        ];
        default = "none";
        description = "Filesystem preparation performed by the launch client.";
      };
    };
  });

  launchProcessType = types.submodule (_: {
    options = {
      executable = mkOption {
        type = types.str;
        description = "Named executable coordinate in package:executable form.";
      };
      argv = mkOption {
        type = types.listOf launchValueType;
        default = [ ];
        description = "Structured argv entries; shell command strings are not supported.";
      };
      environment = mkOption {
        type = types.attrsOf launchValueType;
        default = { };
        description = "Structured environment values keyed by variable name.";
      };
      workingDirectory = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Optional safe package-relative working directory.";
      };
      dependencies = mkOption {
        type = types.attrsOf (
          types.enum [
            "started"
            "ready"
            "succeeded"
            "completed"
          ]
        );
        default = { };
        description = "Required state for each process dependency.";
      };
      endpoints = mkOption {
        type = types.attrsOf (types.uniq launchEndpointType);
        default = { };
        description = "Endpoints published by this process.";
      };
      readiness = mkOption {
        type = types.submodule (_: {
          options = {
            kind = mkOption {
              type = types.enum [
                "none"
                "started"
                "endpoint"
              ];
              default = "none";
              description = "Readiness proof kind.";
            };
            endpoint = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "Local endpoint ID used by endpoint readiness.";
            };
            timeoutMs = mkOption {
              type = types.ints.positive;
              default = 30000;
              description = "Readiness deadline in milliseconds.";
            };
          };
        });
        default = { };
        description = "Declarative readiness policy.";
      };
      restart = mkOption {
        type = types.submodule (_: {
          options = {
            policy = mkOption {
              type = types.enum [
                "never"
                "on-failure"
                "always"
              ];
              default = "never";
              description = "Supervisor restart policy.";
            };
            maxAttempts = mkOption {
              type = types.ints.unsigned;
              default = 0;
              description = "Maximum restart attempts; zero means supervisor default.";
            };
            backoffMs = mkOption {
              type = types.ints.unsigned;
              default = 0;
              description = "Delay between restart attempts.";
            };
          };
        });
        default = { };
        description = "Declarative restart policy.";
      };
      shutdown = mkOption {
        type = types.submodule (_: {
          options = {
            signal = mkOption {
              type = types.enum [
                "SIGINT"
                "SIGTERM"
              ];
              default = "SIGTERM";
              description = "Initial shutdown signal.";
            };
            timeoutMs = mkOption {
              type = types.ints.positive;
              default = 10000;
              description = "Graceful shutdown timeout.";
            };
            killSignal = mkOption {
              type = types.enum [
                "SIGTERM"
                "SIGKILL"
              ];
              default = "SIGKILL";
              description = "Escalation signal after the timeout.";
            };
          };
        });
        default = { };
        description = "Declarative shutdown and escalation policy.";
      };
      required = mkOption {
        type = types.bool;
        default = true;
        description = "Whether losing this process invalidates the launch.";
      };
      onExit = mkOption {
        type = types.enum [
          "stop-launch"
          "restart"
          "ignore"
        ];
        default = "stop-launch";
        description = "Launch response when this process exits.";
      };
      onReadinessLoss = mkOption {
        type = types.enum [
          "stop-launch"
          "restart"
          "ignore"
        ];
        default = "stop-launch";
        description = "Launch response when readiness is lost.";
      };
    };
  });

  launchIncludeType = types.submodule (_: {
    options = {
      launch = mkOption {
        type = types.str;
        description = "Included launch coordinate in package:launch form.";
      };
      parameters = mkOption {
        type = types.attrsOf types.str;
        default = { };
        description = "Included parameter ID to parent parameter ID forwarding map.";
      };
    };
  });

  launchType = types.submodule (_: {
    options = {
      description = mkOption {
        type = types.str;
        description = "Human-readable launch purpose.";
      };
      parameters = mkOption {
        type = types.attrsOf (types.uniq launchParameterType);
        default = { };
        description = "Typed runtime parameters.";
      };
      requiredArtifacts = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = "Exact package:target:artifact requirements.";
      };
      requiredResources = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = "Exact package:resource requirements.";
      };
      sessionEnvironment = mkOption {
        type = types.attrsOf (types.uniq launchSessionEnvironmentType);
        default = { };
        description = ''
          Environment paths rooted in the Nix-selected launch session. The
          runtime client prepares only the declared directory or parent; no
          shell state-preparation command is generated.
        '';
      };
      processes = mkOption {
        type = types.attrsOf (types.uniq launchProcessType);
        default = { };
        description = "Executable-backed supervised process descriptions.";
      };
      includes = mkOption {
        type = types.attrsOf (types.uniq launchIncludeType);
        default = { };
        description = "Explicit package-launch includes and parameter forwarding.";
      };
      capabilities = mkOption {
        type = types.submodule (_: {
          options = {
            provides = mkOption {
              type = types.listOf types.str;
              default = [ ];
              description = "Capabilities exported by this launch.";
            };
            requires = mkOption {
              type = types.listOf types.str;
              default = [ ];
              description = "Capabilities that a containing bundle must provide.";
            };
          };
        });
        default = { };
        description = "Explicit launch capability contract.";
      };
    };
  });

  variantDimensionType = types.submodule (_: {
    options = {
      values = mkOption {
        type = types.listOf types.str;
        description = "Finite set of accepted variant values.";
      };
      default = mkOption {
        type = types.str;
        description = "Variant value used when the caller does not select one.";
      };
    };
  });

  artifactContractType = types.submodule (_: {
    options = {
      name = mkOption {
        type = types.str;
        description = "Stable semantic artifact interface name.";
      };
      version = mkOption {
        type = types.ints.positive;
        description = "Artifact interface major expected by producers and consumers.";
      };
    };
  });

  artifactOutputType = types.submodule (_: {
    options = {
      producedBy = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = ''
          Exact action that publishes this artifact. The declaration may be
          omitted only when the selected shared preset has an unambiguous
          conventional build action or a sole action.
        '';
      };
      kind = mkOption {
        type = types.enum [
          "file"
          "executable"
          "directory"
          "symlink"
        ];
        description = "Filesystem kind published by the producing action.";
      };
      path = mkOption {
        type = types.str;
        description = "Relative path owned by this artifact in the target output root.";
      };
      contract = mkOption {
        type = artifactContractType;
        description = "Typed interface implemented by this artifact.";
      };
    };
  });

  artifactInputType = types.submodule (_: {
    options = {
      consumedBy = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = ''
          Exact actions that consume this artifact. The declaration may be
          omitted only when the selected shared preset has an unambiguous
          conventional build action or a sole action.
        '';
      };
      from = mkOption {
        type = types.str;
        description = "Producer coordinate in package:target:artifact form.";
      };
      environment = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = ''
          Optional environment variable populated with the resolved producer
          artifact path for every consuming action. This is a typed process
          binding, not a command-template language.
        '';
      };
      contract = mkOption {
        type = artifactContractType;
        description = "Typed interface required from the producer.";
      };
    };
  });

  releaseOutputReferenceType = types.submodule (_: {
    options = {
      provider = mkOption {
        type = types.str;
        description = "Root flake input that provides the immutable Nix package output.";
      };
      package = mkOption {
        type = types.str;
        description = "Exact packages.<system> output name exported by the provider flake.";
      };
    };
  });

  targetType = types.submodule (_: {
    options = {
      release = mkOption {
        type = types.nullOr releaseOutputReferenceType;
        default = null;
        description = ''
          Conventional immutable package output for this target. Source and
          definition inputs remain separate authorities.
        '';
      };
      variants = mkOption {
        type = types.submodule (_: {
          options = {
            dimensions = mkOption {
              type = types.attrsOf (types.uniq variantDimensionType);
              default = { };
              description = "Independent finite variant dimensions for this target.";
            };
            allowedCombinations = mkOption {
              type = types.listOf (types.attrsOf types.str);
              default = [ ];
              description = ''
                Complete allowed assignments. An empty list permits the full
                Cartesian product of dimension values.
              '';
            };
          };
        });
        default = { };
        description = "Target variant dimensions and optional constraints.";
      };
      artifacts = mkOption {
        type = types.submodule (_: {
          options = {
            outputs = mkOption {
              type = types.attrsOf (types.uniq artifactOutputType);
              default = { };
              description = "Typed filesystem results uniquely owned by this target.";
            };
            inputs = mkOption {
              type = types.attrsOf (types.uniq artifactInputType);
              default = { };
              description = "Typed artifact references consumed by this target.";
            };
          };
        });
        default = { };
        description = "Artifact input and output contracts for this target.";
      };
      actionRequirements = mkOption {
        type = types.attrsOf (types.uniq actionRequirementType);
        default = { };
        description = "Optional resource requirements keyed by a generated action ID.";
      };
    };
  });

  projectType = types.submodule (
    { config, name, ... }:
    {
      options = {
        packageId = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = "Public package ID; defaults to the project integration key.";
        };
        aliases = mkOption {
          type = types.listOf types.str;
          default = [ ];
          description = "Additional globally unique public package lookup IDs.";
        };
        lifecycle = mkOption {
          type = types.enum [
            "experimental"
            "stable"
            "deprecated"
            "retired"
          ];
          default = "experimental";
          description = "Package support lifecycle.";
        };
        softwareVersion = mkOption {
          type = softwareVersionType;
          default = { };
          description = "Software-version provenance, independent of the interface major.";
        };
        deployability = mkOption {
          type = types.enum [
            "local-only"
            "qualification"
            "deployable"
          ];
          default = "local-only";
          description = "Whether this package can produce an immutable deployable output.";
        };
        owner = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = "Owning team or organization; workspace policy may require it.";
        };
        license = mkOption {
          type = types.submodule (_: {
            options.spdx = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "SPDX license expression; workspace policy may require it.";
            };
          });
          default = { };
          description = "Package license declaration.";
        };
        repositoryId = mkOption {
          type = types.str;
          default = name;
          description = "Stable repository acquisition ID.";
        };
        source = mkOption {
          type = types.submodule (_: {
            options = {
              input = mkOption {
                type = types.str;
                default = "${name}_source";
                description = "Root flake input containing the project source.";
              };
              root = mkOption {
                type = types.str;
                default = ".";
                description = "Relative project root within the source input.";
              };
              visibility = mkOption {
                type = types.enum [
                  "private"
                  "public"
                ];
                default = "private";
                description = ''
                  Source disclosure policy used by promotion and public-cache
                  roots. The fail-safe default prevents undeclared sources from
                  entering public closure publication.
                '';
              };
              dependencies = mkOption {
                type = types.listOf types.str;
                default = [ ];
                description = ''
                  Canonical package IDs whose editable source checkouts are
                  acquired with this package. These are source-workspace edges,
                  not artifact consumption or build-order dependencies.
                '';
              };
            };
          });
          description = "Pinned source coordinate, independent of its definition.";
        };
        definition = mkOption {
          type = types.submodule (_: {
            options = {
              origin = mkOption {
                type = types.enum [
                  "in-tree"
                  "external"
                  "fork"
                ];
                default = "in-tree";
                description = "The one complete authority for this definition.";
              };
              input = mkOption {
                type = types.nullOr types.str;
                default = if config.definition.origin == "external" then "${name}_definition" else null;
                description = ''
                  Root flake input exporting the definition. In-tree and fork
                  definitions use the source input; an external definition
                  defaults to the conventional PROJECT_definition root input.
                '';
              };
            };
          });
          default = { };
          description = "Pinned integration-definition coordinate.";
        };
        preset = mkOption {
          type = types.enum (attrNames presetActions);
          description = "Versioned shared native-project adapter.";
        };
        customActions = mkOption {
          type = types.attrsOf customActionType;
          default = { };
          description = ''
            Genuine project-specific action exceptions. Every entry is reported
            as a bespoke adapter by the normalized compliance index.
          '';
        };
        targets = mkOption {
          type = types.attrsOf (types.uniq targetType);
          default = { };
          description = ''
            Semantic target deltas. Every preset supplies a `default` target, so
            conventional single-target packages do not repeat it.
          '';
        };
        resources = mkOption {
          type = types.attrsOf (types.uniq resourceExportType);
          default = { };
          description = "Named package-owned configuration, data, model, and plugin roots.";
        };
        executables = mkOption {
          type = types.attrsOf (types.uniq executableExportType);
          default = { };
          description = "Named executable artifact exports with argv-safe defaults.";
        };
        launches = mkOption {
          type = types.attrsOf (types.uniq launchType);
          default = { };
          description = "Package-owned declarative launch IR v1 definitions.";
        };
      };
    }
  );

  projects = config.cognipilot.projects;

  packageIdFor =
    projectId: project: if project.packageId == null then projectId else project.packageId;

  complianceWarningsFor =
    project:
    optional (project.owner == null) "missing-owner"
    ++ optional (project.license.spdx == null) "missing-license";

  identityClaims = concatLists (
    mapAttrsToList (
      projectId: project:
      let
        packageId = packageIdFor projectId project;
      in
      [
        {
          id = packageId;
          owner = projectId;
          kind = "package";
        }
      ]
      ++ map (alias: {
        id = alias;
        owner = projectId;
        kind = "alias";
      }) project.aliases
    ) projects
  );

  collidingIdentityIds = unique (
    map (claim: claim.id) (
      filter (claim: length (filter (other: other.id == claim.id) identityClaims) > 1) identityClaims
    )
  );

  packageIds = mapAttrsToList (projectId: project: packageIdFor projectId project) projects;
  packageVisibilityIndex = builtins.listToAttrs (
    mapAttrsToList (projectId: project: {
      name = packageIdFor projectId project;
      value = project.source.visibility;
    }) projects
  );

  emptyTarget = {
    release = null;
    variants = {
      dimensions = { };
      allowedCombinations = [ ];
    };
    artifacts = {
      outputs = { };
      inputs = { };
    };
    actionRequirements = { };
  };

  effectiveTargets = project: { default = emptyTarget; } // project.targets;

  presetActionsFor =
    project:
    let
      provider = presetActions.${project.preset};
    in
    if builtins.isFunction provider then provider project else provider;

  definitionInput =
    project:
    if project.definition.input == null then project.source.input else project.definition.input;

  normalizedCustomActions =
    project:
    mapAttrs (_: action: {
      inherit (action)
        dependsOn
        environment
        kind
        toolProfile
        argv
        ;
      adapter = "bespoke-v1";
    }) project.customActions;

  normalizedBaseActions = project: presetActionsFor project // normalizedCustomActions project;

  conventionalArtifactAction =
    project:
    let
      actions = presetActionsFor project;
      actionNames = attrNames actions;
    in
    if project.customActions != { } then
      null
    else if builtins.hasAttr "build" actions then
      "build"
    else if length actionNames == 1 then
      builtins.head actionNames
    else
      null;

  artifactProducer =
    project: artifact:
    if artifact.producedBy != null then artifact.producedBy else conventionalArtifactAction project;

  artifactConsumers =
    project: artifact:
    if artifact.consumedBy != [ ] then
      artifact.consumedBy
    else
      optional (conventionalArtifactAction project != null) (conventionalArtifactAction project);

  normalizedArtifacts = project: target: {
    outputs = mapAttrs (
      _: artifact:
      artifact
      // {
        producedBy = artifactProducer project artifact;
      }
    ) target.artifacts.outputs;
    inputs = mapAttrs (
      _: artifact:
      artifact
      // {
        consumedBy = artifactConsumers project artifact;
      }
    ) target.artifacts.inputs;
  };

  emptyActionRequirements = {
    cpu = null;
    memoryMiB = null;
    exclusiveLocks = [ ];
  };

  normalizedActions =
    project: target:
    mapAttrs (
      actionId: action:
      action
      // {
        requirements =
          if builtins.hasAttr actionId target.actionRequirements then
            target.actionRequirements.${actionId}
          else
            emptyActionRequirements;
      }
    ) (normalizedBaseActions project);

  actionDependencies = project: actionId: (normalizedBaseActions project).${actionId}.dependsOn;

  dependencyClosure =
    project: actionNames: actionId:
    builtins.foldl' (
      reached: _:
      unique (
        reached
        ++ concatLists (
          map (
            dependency:
            if builtins.elem dependency actionNames then actionDependencies project dependency else [ ]
          ) reached
        )
      )
    ) (actionDependencies project actionId) (builtins.genList (_: null) (length actionNames));

  parseArtifactCoordinate =
    value:
    let
      parts = splitString ":" value;
    in
    if length parts == 3 && all validId parts then
      {
        packageId = builtins.elemAt parts 0;
        targetId = builtins.elemAt parts 1;
        artifactId = builtins.elemAt parts 2;
        target = "${builtins.elemAt parts 0}:${builtins.elemAt parts 1}";
      }
    else
      null;

  parsePackageCoordinate =
    value:
    let
      parts = splitString ":" value;
    in
    if length parts == 2 && all validId parts then
      {
        packageId = builtins.elemAt parts 0;
        name = builtins.elemAt parts 1;
      }
    else
      null;

  pathsOverlap =
    left: right: left == right || hasPrefix "${left}/" right || hasPrefix "${right}/" left;

  targetRecords = concatLists (
    mapAttrsToList (
      projectId: project:
      let
        packageId = packageIdFor projectId project;
      in
      mapAttrsToList (targetId: target: {
        inherit
          packageId
          project
          projectId
          targetId
          target
          ;
        coordinate = "${packageId}:${targetId}";
      }) (effectiveTargets project)
    ) projects
  );

  artifactOutputRecords = concatLists (
    map (
      targetRecord:
      mapAttrsToList (artifactId: artifact: {
        artifact = artifact // {
          producedBy = artifactProducer targetRecord.project artifact;
        };
        producingAction = artifactProducer targetRecord.project artifact;
        inherit artifactId;
        inherit (targetRecord) packageId targetId;
        coordinate = "${targetRecord.coordinate}:${artifactId}";
        owner = targetRecord.coordinate;
      }) targetRecord.target.artifacts.outputs
    ) targetRecords
  );

  artifactInputRecords = concatLists (
    map (
      targetRecord:
      mapAttrsToList (inputId: input: {
        input = input // {
          consumedBy = artifactConsumers targetRecord.project input;
        };
        consumingActions = artifactConsumers targetRecord.project input;
        inherit inputId;
        inherit (targetRecord) packageId targetId;
        consumer = targetRecord.coordinate;
        producer = parseArtifactCoordinate input.from;
      }) targetRecord.target.artifacts.inputs
    ) targetRecords
  );

  artifactOutputIndex = builtins.listToAttrs (
    map (record: {
      name = record.coordinate;
      value = record;
    }) artifactOutputRecords
  );

  actionRecords = concatLists (
    map (
      targetRecord:
      mapAttrsToList (actionId: action: {
        inherit action actionId;
        inherit (targetRecord) packageId project targetId;
        coordinate = "${targetRecord.coordinate}:${actionId}";
        target = targetRecord.coordinate;
      }) (normalizedBaseActions targetRecord.project)
    ) targetRecords
  );

  actionCoordinates = map (record: record.coordinate) actionRecords;

  actionIndex = builtins.listToAttrs (
    map (record: {
      name = record.coordinate;
      value = record;
    }) actionRecords
  );

  resourceRecords = concatLists (
    mapAttrsToList (
      projectId: project:
      let
        packageId = packageIdFor projectId project;
      in
      mapAttrsToList (resourceId: resource: {
        inherit
          packageId
          projectId
          resourceId
          resource
          ;
        coordinate = "${packageId}:${resourceId}";
      }) project.resources
    ) projects
  );

  executableRecords = concatLists (
    mapAttrsToList (
      projectId: project:
      let
        packageId = packageIdFor projectId project;
      in
      mapAttrsToList (executableId: executable: {
        inherit
          executable
          executableId
          packageId
          projectId
          ;
        coordinate = "${packageId}:${executableId}";
      }) project.executables
    ) projects
  );

  launchRecords = concatLists (
    mapAttrsToList (
      projectId: project:
      let
        packageId = packageIdFor projectId project;
      in
      mapAttrsToList (launchId: launch: {
        inherit
          launch
          launchId
          packageId
          project
          projectId
          ;
        coordinate = "${packageId}:${launchId}";
      }) project.launches
    ) projects
  );

  resourceIndex = builtins.listToAttrs (
    map (record: {
      name = record.coordinate;
      value = record;
    }) resourceRecords
  );

  executableIndex = builtins.listToAttrs (
    map (record: {
      name = record.coordinate;
      value = record;
    }) executableRecords
  );

  launchIndex = builtins.listToAttrs (
    map (record: {
      name = record.coordinate;
      value = record;
    }) launchRecords
  );

  errorsForTarget =
    packageId: project: targetId: target:
    let
      dimensionNames = builtins.sort builtins.lessThan (attrNames target.variants.dimensions);
      combinationKeys = combination: builtins.sort builtins.lessThan (attrNames combination);
      invalidCombinationShapes = filter (
        combination: combinationKeys combination != dimensionNames
      ) target.variants.allowedCombinations;
      invalidCombinationValues = concatLists (
        map (
          combination:
          concatLists (
            mapAttrsToList (
              dimensionId: value:
              optional (
                builtins.hasAttr dimensionId target.variants.dimensions
                && !(builtins.elem value target.variants.dimensions.${dimensionId}.values)
              ) "${dimensionId}=${value}"
            ) combination
          )
        ) target.variants.allowedCombinations
      );
      outputRecords = mapAttrsToList (artifactId: artifact: {
        inherit artifactId artifact;
      }) target.artifacts.outputs;
      overlappingOutputs = filter (
        left:
        builtins.any (
          right:
          left.artifactId != right.artifactId
          && validRelativePath left.artifact.path
          && validRelativePath right.artifact.path
          && pathsOverlap left.artifact.path right.artifact.path
        ) outputRecords
      ) outputRecords;
      actionNames = attrNames (normalizedBaseActions project);
      artifactEnvironmentClaims = concatLists (
        mapAttrsToList (
          inputId: input:
          if input.environment == null then
            [ ]
          else
            map (actionId: {
              inherit actionId inputId;
              name = input.environment;
            }) (artifactConsumers project input)
        ) target.artifacts.inputs
      );
      duplicateArtifactEnvironmentClaims = filter (
        claim:
        length (
          filter (
            other: other.actionId == claim.actionId && other.name == claim.name
          ) artifactEnvironmentClaims
        ) > 1
      ) artifactEnvironmentClaims;
      artifactArgvReferences = concatLists (
        mapAttrsToList (
          actionId: action:
          map (segment: {
            inherit actionId;
            inputId = segment.artifactInput;
          }) (filter builtins.isAttrs action.argv)
        ) (normalizedBaseActions project)
      );
    in
    optional (!validId targetId) "package `${packageId}` target ID `${targetId}` is invalid"
    ++
      optional (target.release != null && !validId target.release.provider)
        "package `${packageId}` target `${targetId}` release provider `${target.release.provider}` is invalid"
    ++
      optional (target.release != null && !validId target.release.package)
        "package `${packageId}` target `${targetId}` release package `${target.release.package}` is invalid"
    ++ concatLists (
      mapAttrsToList (
        dimensionId: dimension:
        optional (
          !validId dimensionId
        ) "package `${packageId}` target `${targetId}` variant dimension ID `${dimensionId}` is invalid"
        ++ optional (
          dimension.values == [ ]
        ) "package `${packageId}` target `${targetId}` variant `${dimensionId}` has no values"
        ++ optional (
          length (unique dimension.values) != length dimension.values
        ) "package `${packageId}` target `${targetId}` variant `${dimensionId}` has duplicate values"
        ++ map (
          value:
          "package `${packageId}` target `${targetId}` variant `${dimensionId}` value `${value}` is unsafe"
        ) (filter (value: !validVariantValue value) dimension.values)
        ++
          optional (!(builtins.elem dimension.default dimension.values))
            "package `${packageId}` target `${targetId}` variant `${dimensionId}` default `${dimension.default}` is not an allowed value"
      ) target.variants.dimensions
    )
    ++
      optional (invalidCombinationShapes != [ ])
        "package `${packageId}` target `${targetId}` allowed variant combinations must assign exactly: ${concatStringsSep ", " dimensionNames}"
    ++
      optional (invalidCombinationValues != [ ])
        "package `${packageId}` target `${targetId}` has invalid variant constraints: ${concatStringsSep ", " invalidCombinationValues}"
    ++ optional (
      length (unique (map builtins.toJSON target.variants.allowedCombinations))
      != length target.variants.allowedCombinations
    ) "package `${packageId}` target `${targetId}` has duplicate allowed variant combinations"
    ++ concatLists (
      mapAttrsToList (
        artifactId: artifact:
        let
          producer = artifactProducer project artifact;
        in
        optional (
          !validId artifactId
        ) "package `${packageId}` target `${targetId}` artifact output ID `${artifactId}` is invalid"
        ++
          optional (producer == null)
            "package `${packageId}` target `${targetId}` artifact `${artifactId}` must declare exact `producedBy`; custom or ambiguous action sets have no artifact-action default"
        ++
          optional (producer != null && !validId producer)
            "package `${packageId}` target `${targetId}` artifact `${artifactId}` producing action ID `${producer}` is invalid"
        ++
          optional (producer != null && validId producer && !(builtins.elem producer actionNames))
            "package `${packageId}` target `${targetId}` artifact `${artifactId}` references unknown producing action `${producer}`"
        ++
          optional (!validRelativePath artifact.path)
            "package `${packageId}` target `${targetId}` artifact `${artifactId}` path `${artifact.path}` is not a safe relative path"
        ++
          optional (!validId artifact.contract.name)
            "package `${packageId}` target `${targetId}` artifact `${artifactId}` contract name `${artifact.contract.name}` is invalid"
      ) target.artifacts.outputs
    )
    ++
      optional (overlappingOutputs != [ ])
        "package `${packageId}` target `${targetId}` has overlapping artifact output ownership: ${
          concatStringsSep ", " (map (record: record.artifactId) overlappingOutputs)
        }"
    ++ concatLists (
      mapAttrsToList (
        inputId: input:
        let
          consumers = artifactConsumers project input;
        in
        optional (
          !validId inputId
        ) "package `${packageId}` target `${targetId}` artifact input ID `${inputId}` is invalid"
        ++
          optional (consumers == [ ])
            "package `${packageId}` target `${targetId}` artifact input `${inputId}` must declare exact `consumedBy`; custom or ambiguous action sets have no artifact-action default"
        ++
          optional (length (unique consumers) != length consumers)
            "package `${packageId}` target `${targetId}` artifact input `${inputId}` declares duplicate consuming actions"
        ++ map (
          actionId:
          "package `${packageId}` target `${targetId}` artifact input `${inputId}` consuming action ID `${actionId}` is invalid"
        ) (filter (actionId: !validId actionId) consumers)
        ++ map (
          actionId:
          "package `${packageId}` target `${targetId}` artifact input `${inputId}` references unknown consuming action `${actionId}`"
        ) (filter (actionId: validId actionId && !(builtins.elem actionId actionNames)) consumers)
        ++
          optional (parseArtifactCoordinate input.from == null)
            "package `${packageId}` target `${targetId}` artifact input `${inputId}` reference `${input.from}` must use package:target:artifact IDs"
        ++
          optional (input.environment != null && !validEnvironmentName input.environment)
            "package `${packageId}` target `${targetId}` artifact input `${inputId}` environment `${toString input.environment}` is invalid"
        ++ concatLists (
          map (
            actionId:
            optional
              (
                input.environment != null
                && builtins.hasAttr actionId (normalizedBaseActions project)
                && builtins.hasAttr input.environment (normalizedBaseActions project).${actionId}.environment
              )
              "package `${packageId}` target `${targetId}` artifact input `${inputId}` environment `${toString input.environment}` collides with action `${actionId}` static environment"
          ) consumers
        )
        ++
          optional (!validId input.contract.name)
            "package `${packageId}` target `${targetId}` artifact input `${inputId}` contract name `${input.contract.name}` is invalid"
      ) target.artifacts.inputs
    )
    ++
      optional (duplicateArtifactEnvironmentClaims != [ ])
        "package `${packageId}` target `${targetId}` repeats artifact environment bindings: ${
          concatStringsSep ", " (
            unique (map (claim: "${claim.actionId}:${claim.name}") duplicateArtifactEnvironmentClaims)
          )
        }"
    ++ concatLists (
      map (
        reference:
        optional (!validId reference.inputId)
          "package `${packageId}` target `${targetId}` action `${reference.actionId}` argv artifact input ID `${reference.inputId}` is invalid"
        ++
          optional
            (validId reference.inputId && !(builtins.hasAttr reference.inputId target.artifacts.inputs))
            "package `${packageId}` target `${targetId}` action `${reference.actionId}` argv references unknown artifact input `${reference.inputId}`"
        ++
          optional
            (
              builtins.hasAttr reference.inputId target.artifacts.inputs
              && !(builtins.elem reference.actionId (
                artifactConsumers project target.artifacts.inputs.${reference.inputId}
              ))
            )
            "package `${packageId}` target `${targetId}` action `${reference.actionId}` argv artifact input `${reference.inputId}` must list the action in `consumedBy`"
      ) artifactArgvReferences
    )
    ++ concatLists (
      mapAttrsToList (
        actionId: requirements:
        optional (
          !validId actionId
        ) "package `${packageId}` target `${targetId}` action requirement ID `${actionId}` is invalid"
        ++
          optional (!(builtins.elem actionId actionNames))
            "package `${packageId}` target `${targetId}` action requirements reference unknown action `${actionId}`"
        ++ map (
          lockId:
          "package `${packageId}` target `${targetId}` action `${actionId}` exclusive lock ID `${lockId}` is invalid"
        ) (filter (lockId: !validId lockId) requirements.exclusiveLocks)
        ++ optional (
          length (unique requirements.exclusiveLocks) != length requirements.exclusiveLocks
        ) "package `${packageId}` target `${targetId}` action `${actionId}` has duplicate exclusive locks"
      ) target.actionRequirements
    );

  validParameterDefault =
    parameter:
    let
      value = parameter.default;
      valueType = builtins.typeOf value;
    in
    value == null
    || (parameter.type == "string" && valueType == "string")
    || (parameter.type == "integer" && valueType == "int")
    || (
      parameter.type == "float"
      && builtins.elem valueType [
        "int"
        "float"
      ]
    )
    || (parameter.type == "boolean" && valueType == "bool")
    || (parameter.type == "enum" && valueType == "string" && builtins.elem value parameter.enumValues)
    || (
      builtins.elem parameter.type [
        "host"
        "url"
      ]
      && valueType == "string"
      && nonBlank value
    )
    || (parameter.type == "port" && valueType == "int" && value >= 1 && value <= 65535)
    || (parameter.type == "duration" && valueType == "int" && value >= 0)
    || (parameter.type == "path" && valueType == "string" && validRuntimePath value);

  errorsForLaunchValue =
    context: parameters: value:
    optional (
      value.literal == null && value.parameter == null
    ) "${context} must declare exactly one of literal or parameter"
    ++ optional (
      value.literal != null && value.parameter != null
    ) "${context} must not combine literal and parameter"
    ++ optional (value.literal != null && value.literal == "") "${context} literal must not be empty"
    ++ optional (
      value.literal != null && (value.prefix != "" || value.suffix != "")
    ) "${context} literal cannot declare a parameter prefix or suffix"
    ++ optional (
      value.parameter != null && !validId value.parameter
    ) "${context} parameter ID `${toString value.parameter}` is invalid"
    ++ optional (
      value.parameter != null && !(builtins.hasAttr value.parameter parameters)
    ) "${context} references unknown parameter `${toString value.parameter}`";

  errorsForLaunch =
    record:
    let
      inherit (record)
        launch
        launchId
        packageId
        ;
      processNames = attrNames launch.processes;
      processDependencies =
        processId:
        if builtins.hasAttr processId launch.processes then
          attrNames launch.processes.${processId}.dependencies
        else
          [ ];
      processDependencyClosure =
        processId:
        builtins.foldl' (
          reached: _:
          unique (
            reached
            ++ concatLists (
              map (
                dependency: if builtins.elem dependency processNames then processDependencies dependency else [ ]
              ) reached
            )
          )
        ) (processDependencies processId) (builtins.genList (_: null) (length processNames));
      cyclicProcesses = filter (
        processId: builtins.elem processId (processDependencyClosure processId)
      ) processNames;
      includeProcessCollisions = filter (includeId: builtins.hasAttr includeId launch.processes) (
        attrNames launch.includes
      );
      capabilityIds = launch.capabilities.provides ++ launch.capabilities.requires;
    in
    optional (!validId launchId) "package `${packageId}` launch ID `${launchId}` is invalid"
    ++ optional (
      !nonBlank launch.description
    ) "package `${packageId}` launch `${launchId}` description must not be blank"
    ++ concatLists (
      mapAttrsToList (
        parameterId: parameter:
        let
          numericType = builtins.elem parameter.type [
            "integer"
            "float"
            "port"
            "duration"
          ];
          numericDefault = builtins.elem (builtins.typeOf parameter.default) [
            "int"
            "float"
          ];
        in
        optional (
          !validId parameterId
        ) "launch `${record.coordinate}` parameter ID `${parameterId}` is invalid"
        ++
          optional (!validParameterDefault parameter)
            "launch `${record.coordinate}` parameter `${parameterId}` has an invalid default for type `${parameter.type}`"
        ++ optional (
          parameter.required && parameter.default != null
        ) "launch `${record.coordinate}` parameter `${parameterId}` cannot be required and have a default"
        ++ optional (
          parameter.type == "secret" && parameter.default != null
        ) "launch `${record.coordinate}` secret parameter `${parameterId}` cannot have a default"
        ++ optional (
          parameter.type == "enum"
          && (
            parameter.enumValues == [ ]
            || length (unique parameter.enumValues) != length parameter.enumValues
            || builtins.any (value: !validId value) parameter.enumValues
          )
        ) "launch `${record.coordinate}` enum parameter `${parameterId}` must have unique ID values"
        ++ optional (
          parameter.type != "enum" && parameter.enumValues != [ ]
        ) "launch `${record.coordinate}` non-enum parameter `${parameterId}` cannot declare enum values"
        ++ optional (
          !numericType && (parameter.minimum != null || parameter.maximum != null)
        ) "launch `${record.coordinate}` non-numeric parameter `${parameterId}` cannot declare bounds"
        ++ optional (
          parameter.minimum != null && parameter.maximum != null && parameter.minimum > parameter.maximum
        ) "launch `${record.coordinate}` parameter `${parameterId}` has inverted bounds"
        ++ optional (
          parameter.default != null
          && parameter.minimum != null
          && numericType
          && numericDefault
          && parameter.default < parameter.minimum
        ) "launch `${record.coordinate}` parameter `${parameterId}` default is below its minimum"
        ++ optional (
          parameter.default != null
          && parameter.maximum != null
          && numericType
          && numericDefault
          && parameter.default > parameter.maximum
        ) "launch `${record.coordinate}` parameter `${parameterId}` default is above its maximum"
        ++ optional (
          parameter.type == "path"
          && (
            parameter.allowedRoots == [ ] || builtins.any (root: !validRuntimePath root) parameter.allowedRoots
          )
        ) "launch `${record.coordinate}` path parameter `${parameterId}` requires safe allowed roots"
        ++ optional (
          parameter.type != "path"
          && (parameter.allowedRoots != [ ] || parameter.mustExist || parameter.access != "read")
        ) "launch `${record.coordinate}` non-path parameter `${parameterId}` cannot declare path policy"
        ++ optional (
          parameter.allocation == "automatic" && parameter.type != "port"
        ) "launch `${record.coordinate}` automatic parameter `${parameterId}` must have type `port`"
        ++
          optional
            (
              parameter.type != "port"
              && (
                parameter.allocation != "fixed"
                || parameter.allocationHost != "127.0.0.1"
                || parameter.allocationTransport != "tcp"
              )
            )
            "launch `${record.coordinate}` non-port parameter `${parameterId}` cannot declare port allocation policy"
        ++
          optional (parameter.allocation == "automatic" && parameter.allocationHost == "")
            "launch `${record.coordinate}` automatic port `${parameterId}` requires a nonempty allocation host"
      ) launch.parameters
    )
    ++ optional (
      length (unique launch.requiredArtifacts) != length launch.requiredArtifacts
    ) "launch `${record.coordinate}` has duplicate required artifacts"
    ++ map (artifact: "launch `${record.coordinate}` has unresolved required artifact `${artifact}`") (
      filter (
        artifact:
        parseArtifactCoordinate artifact == null || !(builtins.hasAttr artifact artifactOutputIndex)
      ) launch.requiredArtifacts
    )
    ++ optional (
      length (unique launch.requiredResources) != length launch.requiredResources
    ) "launch `${record.coordinate}` has duplicate required resources"
    ++ map (resource: "launch `${record.coordinate}` has unresolved required resource `${resource}`") (
      filter (
        resource: parsePackageCoordinate resource == null || !(builtins.hasAttr resource resourceIndex)
      ) launch.requiredResources
    )
    ++ concatLists (
      mapAttrsToList (
        name: binding:
        optional (
          !validEnvironmentName name
        ) "launch `${record.coordinate}` session environment name `${name}` is invalid"
        ++ optional (
          !validRelativePath binding.path
        ) "launch `${record.coordinate}` session environment `${name}` path `${binding.path}` is unsafe"
        ++
          optional
            (builtins.any (process: builtins.hasAttr name process.environment) (
              builtins.attrValues launch.processes
            ))
            "launch `${record.coordinate}` session environment `${name}` collides with a process environment binding"
      ) launch.sessionEnvironment
    )
    ++ concatLists (
      mapAttrsToList (
        processId: process:
        optional (!validId processId) "launch `${record.coordinate}` process ID `${processId}` is invalid"
        ++
          optional
            (
              parsePackageCoordinate process.executable == null
              || !(builtins.hasAttr process.executable executableIndex)
            )
            "launch `${record.coordinate}` process `${processId}` has unresolved executable `${process.executable}`"
        ++
          optional (process.workingDirectory != null && !validRelativePath process.workingDirectory)
            "launch `${record.coordinate}` process `${processId}` has unsafe working directory `${toString process.workingDirectory}`"
        ++ concatLists (
          builtins.genList (
            index:
            let
              value = builtins.elemAt process.argv index;
              context = "launch `${record.coordinate}` process `${processId}` argv[${toString index}]";
            in
            errorsForLaunchValue context launch.parameters value
            ++
              optional
                (
                  value.parameter != null
                  && builtins.hasAttr value.parameter launch.parameters
                  && launch.parameters.${value.parameter}.type == "secret"
                )
                "${context} cannot reference secret parameter `${toString value.parameter}`; secrets are environment-only"
          ) (length process.argv)
        )
        ++ concatLists (
          mapAttrsToList (
            name: value:
            optional (
              !validEnvironmentName name
            ) "launch `${record.coordinate}` process `${processId}` environment name `${name}` is invalid"
            ++
              errorsForLaunchValue "launch `${record.coordinate}` process `${processId}` environment `${name}`"
                launch.parameters
                value
          ) process.environment
        )
        ++ map (
          dependency:
          "launch `${record.coordinate}` process `${processId}` has unknown dependency `${dependency}`"
        ) (filter (dependency: !(builtins.elem dependency processNames)) (attrNames process.dependencies))
        ++ optional (builtins.hasAttr processId process.dependencies) "launch `${record.coordinate}` process `${processId}` cannot depend on itself"
        ++ concatLists (
          mapAttrsToList (
            endpointId: endpoint:
            optional (
              !validId endpointId
            ) "launch `${record.coordinate}` process `${processId}` endpoint ID `${endpointId}` is invalid"
            ++
              optional
                (
                  endpoint.hostParameter != null
                  && (
                    !(builtins.hasAttr endpoint.hostParameter launch.parameters)
                    || !(builtins.elem launch.parameters.${endpoint.hostParameter}.type [
                      "string"
                      "host"
                    ])
                  )
                )
                "launch `${record.coordinate}` process `${processId}` endpoint `${endpointId}` has invalid host parameter"
            ++
              optional
                (
                  endpoint.portParameter != null
                  && (
                    !(builtins.hasAttr endpoint.portParameter launch.parameters)
                    || launch.parameters.${endpoint.portParameter}.type != "port"
                  )
                )
                "launch `${record.coordinate}` process `${processId}` endpoint `${endpointId}` has invalid port parameter"
            ++
              optional
                (
                  endpoint.portParameter != null
                  && builtins.hasAttr endpoint.portParameter launch.parameters
                  && launch.parameters.${endpoint.portParameter}.allocation == "automatic"
                  &&
                    launch.parameters.${endpoint.portParameter}.allocationTransport
                    != (if endpoint.protocol == "udp" then "udp" else "tcp")
                )
                "launch `${record.coordinate}` process `${processId}` endpoint `${endpointId}` automatic port transport does not match its protocol"
            ++
              optional
                (
                  endpoint.hostParameter != null
                  && endpoint.portParameter != null
                  && builtins.hasAttr endpoint.portParameter launch.parameters
                  && launch.parameters.${endpoint.portParameter}.allocation == "automatic"
                )
                "launch `${record.coordinate}` process `${processId}` endpoint `${endpointId}` automatic port requires its fixed allocation host"
            ++
              optional
                (
                  endpoint.path != null
                  && (
                    endpoint.protocol != "http"
                    || !(lib.hasPrefix "/" endpoint.path)
                    || builtins.match "[^[:cntrl:]]+" endpoint.path == null
                  )
                )
                "launch `${record.coordinate}` process `${processId}` endpoint `${endpointId}` has invalid HTTP path"
            ++
              optional (endpoint.expectedStatus != null && endpoint.protocol != "http")
                "launch `${record.coordinate}` process `${processId}` endpoint `${endpointId}` HTTP status requires protocol `http`"
          ) process.endpoints
        )
        ++ optional (
          process.readiness.kind == "endpoint"
          && (
            process.readiness.endpoint == null
            || !(builtins.hasAttr process.readiness.endpoint process.endpoints)
          )
        ) "launch `${record.coordinate}` process `${processId}` endpoint readiness has an invalid endpoint"
        ++
          optional (process.readiness.kind != "endpoint" && process.readiness.endpoint != null)
            "launch `${record.coordinate}` process `${processId}` non-endpoint readiness cannot name an endpoint"
        ++
          optional
            (
              (process.onExit == "restart" || process.onReadinessLoss == "restart")
              && process.restart.policy == "never"
            )
            "launch `${record.coordinate}` process `${processId}` requests restart behavior with restart policy `never`"
      ) launch.processes
    )
    ++
      optional (cyclicProcesses != [ ])
        "launch `${record.coordinate}` process dependency cycle involves: ${concatStringsSep ", " cyclicProcesses}"
    ++
      optional (includeProcessCollisions != [ ])
        "launch `${record.coordinate}` process and include IDs collide: ${concatStringsSep ", " includeProcessCollisions}"
    ++ concatLists (
      mapAttrsToList (
        includeId: include:
        let
          included =
            if builtins.hasAttr include.launch launchIndex then launchIndex.${include.launch}.launch else null;
          missingRequired =
            if included == null then
              [ ]
            else
              filter (
                parameterId:
                included.parameters.${parameterId}.required && !(builtins.hasAttr parameterId include.parameters)
              ) (attrNames included.parameters);
        in
        optional (!validId includeId) "launch `${record.coordinate}` include ID `${includeId}` is invalid"
        ++ optional (
          parsePackageCoordinate include.launch == null || !(builtins.hasAttr include.launch launchIndex)
        ) "launch `${record.coordinate}` include `${includeId}` has unresolved launch `${include.launch}`"
        ++ optional (
          include.launch == record.coordinate
        ) "launch `${record.coordinate}` cannot include itself"
        ++ concatLists (
          mapAttrsToList (
            includedParameter: parentParameter:
            optional (included != null && !(builtins.hasAttr includedParameter included.parameters))
              "launch `${record.coordinate}` include `${includeId}` forwards unknown included parameter `${includedParameter}`"
            ++
              optional (!(builtins.hasAttr parentParameter launch.parameters))
                "launch `${record.coordinate}` include `${includeId}` references unknown parent parameter `${parentParameter}`"
            ++ optional (
              included != null
              && builtins.hasAttr includedParameter included.parameters
              && builtins.hasAttr parentParameter launch.parameters
              && included.parameters.${includedParameter}.type != launch.parameters.${parentParameter}.type
            ) "launch `${record.coordinate}` include `${includeId}` forwards incompatible parameter types"
          ) include.parameters
        )
        ++
          optional (missingRequired != [ ])
            "launch `${record.coordinate}` include `${includeId}` does not forward required parameters: ${concatStringsSep ", " missingRequired}"
      ) launch.includes
    )
    ++ map (capability: "launch `${record.coordinate}` capability ID `${capability}` is invalid") (
      filter (capability: !validId capability) capabilityIds
    )
    ++ optional (
      length (unique capabilityIds) != length capabilityIds
    ) "launch `${record.coordinate}` has duplicate or conflicting capabilities";

  errorsForProject =
    projectId: project:
    let
      packageId = packageIdFor projectId project;
      standardActionNames = attrNames (presetActionsFor project);
      customActionNames = attrNames project.customActions;
      actionNames = attrNames (normalizedBaseActions project);
      definitionInputName = definitionInput project;
      invalidDependencies = concatLists (
        mapAttrsToList (
          actionId: action:
          map (dependency: "${actionId} -> ${dependency}") (
            filter (dependency: !(builtins.elem dependency actionNames)) action.dependsOn
          )
        ) (normalizedBaseActions project)
      );
      cyclicActions = filter (
        actionId: builtins.elem actionId (dependencyClosure project actionNames actionId)
      ) actionNames;
      version = project.softwareVersion;
      coherentVersion =
        if version.source == "native" then
          version.value == null && version.file == null
        else if version.source == "literal" then
          version.value != null && nonBlank version.value && version.file == null
        else
          version.value == null && version.file != null;
      releaseTargets = filter (target: target.release != null) (
        builtins.attrValues (effectiveTargets project)
      );
    in
    optional (!validId projectId) "project ID `${projectId}` is invalid"
    ++ optional (!validId packageId) "project `${projectId}` package ID `${packageId}` is invalid"
    ++ map (alias: "package `${packageId}` alias ID `${alias}` is invalid") (
      filter (alias: !validId alias) project.aliases
    )
    ++ optional (
      length (unique project.aliases) != length project.aliases
    ) "package `${packageId}` declares duplicate aliases"
    ++ optional (builtins.elem packageId project.aliases) "package `${packageId}` cannot also declare its package ID as an alias"
    ++ optional (
      !coherentVersion
    ) "package `${packageId}` has an incoherent `${version.source}` software-version declaration"
    ++ optional (
      version.file != null && !validRelativePath version.file
    ) "package `${packageId}` software-version file `${version.file}` is not a safe relative path"
    ++ optional (
      project.owner != null && !nonBlank project.owner
    ) "package `${packageId}` owner must not be blank"
    ++ optional (
      project.license.spdx != null && !validSpdxExpression project.license.spdx
    ) "package `${packageId}` SPDX license expression `${project.license.spdx}` is invalid"
    ++ optional (
      project.deployability == "deployable" && releaseTargets == [ ]
    ) "package `${packageId}` is deployable but declares no immutable target release output"
    ++
      optional (project.deployability != "deployable" && releaseTargets != [ ])
        "package `${packageId}` deployability `${project.deployability}` cannot declare immutable target release outputs"
    ++ optional (
      !validId project.repositoryId
    ) "project `${projectId}` has invalid repository ID `${project.repositoryId}`"
    ++ optional (
      !validId project.source.input
    ) "project `${projectId}` has invalid source input `${project.source.input}`"
    ++ optional (
      !validRelativePath project.source.root
    ) "project `${projectId}` source root `${project.source.root}` is not a safe relative path"
    ++ map (
      dependency: "project `${projectId}` source dependency package ID `${dependency}` is invalid"
    ) (filter (dependency: !validId dependency) project.source.dependencies)
    ++ map (dependency: "project `${projectId}` has unresolved source dependency `${dependency}`") (
      filter (
        dependency: validId dependency && !(builtins.elem dependency packageIds)
      ) project.source.dependencies
    )
    ++ optional (builtins.elem packageId project.source.dependencies) "project `${projectId}` cannot depend on its own source"
    ++
      map (dependency: "public project `${projectId}` cannot depend on private source `${dependency}`")
        (
          filter (
            dependency:
            project.source.visibility == "public"
            && builtins.hasAttr dependency packageVisibilityIndex
            && packageVisibilityIndex.${dependency} == "private"
          ) project.source.dependencies
        )
    ++ optional (
      length (unique project.source.dependencies) != length project.source.dependencies
    ) "project `${projectId}` declares duplicate source dependencies"
    ++ optional (
      !validId definitionInputName
    ) "project `${projectId}` has invalid definition input `${definitionInputName}`"
    ++ optional (
      project.definition.origin == "external"
      && (project.definition.input == null || definitionInputName == project.source.input)
    ) "project `${projectId}` external definition must name an input distinct from its source"
    ++ optional (
      project.definition.origin != "external" && definitionInputName != project.source.input
    ) "project `${projectId}` ${project.definition.origin} definition must be owned by its source input"
    ++ map (actionId: "project `${projectId}` custom action ID `${actionId}` is invalid") (
      filter (actionId: !validId actionId) customActionNames
    )
    ++ map (actionId: "project `${projectId}` custom action `${actionId}` replaces preset behavior") (
      filter (actionId: builtins.elem actionId standardActionNames) customActionNames
    )
    ++ concatLists (
      mapAttrsToList (
        actionId: action:
        optional
          (
            action.argv == [ ]
            || !(builtins.isString (builtins.head action.argv))
            || builtins.head action.argv == ""
          )
          "project `${projectId}` action `${actionId}` must provide a non-empty literal executable as argv[0]"
        ++ map (name: "project `${projectId}` action `${actionId}` environment name `${name}` is invalid") (
          filter (name: !validEnvironmentName name) (attrNames action.environment)
        )
      ) (normalizedBaseActions project)
    )
    ++
      optional (invalidDependencies != [ ])
        "project `${projectId}` has unresolved action dependencies: ${concatStringsSep ", " invalidDependencies}"
    ++
      optional (cyclicActions != [ ])
        "project `${projectId}` has an action dependency cycle involving: ${concatStringsSep ", " cyclicActions}"
    ++ concatLists (mapAttrsToList (errorsForTarget packageId project) (effectiveTargets project))
    ++ concatLists (
      mapAttrsToList (
        resourceId: resource:
        optional (!validId resourceId) "package `${packageId}` resource ID `${resourceId}` is invalid"
        ++
          optional (!validRelativePath resource.path)
            "package `${packageId}` resource `${resourceId}` path `${resource.path}` is not a safe relative path"
      ) project.resources
    )
    ++ (
      let
        resourceRecords = mapAttrsToList (resourceId: resource: {
          inherit resourceId resource;
        }) project.resources;
        overlappingResources = filter (
          left:
          builtins.any (
            right:
            left.resourceId != right.resourceId
            && validRelativePath left.resource.path
            && validRelativePath right.resource.path
            && pathsOverlap left.resource.path right.resource.path
          ) resourceRecords
        ) resourceRecords;
        exportCollisions = filter (resourceId: builtins.hasAttr resourceId project.executables) (
          attrNames project.resources
        );
        executableReferences = map (executable: executable.from) (builtins.attrValues project.executables);
      in
      optional (overlappingResources != [ ])
        "package `${packageId}` has overlapping resource ownership: ${
          concatStringsSep ", " (map (record: record.resourceId) overlappingResources)
        }"
      ++
        optional (exportCollisions != [ ])
          "package `${packageId}` resource and executable export IDs collide: ${concatStringsSep ", " exportCollisions}"
      ++ optional (
        length (unique executableReferences) != length executableReferences
      ) "package `${packageId}` exports the same executable artifact more than once"
      ++ concatLists (
        mapAttrsToList (
          executableId: executable:
          optional (!validId executableId) "package `${packageId}` executable ID `${executableId}` is invalid"
          ++
            optional (parseArtifactCoordinate executable.from == null)
              "package `${packageId}` executable `${executableId}` reference `${executable.from}` must use package:target:artifact IDs"
          ++
            optional
              (
                parseArtifactCoordinate executable.from != null
                && !(builtins.hasAttr executable.from artifactOutputIndex)
              )
              "package `${packageId}` executable `${executableId}` has unresolved artifact reference `${executable.from}`"
          ++
            optional
              (
                parseArtifactCoordinate executable.from != null
                && (parseArtifactCoordinate executable.from).packageId != packageId
              )
              "package `${packageId}` executable `${executableId}` cannot export an artifact owned by another package"
          ++ optional (
            builtins.hasAttr executable.from artifactOutputIndex
            && artifactOutputIndex.${executable.from}.artifact.kind != "executable"
          ) "package `${packageId}` executable `${executableId}` must reference an executable artifact"
          ++ optional (builtins.any (argument: argument == "")
            executable.argv
          ) "package `${packageId}` executable `${executableId}` argv contains an empty entry"
        ) project.executables
      )
    );

  artifactReferenceErrors = concatLists (
    map (
      record:
      optional (record.producer != null && !(builtins.hasAttr record.input.from artifactOutputIndex))
        "package `${record.packageId}` target `${record.targetId}` artifact input `${record.inputId}` has unresolved reference `${record.input.from}`"
      ++
        optional
          (
            record.producer != null
            && builtins.hasAttr record.input.from artifactOutputIndex
            && artifactOutputIndex.${record.input.from}.artifact.contract != record.input.contract
          )
          "package `${record.packageId}` target `${record.targetId}` artifact input `${record.inputId}` contract does not match `${record.input.from}`"
      ++ optional (
        record.producer != null
        && builtins.hasAttr record.input.from artifactOutputIndex
        && packageVisibilityIndex.${record.packageId} == "public"
        && packageVisibilityIndex.${record.producer.packageId} == "private"
      ) "public package `${record.packageId}` cannot consume private artifact `${record.input.from}`"
    ) artifactInputRecords
  );

  artifactActionDependencies =
    actionRecord:
    unique (
      concatLists (
        map (
          record:
          let
            output =
              if builtins.hasAttr record.input.from artifactOutputIndex then
                artifactOutputIndex.${record.input.from}
              else
                null;
            dependency =
              if output == null || output.producingAction == null then
                null
              else
                "${record.producer.target}:${output.producingAction}";
          in
          optional (
            record.consumer == actionRecord.target
            && builtins.elem actionRecord.actionId record.consumingActions
            && dependency != null
            && builtins.elem dependency actionCoordinates
          ) dependency
        ) artifactInputRecords
      )
    );

  actionDependenciesFor =
    actionRecord:
    unique (
      map (dependency: "${actionRecord.target}:${dependency}") (actionRecord.action.dependsOn or [ ])
      ++ artifactActionDependencies actionRecord
    );

  actionDependencyClosure =
    actionCoordinate:
    builtins.foldl'
      (
        reached: _:
        unique (
          reached
          ++ concatLists (
            map (
              dependency:
              if builtins.hasAttr dependency actionIndex then
                actionDependenciesFor actionIndex.${dependency}
              else
                [ ]
            ) reached
          )
        )
      )
      (actionDependenciesFor actionIndex.${actionCoordinate})
      (builtins.genList (_: null) (length actionCoordinates));

  cyclicActionCoordinates = filter (
    actionCoordinate: builtins.elem actionCoordinate (actionDependencyClosure actionCoordinate)
  ) actionCoordinates;

  artifactConsumerClaims = concatLists (
    map (
      record:
      map (actionId: {
        coordinate = "${record.consumer}:${actionId}";
        artifact = record.input.from;
        input = record.inputId;
        key = "${record.consumer}:${actionId}->${record.input.from}";
      }) record.consumingActions
    ) artifactInputRecords
  );

  duplicateArtifactConsumerClaims = filter (
    claim: length (filter (other: other.key == claim.key) artifactConsumerClaims) > 1
  ) artifactConsumerClaims;

  launchCoordinates = map (record: record.coordinate) launchRecords;

  launchDependencies =
    launchCoordinate:
    if builtins.hasAttr launchCoordinate launchIndex then
      map (include: include.launch) (builtins.attrValues launchIndex.${launchCoordinate}.launch.includes)
    else
      [ ];

  launchDependencyClosure =
    launchCoordinate:
    builtins.foldl' (
      reached: _:
      unique (
        reached
        ++ concatLists (
          map (
            dependency:
            if builtins.elem dependency launchCoordinates then launchDependencies dependency else [ ]
          ) reached
        )
      )
    ) (launchDependencies launchCoordinate) (builtins.genList (_: null) (length launchCoordinates));

  launchComposition =
    launchCoordinate: unique ([ launchCoordinate ] ++ launchDependencyClosure launchCoordinate);

  launchCapabilityErrors = concatLists (
    map (
      record:
      let
        composition = launchComposition record.coordinate;
        required = unique (
          concatLists (map (coordinate: launchIndex.${coordinate}.launch.capabilities.requires) composition)
        );
        providersFor =
          capability:
          filter (
            coordinate: builtins.elem capability launchIndex.${coordinate}.launch.capabilities.provides
          ) composition;
        missing = filter (capability: providersFor capability == [ ]) required;
        ambiguous = filter (capability: length (providersFor capability) > 1) required;
      in
      map (
        capability:
        "launch `${record.coordinate}` requires capability `${capability}` but selects no provider"
      ) missing
      ++ map (
        capability:
        "launch `${record.coordinate}` requires capability `${capability}` but selects multiple providers: ${concatStringsSep ", " (providersFor capability)}"
      ) ambiguous
    ) launchRecords
  );

  cyclicLaunches = filter (
    launchCoordinate: builtins.elem launchCoordinate (launchDependencyClosure launchCoordinate)
  ) launchCoordinates;

  validationErrors =
    optional (
      collidingIdentityIds != [ ]
    ) "package IDs and aliases collide: ${concatStringsSep ", " collidingIdentityIds}"
    ++ concatLists (mapAttrsToList errorsForProject projects)
    ++ artifactReferenceErrors
    ++
      optional (duplicateArtifactConsumerClaims != [ ])
        "duplicate artifact consumption declarations: ${
          concatStringsSep ", " (unique (map (claim: claim.key) duplicateArtifactConsumerClaims))
        }"
    ++ optional (
      cyclicActionCoordinates != [ ]
    ) "artifact/action dependency cycle involves: ${concatStringsSep ", " cyclicActionCoordinates}"
    ++ concatLists (map errorsForLaunch launchRecords)
    ++ optional (
      cyclicLaunches != [ ]
    ) "launch include cycle involves: ${concatStringsSep ", " cyclicLaunches}"
    ++ (if cyclicLaunches == [ ] then launchCapabilityErrors else [ ]);

  normalizedProjects = mapAttrs (projectId: project: {
    id = projectId;
    packageId = packageIdFor projectId project;
    inherit (project)
      aliases
      deployability
      lifecycle
      license
      owner
      preset
      repositoryId
      softwareVersion
      ;
    source = {
      inherit (project.source)
        dependencies
        input
        root
        visibility
        ;
    };
    targets = mapAttrs (targetId: target: {
      id = targetId;
      actions = normalizedActions project target;
      artifacts = normalizedArtifacts project target;
      inherit (target) variants release;
    }) (effectiveTargets project);
    inherit (project)
      executables
      launches
      resources
      ;
    compliance = {
      bespokeAdapterCount = length (attrNames project.customActions);
      warnings = complianceWarningsFor project;
      warningCount = length (complianceWarningsFor project);
    };
  }) projects;

  publicPackageCoordinate =
    coordinate:
    let
      parsed = parsePackageCoordinate coordinate;
    in
    if parsed == null then coordinate else "${parsed.packageId}/${parsed.name}";

  # Project modules use package:name references because they are schema
  # foreign keys.  The cached/public workspace interface uses package/name
  # coordinates accepted directly by `nixspace resource` and `nixspace run`.
  publicLaunchSummary =
    launch:
    launch
    // {
      requiredResources = map publicPackageCoordinate launch.requiredResources;
      processes = mapAttrs (
        _: process: process // { executable = publicPackageCoordinate process.executable; }
      ) launch.processes;
      includes = mapAttrs (
        _: include: include // { launch = publicPackageCoordinate include.launch; }
      ) launch.includes;
    };

  staticCatalog = {
    packages = builtins.sort (left: right: left.packageId < right.packageId) (
      builtins.attrValues normalizedProjects
    );
    targets = builtins.sort (left: right: left.coordinate < right.coordinate) (
      concatLists (
        mapAttrsToList (
          _: project:
          mapAttrsToList (targetId: target: {
            coordinate = "${project.packageId}/${targetId}";
            inherit (project) packageId;
            inherit target;
          }) project.targets
        ) normalizedProjects
      )
    );
    artifacts = builtins.sort (left: right: left.coordinate < right.coordinate) (
      concatLists (
        mapAttrsToList (
          _: project:
          concatLists (
            mapAttrsToList (
              targetId: target:
              mapAttrsToList (
                artifactId: artifact:
                artifact
                // {
                  coordinate = "${project.packageId}:${targetId}:${artifactId}";
                  inherit (project) packageId;
                  inherit targetId artifactId;
                }
              ) target.artifacts.outputs
            ) project.targets
          )
        ) normalizedProjects
      )
    );
    resources = builtins.sort (left: right: left.coordinate < right.coordinate) (
      concatLists (
        mapAttrsToList (
          _: project:
          mapAttrsToList (
            resourceId: resource:
            resource
            // {
              coordinate = "${project.packageId}/${resourceId}";
              inherit (project) packageId;
              inherit resourceId;
            }
          ) project.resources
        ) normalizedProjects
      )
    );
    executables = builtins.sort (left: right: left.coordinate < right.coordinate) (
      concatLists (
        mapAttrsToList (
          _: project:
          mapAttrsToList (
            executableId: executable:
            executable
            // {
              coordinate = "${project.packageId}/${executableId}";
              inherit (project) packageId;
              inherit executableId;
            }
          ) project.executables
        ) normalizedProjects
      )
    );
    launches = builtins.sort (left: right: left.coordinate < right.coordinate) (
      concatLists (
        mapAttrsToList (
          _: project:
          mapAttrsToList (launchId: launch: {
            coordinate = "${project.packageId}/${launchId}";
            inherit (project) packageId;
            inherit launchId;
            launch = publicLaunchSummary launch;
          }) project.launches
        ) normalizedProjects
      )
    );
  };

  expandLaunchTemplate =
    launchCoordinate: instanceId: parameterBindings: includedBy:
    let
      record = launchIndex.${launchCoordinate};
      inherit (record) launch;
      publicCoordinate = "${record.packageId}/${record.launchId}";
      childTemplates = mapAttrsToList (
        includeId: include:
        let
          included = launchIndex.${include.launch}.launch;
          childBindings = mapAttrs (
            parameterId: _:
            if builtins.hasAttr parameterId include.parameters then
              parameterBindings.${include.parameters.${parameterId}}
            else
              null
          ) included.parameters;
        in
        expandLaunchTemplate include.launch "${instanceId}/${includeId}" childBindings instanceId
      ) launch.includes;
      processTemplates = mapAttrsToList (
        processId: process:
        let
          executable = executableIndex.${process.executable}.executable;
        in
        {
          id = "${instanceId}/${processId}";
          instance = instanceId;
          launch = publicCoordinate;
          inherit processId;
          inherit (process)
            argv
            endpoints
            environment
            onExit
            onReadinessLoss
            readiness
            required
            restart
            shutdown
            workingDirectory
            ;
          executable = publicPackageCoordinate process.executable;
          artifact = executable.from;
          executableArgv = executable.argv;
          dependencies = mapAttrs' (dependency: state: {
            name = "${instanceId}/${dependency}";
            value = state;
          }) process.dependencies;
        }
      ) launch.processes;
    in
    {
      instances = [
        {
          inherit instanceId includedBy parameterBindings;
          launch = publicCoordinate;
          inherit (launch) parameters;
        }
      ]
      ++ concatLists (map (template: template.instances) childTemplates);
      processes = processTemplates ++ concatLists (map (template: template.processes) childTemplates);
      requiredArtifacts =
        launch.requiredArtifacts ++ concatLists (map (template: template.requiredArtifacts) childTemplates);
      requiredResources =
        launch.requiredResources ++ concatLists (map (template: template.requiredResources) childTemplates);
      provides =
        launch.capabilities.provides ++ concatLists (map (template: template.provides) childTemplates);
      requires =
        launch.capabilities.requires ++ concatLists (map (template: template.requires) childTemplates);
      sessionEnvironment = builtins.foldl' (
        environment: template: environment // template.sessionEnvironment
      ) launch.sessionEnvironment childTemplates;
    };

  staticLaunchPlans = builtins.listToAttrs (
    map (
      record:
      let
        publicCoordinate = "${record.packageId}/${record.launchId}";
        expanded = expandLaunchTemplate record.coordinate publicCoordinate (mapAttrs (
          parameterId: _: parameterId
        ) record.launch.parameters) null;
      in
      {
        name = publicCoordinate;
        value = {
          launch = publicCoordinate;
          parameters = record.launch.parameters;
          requiredArtifacts = builtins.sort builtins.lessThan (unique expanded.requiredArtifacts);
          requiredResources = builtins.sort builtins.lessThan (
            unique (map publicPackageCoordinate expanded.requiredResources)
          );
          capabilities = {
            provides = builtins.sort builtins.lessThan (unique expanded.provides);
            requires = builtins.sort builtins.lessThan (unique expanded.requires);
          };
          inherit (expanded) sessionEnvironment;
          inherit (expanded) instances processes;
        };
      }
    ) launchRecords
  );

  graphTargetRecords = concatLists (
    mapAttrsToList (
      projectId: project:
      mapAttrsToList (targetId: target: {
        inherit
          project
          projectId
          targetId
          target
          ;
        inherit (project) packageId;
        schemaCoordinate = "${project.packageId}:${targetId}";
        coordinate = "${project.packageId}/${targetId}";
      }) project.targets
    ) normalizedProjects
  );
  graphTargetIndex = builtins.listToAttrs (
    map (record: {
      name = record.schemaCoordinate;
      value = record;
    }) graphTargetRecords
  );
  graphTargetNode = record: {
    id = record.coordinate;
    type = "target";
    package = record.packageId;
    target = record.targetId;
  };
  graphActionNodes =
    record:
    mapAttrsToList (actionId: action: {
      id = "${record.coordinate}/${actionId}";
      type = "action";
      package = record.packageId;
      target = record.targetId;
      action = actionId;
      inherit (action) kind adapter;
    }) record.target.actions;
  graphActionEdges =
    record:
    concatLists (
      mapAttrsToList (
        actionId: action:
        map (dependency: {
          from = "${record.coordinate}/${dependency}";
          to = "${record.coordinate}/${actionId}";
          kind = "action";
        }) (action.dependsOn or [ ])
      ) record.target.actions
    );
  graphArtifactRecords = concatLists (
    map (
      record:
      mapAttrsToList (
        inputId: input:
        let
          producer = parseArtifactCoordinate input.from;
          producerTarget = graphTargetIndex.${producer.target};
        in
        {
          inherit inputId;
          producerSchemaCoordinate = producer.target;
          consumerSchemaCoordinate = record.schemaCoordinate;
          document = {
            from = producerTarget.coordinate;
            to = record.coordinate;
            kind = "artifact";
            producerArtifact = input.from;
            consumerInput = inputId;
            inherit (input) contract;
          };
        }
      ) record.target.artifacts.inputs
    ) graphTargetRecords
  );
  graphSourceRecords = concatLists (
    map (
      record:
      concatLists (
        map (
          dependency:
          map (producerTarget: {
            producerSchemaCoordinate = producerTarget.schemaCoordinate;
            consumerSchemaCoordinate = record.schemaCoordinate;
            document = {
              from = producerTarget.coordinate;
              to = record.coordinate;
              kind = "source";
            };
          }) (filter (candidate: candidate.packageId == dependency) graphTargetRecords)
        ) record.project.source.dependencies
      )
    ) graphTargetRecords
  );
  graphDependencyRecords = graphArtifactRecords ++ graphSourceRecords;
  sortGraphNodes = builtins.sort (left: right: left.id < right.id);
  graphEdgeKey = edge: "${edge.from}\n${edge.to}\n${edge.kind}\n${edge.producerArtifact or ""}";
  sortGraphEdges = builtins.sort (left: right: graphEdgeKey left < graphEdgeKey right);
  graphDocument = nodeRecords: actionRecords: artifactRecords: sourceRecords: {
    nodes = sortGraphNodes (
      map graphTargetNode nodeRecords ++ concatLists (map graphActionNodes actionRecords)
    );
    edges = sortGraphEdges (
      concatLists (map graphActionEdges actionRecords)
      ++ map (record: record.document) artifactRecords
      ++ map (record: record.document) sourceRecords
    );
  };
  graphPackageDocument =
    project:
    let
      selected = filter (record: record.packageId == project.packageId) graphTargetRecords;
      selectedCoordinates = map (record: record.schemaCoordinate) selected;
      included = builtins.foldl' (
        reached: _:
        unique (
          reached
          ++ map (record: record.producerSchemaCoordinate) (
            filter (record: builtins.elem record.consumerSchemaCoordinate reached) graphDependencyRecords
          )
        )
      ) selectedCoordinates (builtins.genList (_: null) (length graphTargetRecords));
      artifactRecords = filter (
        record:
        builtins.elem record.producerSchemaCoordinate included
        && builtins.elem record.consumerSchemaCoordinate included
      ) graphArtifactRecords;
      sourceRecords = filter (
        record:
        builtins.elem record.producerSchemaCoordinate included
        && builtins.elem record.consumerSchemaCoordinate included
      ) graphSourceRecords;
      nodeRecords = filter (record: builtins.elem record.schemaCoordinate included) graphTargetRecords;
    in
    graphDocument nodeRecords selected artifactRecords sourceRecords;
  graphReverseDocument =
    project:
    let
      seeds = map (record: record.schemaCoordinate) (
        filter (record: record.packageId == project.packageId) graphTargetRecords
      );
      included = builtins.foldl' (
        reached: _:
        unique (
          reached
          ++ map (record: record.consumerSchemaCoordinate) (
            filter (record: builtins.elem record.producerSchemaCoordinate reached) graphDependencyRecords
          )
        )
      ) seeds (builtins.genList (_: null) (length graphTargetRecords));
      records = filter (record: builtins.elem record.schemaCoordinate included) graphTargetRecords;
      artifactRecords = filter (
        record:
        builtins.elem record.producerSchemaCoordinate included
        && builtins.elem record.consumerSchemaCoordinate included
      ) graphArtifactRecords;
      sourceRecords = filter (
        record:
        builtins.elem record.producerSchemaCoordinate included
        && builtins.elem record.consumerSchemaCoordinate included
      ) graphSourceRecords;
    in
    graphDocument records records artifactRecords sourceRecords;
  staticGraph = {
    schemaVersion = 1;
    all = graphDocument graphTargetRecords graphTargetRecords graphArtifactRecords graphSourceRecords;
    packages = builtins.listToAttrs (
      mapAttrsToList (_: project: {
        name = project.packageId;
        value = graphPackageDocument project;
      }) normalizedProjects
    );
    reverse = builtins.listToAttrs (
      mapAttrsToList (_: project: {
        name = project.packageId;
        value = graphReverseDocument project;
      }) normalizedProjects
    );
  };

  staticResolutionTemplate = import ./resolution-template.nix {
    inherit
      actionRecords
      artifactConsumerClaims
      artifactInputRecords
      artifactOutputRecords
      executableRecords
      graphArtifactRecords
      graphDependencyRecords
      graphTargetIndex
      lib
      normalizedProjects
      packageIds
      resourceRecords
      staticLaunchPlans
      ;
  };
  actionTasksForProject =
    kind: project:
    builtins.sort builtins.lessThan (
      concatLists (
        mapAttrsToList (
          targetId: target:
          mapAttrsToList (actionId: _: "${project.packageId}:${targetId}:${actionId}") (
            lib.filterAttrs (_: action: action.kind == kind) target.actions
          )
        ) project.targets
      )
    );
  actionPlanForKind =
    kind:
    let
      packages = builtins.listToAttrs (
        mapAttrsToList (_: project: {
          name = project.packageId;
          value = actionTasksForProject kind project;
        }) normalizedProjects
      );
    in
    {
      all = builtins.sort builtins.lessThan (unique (concatLists (builtins.attrValues packages)));
      inherit packages;
    };
  staticActionPlans = {
    schemaVersion = 2;
    runner = {
      kind = "devenv-task";
      direct = {
        argv = [
          "devenv-flake-tasks"
          "run"
        ];
        requiredEnvironment = [
          "DEVENV_TASK_FILE"
          "NIXSPACE_INDEX"
          "NIXSPACE_WORKSPACE_ROOT"
        ];
      };
      bootstrap = {
        argv = [
          "nix"
          "develop"
          "--no-pure-eval"
          ".#default"
          "--command"
          "devenv-flake-tasks"
          "run"
        ];
        environment.NIXSPACE_WORKSPACE_ROOT = "workspace-root";
      };
    };
    actions = {
      build = actionPlanForKind "build";
      test = actionPlanForKind "test";
    };
  };
in
{
  options.cognipilot = {
    interfaceVersion = mkOption {
      type = types.enum [ 1 ];
      default = 1;
      description = "CogniPilot project-module interface major.";
    };
    projects = mkOption {
      type = types.attrsOf (types.uniq projectType);
      default = { };
      description = "Selected typed CogniPilot project definitions.";
    };
    validatedIndex = mkOption {
      type = types.attrs;
      readOnly = true;
      description = "Definition-location-independent, JSON-safe project index.";
    };
    nixspaceIndex = mkOption {
      type = types.attrs;
      readOnly = true;
      description = "Generic nixspace Workspace v2 projection with provider policy isolated in namespaced extensions.";
    };
  };

  config.cognipilot.validatedIndex =
    if validationErrors == [ ] then
      {
        apiVersion = "nixspace/v1";
        kind = "Workspace";
        interfaceVersion = config.cognipilot.interfaceVersion;
        projects = normalizedProjects;
        catalog = staticCatalog;
        graph = staticGraph;
        launchPlans = staticLaunchPlans;
        actionPlans = staticActionPlans;
        resolutionTemplate = staticResolutionTemplate;
        compliance = {
          bespokeAdapterCount = builtins.foldl' (
            count: project: count + project.compliance.bespokeAdapterCount
          ) 0 (builtins.attrValues normalizedProjects);
          warningCount = builtins.foldl' (count: project: count + project.compliance.warningCount) 0 (
            builtins.attrValues normalizedProjects
          );
        };
      }
    else
      throw ''
        CogniPilot project contract violations:
        - ${concatStringsSep "\n- " validationErrors}
      '';

  config.cognipilot.nixspaceIndex = import ./nixspace-index.nix {
    index = config.cognipilot.validatedIndex;
  };
}
