{
  description = "CogniPilot development product workspace";

  # Flake schema evaluation requires nixConfig to be a literal set. The host
  # plan mirrors this list and a policy test rejects any drift.
  nixConfig = {
    extra-substituters = [
      "https://devenv.cachix.org"
      "https://cachix.cachix.org"
      "https://cognipilot.cachix.org"
      "https://ros.cachix.org"
    ];
    extra-trusted-public-keys = [
      "devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw="
      "cachix.cachix.org-1:eWNHQldwUO7G2VkjpnjDbWwy4KQ/HNxht7H4SSoMckM="
      "cognipilot.cachix.org-1:TSm+h3sAVYP0B+D+1uG5hvgI/4JFAS3LFvv/yhTwvK8="
      "ros.cachix.org-1:dSyZxI8geDCJrwgvCOHDoAfOm5sV1wCPjBkKL+38Rvo="
    ];
  };

  inputs = {
    # Direnv overrides this empty, host-independent file with a generated
    # absolute workspace root. Pure consumers fall back to self.outPath.
    devenv-root = {
      url = "file+file:///dev/null";
      flake = false;
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
    devenv = {
      url = "github:cachix/devenv/407080febcc800abfd0fd688a0d513884aad620c";
      inputs.flake-parts.follows = "flake-parts";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    synapse_fbs_source = {
      url = "github:CogniPilot/synapse_fbs/f6efaadf1a1cf0c48608b9b104b578da51322979";
      flake = false;
    };
    synapse_fbs_definition.url = "path:./nix/project-definitions/synapse_fbs";

    cerebri_cubs2_source = {
      url = "github:CogniPilot/cerebri_cubs2/8c4e2f29928445448ab2bef897d2135d0be8c1ea";
      flake = false;
    };
    cerebri_cubs2_definition.url = "path:./nix/project-definitions/cerebri_cubs2";

    electrode_web_source = {
      url = "github:CogniPilot/electrode_web/bda10981cddd91ba12c216df67d34081fa21730b";
      flake = false;
    };
    electrode_web_definition.url = "path:./nix/project-definitions/electrode_web";

    rumoca_source = {
      url = "github:CogniPilot/rumoca/25ca822383954db4e71d2e522b56a81f0f3a6161";
      flake = false;
    };
    rumoca_definition.url = "path:./nix/project-definitions/rumoca";

    modelica_models_source = {
      url = "github:CogniPilot/modelica_models/8e0c1ddd0a48e2a7106f6b79137d4a04fcc94b37";
      flake = false;
    };
    modelica_models_definition.url = "path:./nix/project-definitions/modelica_models";

    csyn_source = {
      url = "github:CogniPilot/csyn/46debf42bb2b310c00ed5723f61989f1a2705634";
      flake = false;
    };
    csyn_definition.url = "path:./nix/project-definitions/csyn";

    cerebri_modules_source = {
      url = "github:CogniPilot/cerebri_modules/ef73a4f8adeb4385c34af4344c9af27e43e02033";
      flake = false;
    };
    cerebri_modules_definition.url = "path:./nix/project-definitions/cerebri_modules";

    zros_source = {
      url = "github:CogniPilot/zros/32df0f27ef2fac9e30d48e9a78ddf0c9f45dfe9c";
      flake = false;
    };
    zros_definition.url = "path:./nix/project-definitions/zros";

    zros_drivers_source = {
      url = "github:CogniPilot/zros_drivers/e0b37b4ae5f03f9bc985bf6f00fce2aee16b8e84";
      flake = false;
    };
    zros_drivers_definition.url = "path:./nix/project-definitions/zros_drivers";

    qualisys_rust_sdk_source = {
      url = "github:CogniPilot/qualisys_rust_sdk/a8ed1ac40bfb3007828cb581c8631fd6f79b2f9d";
      flake = false;
    };
    qualisys_rust_sdk_definition.url = "path:./nix/project-definitions/qualisys_rust_sdk";

    synapse_qualisys_bridge_source = {
      url = "github:CogniPilot/synapse_qualisys_bridge/4017e56ed132fd8527d5386fe208d7927c6c9bef";
      flake = false;
    };
    synapse_qualisys_bridge_definition.url = "path:./nix/project-definitions/synapse_qualisys_bridge";

    synapse_ppm_bridge_source = {
      url = "github:CogniPilot/synapse_ppm_bridge/5fb6919e100f899cc7a029d6f71c06d1c9b89b16";
      flake = false;
    };
    synapse_ppm_bridge_definition = {
      url = "path:./nix/project-definitions/synapse_ppm_bridge";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.source.follows = "synapse_ppm_bridge_source";
    };

    cerebri_rdd2_source = {
      url = "github:CogniPilot/cerebri_rdd2/10bb0de2b443d3292b50e2b7ccccfe73e9192184";
      flake = false;
    };
    cerebri_rdd2_definition.url = "path:./nix/project-definitions/cerebri_rdd2";

    csyn_ros2_bridge_source = {
      url = "git+https://github.com/CogniPilot/csyn_ros2_bridge.git?rev=e709f66869dc7ea90dcf48cc4e800c0613eb4d4e&submodules=1";
      flake = false;
    };
    csyn_ros2_bridge_definition.url = "path:./nix/project-definitions/csyn_ros2_bridge";

    fastdyn_source = {
      url = "git+https://github.com/jgoppert/FastDyn.git?rev=376ad4e3adc2085e02837a2d38c2472b52f5ea98";
      flake = false;
    };
    fastdyn_definition.url = "path:./nix/project-definitions/fastdyn";
  };

  outputs =
    inputs@{ flake-parts, ... }:
    let
      cachePolicy = import ./nix/cognipilot/cache-policy.nix;
    in
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        ./nix/cognipilot/devenv-flake-module.nix
        ./nix/cognipilot/flake-module.nix
        ./nix/cognipilot/product-flake-module.nix
        ./nix/cognipilot/compliance-flake-module.nix
        ./nix/cognipilot/workspace-policy-module.nix
        ./nix/cognipilot/devenv-task-module.nix
        ./nix/cognipilot/devenv-launch-module.nix
        ./nix/cognipilot/devenv-workspace-module.nix
        ./nix/cognipilot/resolution-module.nix
        ./nix/cognipilot/source-workspace-module.nix
        ./nix/nixspace/west-workspace-module.nix
        ./nix/nixspace/host-module.nix
        ./nix/nixspace/benchmark-module.nix
        ./nix/cognipilot/nixspace-module.nix
        inputs.synapse_fbs_definition.flakeModules.default
        inputs.cerebri_cubs2_definition.flakeModules.default
        inputs.electrode_web_definition.flakeModules.default
        inputs.rumoca_definition.flakeModules.default
        inputs.modelica_models_definition.flakeModules.default
        inputs.csyn_definition.flakeModules.default
        inputs.cerebri_modules_definition.flakeModules.default
        inputs.zros_definition.flakeModules.default
        inputs.zros_drivers_definition.flakeModules.default
        inputs.qualisys_rust_sdk_definition.flakeModules.default
        inputs.synapse_qualisys_bridge_definition.flakeModules.default
        inputs.synapse_ppm_bridge_definition.flakeModules.default
        inputs.cerebri_rdd2_definition.flakeModules.default
        inputs.csyn_ros2_bridge_definition.flakeModules.default
        inputs.fastdyn_definition.flakeModules.default
      ];

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      cognipilot.product.name = "cognipilot-development";
      # This public composition may retain private/local projects in its
      # editable catalog, but its immutable product, SBOM and attestations are
      # projections of public projects only.
      cognipilot.product.promotionVisibilities = [ "public" ];
      cognipilot.workspacePolicy.source = ./.;
      # This is a public CogniPilot product.  The explicitly private FastDyn
      # integration remains available for opted-in local development, but it
      # is neither compliance-enforced as a public package nor promotable into
      # the public Cachix root.
      cognipilot.compliancePolicy = {
        enforcedVisibilities = [ "public" ];
        allowedLifecycles = [
          "experimental"
          "stable"
        ];
      };
      cognipilot.sourceWorkspace = {
        enable = true;
        branches.fastdyn_source = "fix/cerebri-fastdyn-lockstep";
        paths.fastdyn_source = "src/FastDyn";
      };
      nixspace.host.plan = cachePolicy.hostPlan;
      nixspace.west = {
        enable = true;
        product = {
          id = "cognipilot-development";
          interfaceVersion = 1;
        };
        workspace = {
          id = "cerebri_cubs2";
          source = {
            input = "cerebri_cubs2_source";
            root = ".";
          };
          manifest = {
            resource = "cerebri_cubs2:west-manifest";
            relativePath = "west.yml";
          };
        };
        workspaceRoot = ".";
        localOverrides = {
          # Native West names the manifest repository `manifest`. Bind that
          # exact identity to the editable app checkout; otherwise the local
          # view would silently build the immutable flake input instead.
          manifest = {
            source = "src/cerebri_cubs2";
            required = true;
          };
          cerebri_modules = {
            source = "src/cerebri_modules";
            zephyrModule = true;
          };
          zros = {
            source = "src/zros";
            zephyrModule = true;
          };
          csyn = {
            source = "src/csyn";
            zephyrModule = true;
          };
          modelica_models.source = "src/modelica_models";
        };
      };
      cognipilot.devenvWorkspace = {
        enable = true;
        name = "cognipilot-development";
      };

      perSystem =
        {
          config,
          lib,
          pkgs,
          system,
          ...
        }:
        let
          client = lib.getExe config.packages.nixspace;
          index = "${config.packages.nixspace-index}/share/nixspace/index.json";
          resolutionPlan =
            "${config.packages.nixspace-resolution-plan}/share/nixspace/resolution-plan.json";
          selectedSource = toString inputs.self.outPath;
          selectedFlake = "path:${selectedSource}";
          nix = lib.getExe pkgs.nix;
          scale100Expression = pkgs.writeText "cognipilot-scale-100.nix" ''
            let
              index = import ${./nix/spikes/cognipilot-scale-fixture.nix} {
                count = 100;
                lib = import ${inputs.nixpkgs}/lib;
                module = import "${./nix/cognipilot}/flake-module.nix";
              };
            in
            builtins.hashString "sha256" (builtins.toJSON index)
          '';
          benchmarkFixtures = import ./nix/spikes/cognipilot-benchmark-fixtures.nix {
            inherit lib pkgs;
            nixspace = config.packages.nixspace;
            devenvSource = inputs.devenv;
          };
          # The contract check runs Nix with a private writable store so it can
          # evaluate fixtures without daemon access.  A build sandbox exposes
          # only declared derivation inputs, so retain the complete source graph
          # of every public root input explicitly.  Do not make the private
          # FastDyn integration a build-time dependency of the public cache
          # root merely to seed that nested store.
          contractExcludedInputNames = [
            "fastdyn_definition"
            "fastdyn_source"
          ];
          contractFlakeInputNames = builtins.filter (
            name: name != "self" && !(builtins.elem name contractExcludedInputNames)
          ) (builtins.attrNames inputs);
          contractInputNode = input: {
            key = toString input.outPath;
            value = input;
          };
          contractFlakeInputClosure = builtins.genericClosure {
            startSet = map contractInputNode (
              builtins.filter (
                input: builtins.isAttrs input && input ? outPath
              ) (map (name: inputs.${name}) contractFlakeInputNames)
            );
            operator = node:
              if builtins.isAttrs node.value && node.value ? inputs then
                map contractInputNode (
                  builtins.filter (
                    input: builtins.isAttrs input && input ? outPath
                  ) (builtins.attrValues node.value.inputs)
                )
              else
                [ ];
          };
          # Relative path inputs live inside `self` and are available when that
          # source is copied into a fixture. Import only independently addressed
          # top-level store paths into the nested store.
          contractFlakeInputSources = map (node: node.value.outPath) (
            builtins.filter (
              node:
              builtins.match "^${builtins.storeDir}/[^/]+$" (toString node.value.outPath) != null
            ) contractFlakeInputClosure
          );
          contractStoreName = source:
            let
              match = builtins.match "^[^-]+-(.+)$" (builtins.baseNameOf (toString source));
            in
            if match == null then
              throw "contract flake input is not a named Nix store path: ${toString source}"
            else
              builtins.head match;
          contractTestSelections =
            [
              "tests.test_cognipilot_compliance"
              "tests.test_cognipilot_flake_module"
              "tests.test_cognipilot_launch_ir"
              "tests.test_cognipilot_package_metadata"
              "tests.test_cognipilot_resolution"
              "tests.test_cognipilot_resources"
              "tests.test_cognipilot_source_workspace"
              "tests.test_ci_cache_policy"
              "tests.test_nixspace_generic"
              "tests.test_workspace_policy"
            ];
          contractTests =
            pkgs.runCommand "cognipilot-python-contract-tests"
              {
                # This value is deliberately unused by the command. Its string
                # contexts are the sandbox/source-availability contract for the
                # nested offline Nix store described above.
                inherit contractFlakeInputSources;
                source = inputs.self;
                passthru = {
                  inherit
                    contractExcludedInputNames
                    contractFlakeInputNames
                    contractFlakeInputSources
                    ;
                };
              }
              ''
                export PATH=${lib.makeBinPath [
                  pkgs.git
                  pkgs.nix
                  pkgs.python3
                ]}:$PATH
                export HOME="$TMPDIR/home"
                export NIX_CONFIG="experimental-features = nix-command flakes"
                export NIX_PATH=nixpkgs=${inputs.nixpkgs}
                export NIX_REMOTE="local?root=$TMPDIR/nix-store"
                mkdir -p "$HOME"
                # Merely mounting a source in the build sandbox does not register
                # it in this separate store. Import each exact public flake input
                # under its original store name so locked offline fetches resolve
                # without network access or the host daemon.
                ${lib.concatMapStringsSep "\n" (source: ''
                  imported="$(${lib.getExe pkgs.nix} store add \
                    --store "$NIX_REMOTE" --mode nar \
                    --name ${lib.escapeShellArg (contractStoreName source)} \
                    ${lib.escapeShellArg (toString source)})"
                  if [ "$imported" != ${lib.escapeShellArg (toString source)} ]; then
                    echo "nested store imported $imported instead of ${toString source}" >&2
                    exit 1
                  fi
                '') contractFlakeInputSources}
                cd "$source"
                python -m unittest -q ${lib.escapeShellArgs contractTestSelections}
                touch "$out"
              '';
          cachedQuery =
            {
              coordinate,
              description,
              argv,
              budget,
            }:
            {
              inherit description;
              context = {
                inherit coordinate;
                cacheState = "warm generated index; measured command performs no Nix evaluation";
              };
              beforeEach = [ ];
              measure = [
                {
                  inherit argv;
                  cwd = ".";
                  environment = { };
                  inheritEnvironment = false;
                  timeoutMilliseconds = 5000;
                  expectedExitCodes = [ 0 ];
                }
              ];
              afterEach = [ ];
              warmupSamples = 1;
              measuredSamples = 7;
              gates = {
                p50Milliseconds = budget;
                p95Milliseconds = budget;
              };
            };
          # These are the historical unchanged-build SLOs for every selected
          # native package that can be exercised without FastDyn's QEMU build.
          # Keeping the map here makes the Nix-emitted BenchmarkPlan the only
          # active authority; no package-local benchmark boilerplate is needed.
          nativeWarmBudgets = {
            cerebri_cubs2 = 6000;
            cerebri_modules = 2000;
            cerebri_rdd2 = 10000;
            csyn = 2000;
            electrode_web = 4000;
            modelica_models = 2000;
            qualisys_rust_sdk = 1000;
            rumoca = 2000;
            synapse_fbs = 1000;
            synapse_ppm_bridge = 2000;
            synapse_qualisys_bridge = 2000;
            zros = 2000;
            zros_drivers = 1000;
          };
          nativeWarmCase =
            package: budget:
            {
              description =
                "Run the unchanged ${package} build through its exact Nix-generated devenv task roots.";
              context = {
                category = "native-warm-build";
                coordinate = package;
                cacheState =
                  "one recorded warmup establishes mutable native outputs; three measured samples are unchanged repeats";
                invocation =
                  "run from the generated devenv shell for direct task dispatch; bootstrap overhead is otherwise measured and expected to fail the native SLO";
              };
              beforeEach = [ ];
              measure = [
                {
                  argv = [
                    client
                    "--index"
                    index
                    "--resolution-plan"
                    resolutionPlan
                    "build"
                    package
                  ];
                  cwd = ".";
                  environment = {
                    NIXSPACE_INDEX = index;
                    NIXSPACE_RESOLUTION_PLAN = resolutionPlan;
                    NIXSPACE_WORKSPACE_ROOT = ".";
                  };
                  # DEVENV_TASK_FILE and the generated runner PATH are supplied
                  # by the selected devenv shell. The fixed plan paths above
                  # remain Nix-owned and do not depend on ambient discovery.
                  inheritEnvironment = true;
                  timeoutMilliseconds = 1800000;
                  expectedExitCodes = [ 0 ];
                }
              ];
              afterEach = [ ];
              warmupSamples = 1;
              measuredSamples = 3;
              gates = {
                p50Milliseconds = budget;
                p95Milliseconds = budget;
              };
            };
          nativeWarmCases = lib.optionalAttrs (system == "x86_64-linux") (
            lib.mapAttrs' (package: budget: {
              name = "native-warm-${package}";
              value = nativeWarmCase package budget;
            }) nativeWarmBudgets
          );
        in
        {
          checks.cognipilot-contract-tests = contractTests;

          nixspace.benchmark = {
            enable = true;
            id = "cognipilot-lightweight-client-v1";
            reference = {
              name = "cognipilot-developer-host";
              class = "local-developer";
            };
            context = {
              flakeLockSha256 = builtins.hashFile "sha256" ./flake.lock;
              # The measured Nix cases execute this exact package. Preserve
              # the outer evaluator separately instead of conflating the two.
              nixVersion = pkgs.nix.version;
              planEvaluatorNixVersion = builtins.nixVersion;
              inherit system;
              cacheState = "warm generated index";
            };
            stateRoot = ".nixspace/state/benchmarks";
            # Keep the everyday developer command bounded. Evaluator-heavy
            # cases remain part of the Nix-owned plan and run when selected
            # explicitly or with `nixspace benchmark --all`.
            defaultCases = [
              "help"
              "completion-backend"
              "package-list"
              "graph-plan"
              "launch-list"
              "launch-plan"
              "module-index-100"
              "ws-build-plan"
            ];
            cases = benchmarkFixtures.cases // nativeWarmCases // {
              help = cachedQuery {
                coordinate = "nixspace/help";
                description = "Render the cached native help tree.";
                argv = [
                  client
                  "--help"
                ];
                budget = 100;
              };
              package-list = cachedQuery {
                coordinate = "workspace/packages";
                description = "List packages from the generated index.";
                argv = [
                  client
                  "--index"
                  index
                  "package"
                  "list"
                  "--json"
                ];
                budget = 100;
              };
              launch-list = cachedQuery {
                coordinate = "workspace/launches";
                description = "List launches from the generated index.";
                argv = [
                  client
                  "--index"
                  index
                  "launch"
                  "list"
                  "--json"
                ];
                budget = 100;
              };
              completion-backend = cachedQuery {
                coordinate = "workspace/completion/package";
                description = "Complete package commands from the generated index.";
                argv = [
                  client
                  "--index"
                  index
                  "_complete"
                  "package"
                  ""
                ];
                budget = 100;
              };
              graph-plan = cachedQuery {
                coordinate = "workspace/graph";
                description = "Render the complete graph from the generated index.";
                argv = [
                  client
                  "--index"
                  index
                  "graph"
                  "--json"
                ];
                budget = 200;
              };
              launch-plan = cachedQuery {
                coordinate = "electrode_web/simulation";
                description = "Resolve a launch plan from the generated index.";
                argv = [
                  client
                  "--index"
                  index
                  "launch"
                  "plan"
                  "electrode_web/simulation"
                  "--json"
                ];
                budget = 200;
              };
              ws-build-plan = cachedQuery {
                coordinate = "workspace/ws-build-plan";
                description = "Dispatch through the setup-installed ws entry point and resolve a build plan without starting a native task.";
                argv = [
                  "./ws"
                  "build"
                  "synapse_fbs"
                  "--plan"
                  "--json"
                ];
                budget = 500;
              };
              selected-flake-eval-cold = {
                description = "Evaluate and fully serialize the selected workspace index with the Nix evaluation cache disabled.";
                context = {
                  coordinate = "workspace/selected-flake";
                  cacheState = "evaluation cache disabled; immutable flake inputs and Nix store remain warm; no derivations are realized";
                  coldBoundary = "Nix evaluator only";
                };
                beforeEach = [ ];
                measure = [
                  {
                    argv = [
                      nix
                      "--extra-experimental-features"
                      "nix-command flakes"
                      "eval"
                      "--offline"
                      "--no-eval-cache"
                      "--raw"
                      "--apply"
                      ''index: builtins.hashString "sha256" (builtins.toJSON index)''
                      "${selectedFlake}#nixspaceIndex"
                    ];
                    cwd = ".";
                    environment.PWD = selectedSource;
                    inheritEnvironment = false;
                    timeoutMilliseconds = 30000;
                    expectedExitCodes = [ 0 ];
                  }
                ];
                afterEach = [ ];
                warmupSamples = 0;
                measuredSamples = 7;
                gates = {
                  p50Milliseconds = 10000;
                  p95Milliseconds = 10000;
                };
              };
              shell-eval-editable = {
                description = "Evaluate the selected PWD-bound default devenv shell derivation repeatedly without realizing it.";
                context = {
                  coordinate = "workspace/default-shell";
                  cacheState = "repeated impure editable-root evaluation; immutable flake inputs and Nix store remain warm; no Nix evaluation-cache hit is claimed";
                  coldBoundary = "editable shell evaluation; derivation not realized";
                };
                beforeEach = [ ];
                measure = [
                  {
                    argv = [
                      nix
                      "--extra-experimental-features"
                      "nix-command flakes"
                      "eval"
                      "--impure"
                      "--offline"
                      "--raw"
                      "${selectedFlake}#devShells.${system}.default.drvPath"
                    ];
                    cwd = ".";
                    environment.PWD = selectedSource;
                    inheritEnvironment = false;
                    timeoutMilliseconds = 30000;
                    expectedExitCodes = [ 0 ];
                  }
                ];
                afterEach = [ ];
                warmupSamples = 1;
                measuredSamples = 7;
                gates = {
                  p50Milliseconds = 15000;
                  p95Milliseconds = 15000;
                };
              };
              module-index-100 = {
                description = "Evaluate and serialize a synthetic 100-project module index.";
                context = {
                  coordinate = "workspace/module-index";
                  scale = "100";
                  cacheState = "warm Nix evaluator and immutable module inputs";
                };
                beforeEach = [ ];
                measure = [
                  {
                    argv = [
                      (lib.getExe pkgs.nix)
                      "--extra-experimental-features"
                      "nix-command"
                      "eval"
                      "--offline"
                      "--raw"
                      "--file"
                      "${scale100Expression}"
                    ];
                    cwd = ".";
                    environment = { };
                    inheritEnvironment = false;
                    timeoutMilliseconds = 5000;
                    expectedExitCodes = [ 0 ];
                  }
                ];
                afterEach = [ ];
                warmupSamples = 1;
                measuredSamples = 7;
                gates = {
                  p50Milliseconds = 1000;
                  p95Milliseconds = 1000;
                };
              };
            };
          };
        };

      flake.flakeModules = {
        default = ./nix/cognipilot/flake-module.nix;
        compliance = ./nix/cognipilot/compliance-flake-module.nix;
        contract = ./nix/cognipilot/flake-module.nix;
        devenvLaunches = ./nix/cognipilot/devenv-launch-module.nix;
        devenv = ./nix/cognipilot/devenv-flake-module.nix;
        devenvTasks = ./nix/cognipilot/devenv-task-module.nix;
        devenvWorkspace = ./nix/cognipilot/devenv-workspace-module.nix;
        resolution = ./nix/cognipilot/resolution-module.nix;
        product = ./nix/cognipilot/product-flake-module.nix;
        project = ./nix/cognipilot/project-flake-module.nix;
        sourceWorkspace = ./nix/cognipilot/source-workspace-module.nix;
        workspacePolicy = ./nix/cognipilot/workspace-policy-module.nix;
        westWorkspace = ./nix/nixspace/west-workspace-module.nix;
        host = ./nix/nixspace/host-module.nix;
        benchmark = ./nix/nixspace/benchmark-module.nix;
        nixspace = ./nix/nixspace/index-module.nix;
        nixspaceTool = ./nix/nixspace/tool-module.nix;
        nixspaceCogniPilot = ./nix/cognipilot/nixspace-module.nix;
        projectRoot = {
          imports = [
            ./nix/cognipilot/flake-module.nix
            ./nix/cognipilot/project-flake-module.nix
            ./nix/cognipilot/resolution-module.nix
          ];
        };
        productRoot = {
          imports = [
            ./nix/cognipilot/devenv-flake-module.nix
            ./nix/cognipilot/flake-module.nix
            ./nix/cognipilot/product-flake-module.nix
            ./nix/cognipilot/compliance-flake-module.nix
            ./nix/cognipilot/workspace-policy-module.nix
            ./nix/cognipilot/devenv-task-module.nix
            ./nix/cognipilot/devenv-launch-module.nix
            ./nix/cognipilot/devenv-workspace-module.nix
            ./nix/cognipilot/resolution-module.nix
            ./nix/cognipilot/source-workspace-module.nix
            ./nix/nixspace/west-workspace-module.nix
            ./nix/nixspace/benchmark-module.nix
            ./nix/cognipilot/nixspace-module.nix
          ];
        };
      };
    };
}
