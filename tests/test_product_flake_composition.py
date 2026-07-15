from __future__ import annotations

import json
import pathlib
import shutil
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
COMMAND_TIMEOUT_SECONDS = 45
PILOTS = ("synapse_fbs", "cerebri_cubs2", "electrode_web")
CATALOG_PROJECTS = (
    "rumoca",
    "modelica_models",
    "csyn",
    "cerebri_modules",
    "zros",
    "zros_drivers",
    "qualisys_rust_sdk",
    "synapse_qualisys_bridge",
    "synapse_ppm_bridge",
    "cerebri_rdd2",
    "csyn_ros2_bridge",
    "fastdyn",
)
ALL_PROJECTS = PILOTS + CATALOG_PROJECTS
PROOF_FILES = (
    "flake.nix",
    "flake.lock",
    "nix/cognipilot/action-presets.nix",
    "nix/cognipilot/action-tool-profiles.nix",
    "nix/cognipilot/cache-policy.nix",
    "nix/cognipilot/compliance-flake-module.nix",
    "nix/cognipilot/devenv-launch-module.nix",
    "nix/cognipilot/devenv-launch-renderer.nix",
    "nix/cognipilot/devenv-flake-module.nix",
    "nix/cognipilot/devenv-task-generator.nix",
    "nix/cognipilot/devenv-task-module.nix",
    "nix/cognipilot/devenv-workspace-module.nix",
    "nix/cognipilot/flake-module.nix",
    "nix/cognipilot/nixspace-index.nix",
    "nix/cognipilot/nixspace-module.nix",
    "nix/cognipilot/product-flake-module.nix",
    "nix/cognipilot/project-flake-module.nix",
    "nix/cognipilot/resolution-module.nix",
    "nix/cognipilot/resolution-template.nix",
    "nix/cognipilot/source-workspace-module.nix",
    "nix/cognipilot/workspace-policy-module.nix",
    "nix/nixspace/host-module.nix",
    "nix/nixspace/action-generation-layout.nix",
    "nix/nixspace/benchmark-module.nix",
    "nix/nixspace/index-module.nix",
    "nix/nixspace/tool-module.nix",
    "nix/nixspace/west-workspace-module.nix",
    "nix/spikes/cognipilot-benchmark-fixtures.nix",
    "nix/spikes/cognipilot-scale-fixture.nix",
    "tests/fixtures/sccache-pilot/Cargo.lock",
    "tests/fixtures/sccache-pilot/Cargo.toml",
    "tests/fixtures/sccache-pilot/src/lib.rs",
    "tests/fixtures/sccache-pilot/src/main.rs",
    "tools/nixspace/Cargo.lock",
    "tools/nixspace/Cargo.toml",
    "tools/nixspace/LICENSE",
    "tools/nixspace/README.md",
    "tools/nixspace/src/action.rs",
    "tools/nixspace/src/benchmark.rs",
    "tools/nixspace/src/closure_materialization.rs",
    "tools/nixspace/src/host.rs",
    "tools/nixspace/src/index.rs",
    "tools/nixspace/src/launch.rs",
    "tools/nixspace/src/launch_exec.rs",
    "tools/nixspace/src/main.rs",
    "tools/nixspace/src/model.rs",
    "tools/nixspace/src/resolution.rs",
    "tools/nixspace/src/source.rs",
    "tools/nixspace/src/west.rs",
    "tools/nixspace/tests/action.rs",
    "tools/nixspace/tests/benchmark.rs",
    "tools/nixspace/tests/closure_materialization.rs",
    "tools/nixspace/tests/host.rs",
    "tools/nixspace/tests/index.rs",
    "tools/nixspace/tests/launch_exec.rs",
    "tools/nixspace/tests/package.rs",
    "tools/nixspace/tests/query.rs",
    "tools/nixspace/tests/resolution.rs",
    "tools/nixspace/tests/source.rs",
    "tools/nixspace/tests/west.rs",
) + tuple(
    f"nix/project-definitions/{project}/{filename}"
    for project in ALL_PROJECTS
    for filename in ("flake.nix", "module.nix")
)


@unittest.skipUnless(shutil.which("nix"), "Nix is required for flake evaluation")
class ProductFlakeCompositionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.temporary = tempfile.TemporaryDirectory()
        cls.proof_root = pathlib.Path(cls.temporary.name)

        for relative in PROOF_FILES:
            source = ROOT / relative
            destination = cls.proof_root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)

        cls.run_command("git", "init", "-q")
        cls.run_command("git", "add", ".")
        cls.run_command(
            "git",
            "-c",
            "user.name=Cognipilot Test",
            "-c",
            "user.email=test@invalid",
            "commit",
            "-qm",
            "isolated product flake proof",
        )

    @classmethod
    def tearDownClass(cls) -> None:
        cls.temporary.cleanup()

    @classmethod
    def run_command(cls, *command: str) -> subprocess.CompletedProcess[str]:
        try:
            return subprocess.run(
                command,
                cwd=cls.proof_root,
                check=True,
                capture_output=True,
                text=True,
                timeout=COMMAND_TIMEOUT_SECONDS,
            )
        except subprocess.CalledProcessError as error:
            raise AssertionError(
                f"command failed: {' '.join(command)}\nstdout:\n{error.stdout}\nstderr:\n{error.stderr}"
            ) from error

    @classmethod
    def nix_eval(cls, attribute: str, *extra: str) -> object:
        result = cls.run_command(
            "nix",
            "--accept-flake-config",
            "eval",
            "--offline",
            "--json",
            f".#{attribute}",
            *extra,
        )
        return json.loads(result.stdout)

    def test_proof_is_tiny_tracked_and_offline_complete(self) -> None:
        tracked = self.run_command("git", "ls-files").stdout.splitlines()
        self.assertEqual(sorted(PROOF_FILES), tracked)
        self.assertLess(
            sum((self.proof_root / relative).stat().st_size for relative in tracked),
            2 * 1024 * 1024,
        )

        metadata = self.run_command(
            "nix",
            "--accept-flake-config",
            "flake",
            "metadata",
            "--offline",
            "--json",
            ".",
        )
        locks = json.loads(metadata.stdout)["locks"]
        self.assertEqual(7, locks["version"])
        self.assertEqual(
            {
                "nixpkgs": ["nixpkgs"],
                "source": ["synapse_ppm_bridge_source"],
            },
            locks["nodes"]["synapse_ppm_bridge_definition"]["inputs"],
        )

    def test_product_selects_the_complete_typed_registry_catalog(self) -> None:
        index = self.nix_eval("cognipilotIndex")
        generic = self.nix_eval("nixspaceIndex")

        self.assertEqual("nixspace/v1", index["apiVersion"])
        self.assertEqual("Workspace", index["kind"])
        self.assertEqual(1, index["interfaceVersion"])
        self.assertEqual(2, index["compliance"]["bespokeAdapterCount"])
        self.assertEqual(1, index["compliance"]["warningCount"])
        self.assertEqual(set(ALL_PROJECTS), set(index["projects"]))
        self.assertEqual(2, generic["interfaceVersion"])
        self.assertEqual(
            2,
            self.nix_eval(
                "packages.x86_64-linux.nixspace.normalizedInterfaceVersion"
            ),
        )
        self.assertEqual(
            {project["packageId"] for project in index["projects"].values()},
            {package["id"] for package in generic["catalog"]["packages"]},
        )
        self.assertTrue(
            all(
                set(package) == {"id", "aliases", "extensions"}
                and "org.cognipilot/package-v1" in package["extensions"]
                for package in generic["catalog"]["packages"]
            )
        )
        for project_id, project in index["projects"].items():
            if project_id == "fastdyn":
                self.assertEqual("local-only", project["deployability"])
                self.assertEqual(["missing-license"], project["compliance"]["warnings"])
                self.assertEqual(2, project["compliance"]["bespokeAdapterCount"])
            elif project_id == "synapse_ppm_bridge":
                self.assertEqual("deployable", project["deployability"])
                self.assertEqual([], project["compliance"]["warnings"])
                self.assertEqual(
                    {
                        "provider": "synapse_ppm_bridge_definition",
                        "package": "default",
                    },
                    project["targets"]["default"]["release"],
                )
            else:
                self.assertEqual("qualification", project["deployability"])
                self.assertEqual([], project["compliance"]["warnings"])

    def test_scale_fixtures_fully_serialize_at_required_sizes(self) -> None:
        expression = """
          let
            root = builtins.getFlake (toString ./.);
            fixture = import ./nix/spikes/cognipilot-scale-fixture.nix;
            evaluate = count:
              let
                index = fixture {
                  inherit count;
                  lib = root.inputs.nixpkgs.lib;
                  module = ./nix/cognipilot/flake-module.nix;
                };
                serialized = builtins.toJSON index;
              in {
                projectCount = builtins.length (builtins.attrNames index.projects);
                digest = builtins.hashString "sha256" serialized;
              };
          in builtins.listToAttrs (
            map (count: {
              name = toString count;
              value = evaluate count;
            }) [ 1 15 50 100 200 ]
          )
        """
        result = self.run_command(
            "nix",
            "eval",
            "--impure",
            "--offline",
            "--json",
            "--expr",
            expression,
        )
        scales = json.loads(result.stdout)
        self.assertEqual({"1", "15", "50", "100", "200"}, set(scales))
        for count, evidence in scales.items():
            self.assertEqual(int(count), evidence["projectCount"])
            self.assertRegex(evidence["digest"], r"^[0-9a-f]{64}$")

    def test_all_sources_and_external_definitions_are_pinned(self) -> None:
        flake_lock = json.loads((self.proof_root / "flake.lock").read_text())
        root_flake = (self.proof_root / "flake.nix").read_text()

        for project in ALL_PROJECTS:
            source = flake_lock["nodes"][f"{project}_source"]
            definition = flake_lock["nodes"][f"{project}_definition"]
            self.assertFalse(source.get("flake", True))
            self.assertIn(source["locked"]["rev"], root_flake)
            self.assertEqual(
                f"./nix/project-definitions/{project}",
                definition["locked"]["path"],
            )

        ros_source = flake_lock["nodes"]["csyn_ros2_bridge_source"]
        self.assertEqual("git", ros_source["locked"]["type"])
        self.assertTrue(ros_source["locked"]["submodules"])
        self.assertTrue(ros_source["original"]["submodules"])
        self.assertEqual(
            "https://github.com/CogniPilot/csyn_ros2_bridge.git",
            ros_source["locked"]["url"],
        )

    def test_each_definition_exports_a_standalone_default_module(self) -> None:
        for project in ALL_PROJECTS:
            result = self.run_command(
                "nix",
                "eval",
                "--offline",
                "--json",
                f"path:./nix/project-definitions/{project}#flakeModules.default.cognipilot.projects",
                "--apply",
                "builtins.attrNames",
            )
            self.assertEqual([project], json.loads(result.stdout))

    def test_pilot_targets_variants_and_artifact_edges_compose(self) -> None:
        projects = self.nix_eval("cognipilotIndex")["projects"]
        self.assertTrue(set(PILOTS).issubset(projects))

        synapse = projects["synapse_fbs"]["targets"]["default"]
        self.assertEqual(
            {"c", "javascript", "python", "rust"},
            set(synapse["artifacts"]["outputs"]),
        )

        cubs2 = projects["cerebri_cubs2"]["targets"]["default"]
        self.assertEqual(
            {
                "default": "native_sim/native/64",
                "values": ["native_sim/native/64"],
            },
            cubs2["variants"]["dimensions"]["board"],
        )
        self.assertEqual(
            "synapse_fbs:default:c",
            cubs2["artifacts"]["inputs"]["synapse-c"]["from"],
        )
        self.assertEqual(
            "rumoca:default:python",
            cubs2["artifacts"]["inputs"]["rumoca-python"]["from"],
        )

        electrode = projects["electrode_web"]["targets"]["default"]
        self.assertEqual(
            {
                "rumoca:default:javascript",
                "synapse_fbs:default:javascript",
                "synapse_fbs:default:rust",
            },
            {
                artifact["from"]
                for artifact in electrode["artifacts"]["inputs"].values()
            },
        )
        self.assertEqual(
            {"fake-sim", "ground-station", "web-index"},
            set(electrode["artifacts"]["outputs"]),
        )
        for pilot in PILOTS:
            self.assertEqual("public", projects[pilot]["source"]["visibility"])
            self.assertEqual(projects[pilot]["deployability"], "qualification")
            self.assertTrue(
                all(
                    target["release"] is None
                    for target in projects[pilot]["targets"].values()
                )
            )

    def test_electrode_ground_station_is_declarative_launch_ir(self) -> None:
        project = self.nix_eval("cognipilotIndex")["projects"]["electrode_web"]
        launch = project["launches"]["ground-station"]

        self.assertEqual(
            [
                "electrode_web:default:ground-station",
                "electrode_web:default:web-index",
            ],
            launch["requiredArtifacts"],
        )
        process = launch["processes"]["ground-station"]
        self.assertEqual("electrode_web:ground-station", process["executable"])
        self.assertEqual("endpoint", process["readiness"]["kind"])
        self.assertEqual("health", process["readiness"]["endpoint"])
        self.assertEqual("--addr", process["argv"][0]["literal"])
        self.assertEqual("127.0.0.1:", process["argv"][1]["prefix"])
        self.assertEqual(
            "telemetry-endpoint",
            process["environment"]["ELECTRODE_GCS_TELEMETRY_ZENOH_CONNECT"][
                "parameter"
            ],
        )

    def test_mocap_is_an_explicit_simulation_stack_variant(self) -> None:
        projects = self.nix_eval("cognipilotIndex")["projects"]
        mocap = projects["synapse_qualisys_bridge"]["launches"]["mocap"]
        self.assertEqual(
            ["synapse_qualisys_bridge:default:bridge"],
            mocap["requiredArtifacts"],
        )
        self.assertEqual(
            "endpoint", mocap["processes"]["mocap"]["readiness"]["kind"]
        )

        stack = projects["electrode_web"]["launches"]["simulation-stack"]
        self.assertEqual(
            {"ground-station", "simulation"}, set(stack["processes"])
        )
        self.assertEqual(
            {"ground-station": "ready"},
            stack["processes"]["simulation"]["dependencies"],
        )
        self.assertNotIn(
            "synapse_qualisys_bridge:default:bridge", stack["requiredArtifacts"]
        )
        self.assertNotIn("mocap", stack["capabilities"]["provides"])
        self.assertNotIn("mocap-health-port", stack["parameters"])
        self.assertEqual(
            "automatic",
            stack["parameters"]["ground-station-health-port"]["allocation"],
        )
        self.assertEqual(
            "127.0.0.1:",
            stack["processes"]["ground-station"]["argv"][1]["prefix"],
        )

        stack_with_mocap = projects["electrode_web"]["launches"][
            "simulation-stack-mocap"
        ]
        self.assertEqual(
            {"ground-station", "simulation", "mocap"},
            set(stack_with_mocap["processes"]),
        )
        self.assertEqual(
            {"ground-station": "ready"},
            stack_with_mocap["processes"]["mocap"]["dependencies"],
        )
        self.assertEqual(
            "synapse_qualisys_bridge:bridge",
            stack_with_mocap["processes"]["mocap"]["executable"],
        )
        self.assertIn(
            "synapse_qualisys_bridge:default:bridge",
            stack_with_mocap["requiredArtifacts"],
        )
        self.assertIn("mocap", stack_with_mocap["capabilities"]["provides"])
        self.assertEqual(
            "automatic",
            stack_with_mocap["parameters"]["mocap-health-port"]["allocation"],
        )
        self.assertEqual(
            "127.0.0.1:",
            stack_with_mocap["processes"]["mocap"]["argv"][-1]["prefix"],
        )
        self.assertIn(
            "simulation-stack-mocap",
            stack_with_mocap["capabilities"]["provides"],
        )
        self.assertNotIn(
            "ELECTRODE_GCS_TELEMETRY_ZENOH_CONNECT",
            stack_with_mocap["processes"]["ground-station"]["environment"],
        )

    def test_source_workspace_package_plans_include_source_only_dependencies(self) -> None:
        plan = self.nix_eval("nixspaceSourcePlan")

        self.assertTrue(
            {
                "cerebri_modules",
                "cerebri_rdd2",
                "csyn",
                "rumoca",
                "synapse_fbs",
            }.issubset(plan["plans"]["packages"]["cerebri_rdd2"])
        )
        self.assertTrue(
            {"synapse_fbs", "zros", "zros_drivers"}.issubset(
                plan["plans"]["packages"]["zros_drivers"]
            )
        )
        self.assertTrue(
            {"qualisys_rust_sdk", "synapse_qualisys_bridge"}.issubset(
                plan["plans"]["packages"]["synapse_qualisys_bridge"]
            )
        )
        self.assertTrue(
            {"csyn", "csyn_ros2_bridge"}.issubset(
                plan["plans"]["packages"]["csyn_ros2_bridge"]
            )
        )

    def test_catalog_presets_delegate_to_exact_native_interfaces(self) -> None:
        projects = self.nix_eval("cognipilotIndex")["projects"]
        expected_presets = {
            "rumoca": "rumoca-v1",
            "modelica_models": "nix-flake-app-v1",
            "csyn": "cargo-rust-manifest-v1",
            "cerebri_modules": "twister-v1",
            "zros": "zros-v1",
            "zros_drivers": "resource-only-v1",
            "qualisys_rust_sdk": "nix-flake-check-v1",
            "synapse_qualisys_bridge": "synapse-qualisys-v1",
            "synapse_ppm_bridge": "cargo-locked-v1",
            "cerebri_rdd2": "rdd2-v1",
            "csyn_ros2_bridge": "colcon-v1",
            "fastdyn": "resource-only-v1",
        }
        self.assertEqual(
            expected_presets,
            {project: projects[project]["preset"] for project in CATALOG_PROJECTS},
        )
        ppm_actions = projects["synapse_ppm_bridge"]["targets"]["default"]["actions"]
        self.assertEqual(
            {"rust-libudev-sccache-v1"},
            {action["toolProfile"] for action in ppm_actions.values()},
        )

        modelica = projects["modelica_models"]["targets"]["default"]
        self.assertEqual(
            "rumoca:default:compiler",
            modelica["artifacts"]["inputs"]["rumoca-compiler"]["from"],
        )
        self.assertEqual(
            ["nix", "run", "--no-pure-eval", ".#default"],
            modelica["actions"]["test"]["argv"],
        )

        csyn = projects["csyn"]["targets"]["default"]
        self.assertEqual(
            ["nix", "develop", "--no-pure-eval", ".", "-c", "cargo"],
            csyn["actions"]["build"]["argv"][:6],
        )
        self.assertEqual(
            [
                "nixspace",
                "west",
                "run",
                "--cwd",
                ".",
                "--",
                "nix",
                "develop",
                "--no-pure-eval",
                "./manifest#default",
                "-c",
                "west",
                "twister",
            ],
            csyn["actions"]["qualification"]["argv"][:13],
        )

        modules = projects["cerebri_modules"]["targets"]["default"]
        self.assertEqual(
            [
                "nixspace",
                "west",
                "run",
                "--cwd",
                ".",
                "--",
                "nix",
                "develop",
                "--no-pure-eval",
                "./manifest#default",
                "-c",
                "west",
                "twister",
                "-T",
                "modules/lib/cerebri_lockstep/tests",
                "-p",
                "native_sim/native/64",
                "--force-platform",
                "--outdir",
                "modules/lib/cerebri_lockstep/build/twister/build",
                "--no-clean",
                "--build-only",
            ],
            modules["actions"]["build"]["argv"],
        )
        self.assertEqual(
            "",
            modules["actions"]["build"]["environment"][
                "NIX_HARDENING_ENABLE"
            ],
        )

        zros = projects["zros"]["targets"]["default"]
        self.assertEqual(
            [
                "nixspace",
                "west",
                "run",
                "--cwd",
                "modules/lib/zros",
                "--",
                "nix",
                "develop",
                "--no-pure-eval",
                "../../../manifest#default",
                "-c",
                "python3",
                "scripts/format.py",
                "--check",
            ],
            zros["actions"]["format"]["argv"],
        )
        self.assertEqual(
            {}, projects["zros_drivers"]["targets"]["default"]["actions"]
        )

        qualisys_bridge = projects["synapse_qualisys_bridge"]["targets"]["default"]
        self.assertEqual(
            "qualisys_rust_sdk:default:simulator",
            qualisys_bridge["artifacts"]["inputs"]["qualisys-simulator"]["from"],
        )
        self.assertEqual(
            "QUALISYS_SIM_BIN",
            qualisys_bridge["artifacts"]["inputs"]["qualisys-simulator"][
                "environment"
            ],
        )
        self.assertEqual(
            "target/debug/synapse-qualisys-bridge",
            qualisys_bridge["actions"]["qualification"]["environment"]["BRIDGE_BIN"],
        )
        self.assertEqual(
            ["nix", "develop", "--no-pure-eval", ".", "-c", "playwright"],
            qualisys_bridge["actions"]["qualification"]["argv"][:6],
        )

        ros = projects["csyn_ros2_bridge"]["targets"]["default"]
        self.assertEqual(
            ["nix", "run", "--no-pure-eval", ".#ci"],
            ros["actions"]["test"]["argv"],
        )
        fastdyn = projects["fastdyn"]["targets"]["default"]
        self.assertEqual("fastdyn-qemu-v1", fastdyn["actions"]["build"]["toolProfile"])
        self.assertEqual(
            ["./setup.sh", "--python", "python3", "--venv", "build/venv"],
            fastdyn["actions"]["build"]["argv"][:5],
        )
        self.assertEqual(
            "build/libfastdyn.so",
            fastdyn["artifacts"]["outputs"]["plugin"]["path"],
        )

    def test_pilot_index_preserves_native_artifact_and_west_semantics(self) -> None:
        projects = self.nix_eval("cognipilotIndex")["projects"]

        synapse_paths = {
            artifact["path"]
            for artifact in projects["synapse_fbs"]["targets"]["default"]["artifacts"][
                "outputs"
            ].values()
        }
        self.assertEqual(
            {
                "target/xtask/artifacts-work/synapse_fbs-c",
                "target/xtask/packages/js",
                "target/xtask/packages/python",
                "target/xtask/packages/rust",
            },
            synapse_paths,
        )

        electrode_paths = {
            artifact["path"]
            for artifact in projects["electrode_web"]["targets"]["default"][
                "artifacts"
            ]["outputs"].values()
        }
        self.assertEqual(
            {
                "apps/web/build/index.html",
                "target/debug/electrode-fake-sim",
                "target/debug/electrode-ground-station",
            },
            electrode_paths,
        )
        self.assertEqual("zephyr-native-sim-v1", projects["cerebri_cubs2"]["preset"])

    def test_reusable_module_and_lazy_package_surface(self) -> None:
        default_is_contract = self.nix_eval(
            "flakeModules.default",
            "--apply",
            "module: builtins.isFunction (import module)",
        )
        self.assertTrue(default_is_contract)
        self.assertTrue(
            self.nix_eval(
                "flakeModules",
                "--apply",
                "modules: modules.default == modules.contract",
            )
        )

        self.assertEqual(
            13,
            self.nix_eval(
                "flakeModules.productRoot",
                "--apply",
                "module: builtins.length module.imports",
            ),
        )
        self.assertEqual(
            3,
            self.nix_eval(
                "flakeModules.projectRoot",
                "--apply",
                "module: builtins.length module.imports",
            ),
        )

        modules = self.nix_eval(
            "flakeModules",
            "--apply",
            "builtins.attrNames",
        )
        self.assertEqual(
            [
                "benchmark",
                "compliance",
                "contract",
                "default",
                "devenv",
                "devenvLaunches",
                "devenvTasks",
                "devenvWorkspace",
                "host",
                "nixspace",
                "nixspaceCogniPilot",
                "nixspaceTool",
                "product",
                "productRoot",
                "project",
                "projectRoot",
                "resolution",
                "sourceWorkspace",
                "westWorkspace",
                "workspacePolicy",
            ],
            modules,
        )

        packages = self.nix_eval(
            "packages.x86_64-linux",
            "--apply",
            "builtins.attrNames",
        )
        self.assertTrue(
            {
                "public-cache-root",
                "nixspace",
                "nixspace-benchmark-plan",
                "nixspace-completions",
                "nixspace-host",
                "nixspace-host-plan",
                "nixspace-index",
                "nixspace-launch-plan",
                "nixspace-resolution-plan",
                "nixspace-source-plan",
                "nixspace-west-plan",
                "promotion-attestation",
                "promotion-record",
                "promotion-sbom",
                "product-cognipilot-development",
                "public-workspace",
                "sccache-tools",
                "target-synapse_ppm_bridge--default",
                "workspace",
                "ws",
            }.issubset(packages)
        )
        self.assertNotIn("default", packages)
        for system in ("x86_64-linux", "aarch64-linux", "aarch64-darwin"):
            provider = self.run_command(
                "nix",
                "eval",
                "--impure",
                "--offline",
                "--raw",
                "--expr",
                f"""
                  let flake = builtins.getFlake (toString ./.);
                  in flake.inputs.synapse_ppm_bridge_definition.packages.{system}.default.drvPath
                """,
            ).stdout.strip()
            self.assertEqual(
                provider,
                self.nix_eval(
                    f"packages.{system}.target-synapse_ppm_bridge--default.drvPath"
                ),
            )
        cache_root_name = self.nix_eval(
            "packages.x86_64-linux.public-cache-root.name"
        )
        self.assertEqual(
            "cognipilot-development-public-cache-root",
            cache_root_name,
        )

    def test_contract_and_compliance_checks_compose(self) -> None:
        checks = self.nix_eval(
            "checks.x86_64-linux",
            "--apply",
            "builtins.attrNames",
        )
        self.assertEqual(
            [
                "cognipilot-compliance",
                "cognipilot-contract-tests",
                "cognipilot-promotion-attestation",
                "cognipilot-promotion-record",
                "cognipilot-promotion-sbom",
                "cognipilot-workspace-policy",
                "launch-electrode_web--ground-station-config",
                "launch-electrode_web--simulation-config",
                "launch-electrode_web--simulation-stack-config",
                "launch-electrode_web--simulation-stack-mocap-config",
                "launch-synapse_qualisys_bridge--mocap-config",
                "nixspace-interface",
                "nixspace-standalone",
                "nixspace-west-plan",
            ],
            checks,
        )

        check_name = self.nix_eval("checks.x86_64-linux.cognipilot-compliance.name")
        self.assertEqual("cognipilot-compliance", check_name)

        product_module = (ROOT / "nix/cognipilot/product-flake-module.nix").read_text()
        self.assertIn("publicLaunchCheckNames", product_module)
        self.assertNotIn("launch-electrode_web--", product_module)
        self.assertNotIn("launch-synapse_qualisys_bridge--", product_module)

    def test_supported_systems_share_the_portable_benchmark_contract(self) -> None:
        self.assertEqual(
            ["aarch64-darwin", "aarch64-linux", "x86_64-linux"],
            self.nix_eval("packages", "--apply", "builtins.attrNames"),
        )

        portable_cases = {
            "implementation-edit-cargo",
            "interface-schema-edit",
            "variant-plan-switch",
        }
        for system in ("aarch64-darwin", "aarch64-linux"):
            plan = self.nix_eval(
                f"packages.{system}.nixspace-benchmark-plan.passthru.document"
            )
            self.assertEqual(system, plan["context"]["system"])
            self.assertTrue(portable_cases.issubset(plan["cases"]))

        darwin_plan = self.nix_eval(
            "packages.aarch64-darwin.nixspace-benchmark-plan.passthru.document"
        )
        self.assertNotIn("launch-start-readiness", darwin_plan["cases"])

        linux_plan = self.nix_eval(
            "packages.aarch64-linux.nixspace-benchmark-plan.passthru.document"
        )
        self.assertIn("launch-start-readiness", linux_plan["cases"])

    def test_lightweight_benchmark_plan_is_exact_and_nix_owned(self) -> None:
        plan = self.nix_eval(
            "packages.x86_64-linux.nixspace-benchmark-plan.passthru.document"
        )
        self.assertEqual(plan["apiVersion"], "nixspace/v1")
        self.assertEqual(plan["kind"], "BenchmarkPlan")
        self.assertEqual(plan["interfaceVersion"], 3)
        self.assertEqual(plan["stateRoot"], ".nixspace/state/benchmarks")
        self.assertEqual(
            plan["defaultCases"],
            [
                "help",
                "completion-backend",
                "package-list",
                "graph-plan",
                "launch-list",
                "launch-plan",
                "module-index-100",
                "ws-build-plan",
            ],
        )
        self.assertEqual(
            sorted(plan["cases"]),
            [
                "completion-backend",
                "graph-plan",
                "help",
                "implementation-edit-cargo",
                "interface-schema-edit",
                "launch-list",
                "launch-plan",
                "launch-start-readiness",
                "module-index-100",
                "native-warm-cerebri_cubs2",
                "native-warm-cerebri_modules",
                "native-warm-cerebri_rdd2",
                "native-warm-csyn",
                "native-warm-electrode_web",
                "native-warm-modelica_models",
                "native-warm-qualisys_rust_sdk",
                "native-warm-rumoca",
                "native-warm-synapse_fbs",
                "native-warm-synapse_ppm_bridge",
                "native-warm-synapse_qualisys_bridge",
                "native-warm-zros",
                "native-warm-zros_drivers",
                "package-list",
                "selected-flake-eval-cold",
                "shell-eval-editable",
                "variant-plan-switch",
                "ws-build-plan",
            ],
        )
        self.assertEqual(plan["context"]["system"], "x86_64-linux")
        self.assertEqual(len(plan["context"]["flakeLockSha256"]), 64)
        self.assertRegex(plan["context"]["nixVersion"], r"^2\.")
        self.assertRegex(plan["context"]["planEvaluatorNixVersion"], r"^2\.")
        for case_id, case in plan["cases"].items():
            self.assertIn("setup", case)
            self.assertIn("beforeEach", case)
            self.assertIn("afterEach", case)
            self.assertIn("teardown", case)
            self.assertGreaterEqual(len(case["measure"]), 1)
            command = case["measure"][0]
            if case_id == "ws-build-plan":
                self.assertEqual(command["argv"][0], "./ws")
            else:
                self.assertTrue(command["argv"][0].startswith("/nix/store/"))
            self.assertEqual(
                case["warmupSamples"],
                0 if case_id == "selected-flake-eval-cold" else 1,
            )
            self.assertEqual(
                case["measuredSamples"],
                3 if case_id.startswith("native-warm-") else 7,
            )
            self.assertIn("p50Milliseconds", case["gates"])
            self.assertIn("p95Milliseconds", case["gates"])
        for case_id in (
            "help",
            "completion-backend",
            "package-list",
            "graph-plan",
            "launch-list",
            "launch-plan",
            "module-index-100",
            "selected-flake-eval-cold",
            "shell-eval-editable",
            "ws-build-plan",
        ):
            self.assertIn("coordinate", plan["cases"][case_id]["context"])
        self.assertEqual(plan["cases"]["module-index-100"]["context"]["scale"], "100")
        self.assertEqual(
            plan["cases"]["module-index-100"]["gates"]["p95Milliseconds"], 1000
        )
        cold = plan["cases"]["selected-flake-eval-cold"]
        self.assertIn("--no-eval-cache", cold["measure"][0]["argv"])
        self.assertEqual(cold["context"]["coldBoundary"], "Nix evaluator only")
        shell = plan["cases"]["shell-eval-editable"]
        shell_command = shell["measure"][0]
        self.assertIn("--impure", shell_command["argv"])
        self.assertTrue(shell_command["argv"][-1].endswith(".default.drvPath"))
        self.assertEqual(shell_command["cwd"], ".")
        self.assertTrue(shell_command["environment"]["PWD"].startswith("/nix/store/"))
        self.assertEqual(
            plan["cases"]["ws-build-plan"]["measure"][0]["argv"],
            ["./ws", "build", "synapse_fbs", "--plan", "--json"],
        )
        native = plan["cases"]["native-warm-synapse_fbs"]
        self.assertEqual(native["context"]["category"], "native-warm-build")
        self.assertEqual(native["context"]["coordinate"], "synapse_fbs")
        self.assertEqual(native["gates"]["p95Milliseconds"], 1000)
        self.assertEqual(native["measure"][0]["argv"][-2:], ["build", "synapse_fbs"])
        self.assertTrue(native["measure"][0]["inheritEnvironment"])
        self.assertEqual(native["measure"][0]["timeoutMilliseconds"], 1800000)
        implementation = plan["cases"]["implementation-edit-cargo"]
        self.assertEqual(implementation["context"]["ecosystem"], "cargo")
        self.assertEqual(len(implementation["setup"]), 1)
        self.assertEqual(len(implementation["beforeEach"]), 4)
        self.assertEqual(len(implementation["measure"]), 1)
        self.assertEqual(len(implementation["teardown"]), 1)
        schema = plan["cases"]["interface-schema-edit"]
        self.assertEqual(schema["context"]["change"], "artifact-contract-major")
        self.assertEqual(len(schema["beforeEach"]), 1)
        variant = plan["cases"]["variant-plan-switch"]
        self.assertEqual(variant["context"]["ecosystem"], "zephyr")
        self.assertEqual(len(variant["beforeEach"]), 1)
        launch_start = plan["cases"]["launch-start-readiness"]
        self.assertEqual(launch_start["context"]["supervisor"], "process-compose")
        self.assertEqual(len(launch_start["setup"]), 2)
        self.assertEqual(len(launch_start["beforeEach"]), 1)
        self.assertEqual(len(launch_start["measure"]), 2)
        self.assertEqual(len(launch_start["afterEach"]), 1)
        self.assertEqual(len(launch_start["teardown"]), 1)

    def test_benchmark_module_rejects_invalid_lifecycle_schema(self) -> None:
        def reject(case_body: str, expected_error: str) -> None:
            expression = f'''
              let
                flake = builtins.getFlake (toString ./.);
                invalid = flake.inputs.flake-parts.lib.mkFlake {{
                  inputs = flake.inputs // {{ self = flake; }};
                }} {{
                  systems = [ "x86_64-linux" ];
                  imports = [ flake.flakeModules.benchmark ];
                  perSystem = {{ ... }}: {{
                    nixspace.benchmark = {{
                      enable = true;
                      id = "invalid-benchmark-plan";
                      reference = {{
                        name = "test-host";
                        class = "test";
                      }};
                      defaultCases = [ "invalid" ];
                      cases.invalid = {{
                        description = "invalid benchmark case";
                        {case_body}
                      }};
                    }};
                  }};
                }};
              in
              invalid.packages.x86_64-linux.nixspace-benchmark-plan.passthru.document
            '''
            result = subprocess.run(
                (
                    "nix",
                    "--accept-flake-config",
                    "eval",
                    "--offline",
                    "--impure",
                    "--json",
                    "--expr",
                    expression,
                ),
                cwd=self.proof_root,
                check=False,
                capture_output=True,
                text=True,
                timeout=COMMAND_TIMEOUT_SECONDS,
            )
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn(expected_error, result.stderr)

        reject(
            'measure = [ { argv = [ "true" ]; expectedExitCodes = [ 0 0 ]; } ];',
            "valid lifecycle commands",
        )
        reject(
            'measure = [ { argv = [ "true" ]; } ]; warmupSamples = 1001;',
            "warmupSamples",
        )
        reject("measure = [ ];", "nonempty measure phase")

    def test_resolution_paths_are_normalized_for_strict_runtime_consumers(self) -> None:
        plan = self.nix_eval(
            "packages.x86_64-linux.nixspace-resolution-plan.passthru.document"
        )

        workspace_paths: list[str] = []

        def collect(value: object) -> None:
            if isinstance(value, dict):
                for key, child in value.items():
                    if key == "workspacePath":
                        self.assertIsInstance(child, str)
                        workspace_paths.append(child)
                    collect(child)
            elif isinstance(value, list):
                for child in value:
                    collect(child)

        collect(plan)
        self.assertTrue(workspace_paths)
        self.assertFalse(
            [path for path in workspace_paths if path.startswith("./")],
            "Nix must normalize portable paths rather than relying on a Rust fallback",
        )
        self.assertEqual(plan["roots"]["workspace"], ".")

    def test_public_cache_root_references_only_public_locked_inputs(self) -> None:
        record = self.nix_eval(
            "packages.x86_64-linux.promotion-record.normalizedRecord"
        )
        self.assertTrue(record["packages"])
        self.assertTrue(
            all(
                package["source"]["visibility"] == "public"
                for package in record["packages"]
            )
        )
        self.assertNotIn("fastdyn", {package["packageId"] for package in record["packages"]})
        public_inputs = {
            identity["storePath"]
            for package in record["packages"]
            if package["source"]["visibility"] == "public"
            for identity in (package["source"]["identity"], package["definition"]["identity"])
        }
        public_inputs.add(record["product"]["sourceIdentity"]["storePath"])
        private_input_names = self.nix_eval(
            "cognipilotIndex",
            "--apply",
            """
              index: builtins.concatLists (builtins.map
                (project:
                  if project.source.visibility != "private" then [] else
                  [ project.source.input ])
                (builtins.attrValues index.projects))
            """,
        )
        self.assertTrue(private_input_names)
        private_input_name_list = " ".join(
            json.dumps(name) for name in private_input_names
        )
        private_inputs = set(
            json.loads(
                self.run_command(
                    "nix",
                    "eval",
                    "--impure",
                    "--offline",
                    "--json",
                    "--expr",
                    f"""
                      let flake = builtins.getFlake (toString ./.);
                      in map (name: toString flake.inputs.${{name}}.outPath)
                        [ {private_input_name_list} ]
                    """,
                ).stdout
            )
        )
        self.assertTrue(private_inputs)
        private_only_inputs = private_inputs - public_inputs
        self.assertTrue(private_only_inputs)

        drv_path = self.nix_eval(
            "packages.x86_64-linux.public-cache-root.drvPath",
            "--impure",
        )
        derivations = json.loads(
            self.run_command("nix", "derivation", "show", drv_path).stdout
        )
        if derivations.get("version") == 4:
            derivation = next(iter(derivations["derivations"].values()))
            actual_inputs = {
                f"/nix/store/{source}" for source in derivation["inputs"]["srcs"]
            }
        else:
            derivation = next(iter(derivations.values()))
            actual_inputs = set(derivation["inputSrcs"])
        self.assertTrue(public_inputs.issubset(actual_inputs))
        self.assertTrue(private_only_inputs.isdisjoint(actual_inputs))
        declared_derivation_graph = set(
            self.run_command(
                "nix-store", "--query", "--requisites", drv_path
            ).stdout.splitlines()
        )
        self.assertTrue(
            private_only_inputs.isdisjoint(declared_derivation_graph),
            "the aggregate root, selected shell, and contract checks must not retain a private source transitively",
        )

    def test_public_contract_check_declares_every_non_private_flake_source(self) -> None:
        check = "checks.x86_64-linux.cognipilot-contract-tests"
        declared_names = set(self.nix_eval(f"{check}.contractFlakeInputNames"))
        excluded_names = set(self.nix_eval(f"{check}.contractExcludedInputNames"))
        declared_sources = set(self.nix_eval(f"{check}.contractFlakeInputSources"))
        private_names = set(
            self.nix_eval(
                "cognipilotIndex",
                "--apply",
                """
                  index: builtins.concatLists (builtins.map
                    (project:
                      if project.source.visibility != "private" then [] else
                      [ project.source.input ])
                    (builtins.attrValues index.projects))
                """,
            )
        )

        self.assertTrue(private_names)
        self.assertTrue(private_names.issubset(excluded_names))
        self.assertTrue(private_names.isdisjoint(declared_names))
        self.assertTrue(declared_sources)

        drv_path = self.nix_eval(f"{check}.drvPath")
        derivations = json.loads(
            self.run_command("nix", "derivation", "show", drv_path).stdout
        )
        if derivations.get("version") == 4:
            derivation = next(iter(derivations["derivations"].values()))
            actual_sources = {
                f"/nix/store/{source}" for source in derivation["inputs"]["srcs"]
            }
        else:
            derivation = next(iter(derivations.values()))
            actual_sources = set(derivation["inputSrcs"])
        self.assertTrue(declared_sources.issubset(actual_sources))

    def test_public_cache_root_contains_first_run_tools_and_generated_plans(self) -> None:
        package_names = [
            "nixspace",
            "nixspace-benchmark-plan",
            "nixspace-completions",
            "nixspace-host",
            "nixspace-host-plan",
            "nixspace-index",
            "nixspace-launch-plan",
            "nixspace-resolution-plan",
            "nixspace-source-plan",
            "nixspace-west-plan",
            "promotion-attestation",
            "promotion-record",
            "promotion-sbom",
            "public-workspace",
            "sccache-tools",
            "workspace",
            "ws",
        ]
        package_name_list = " ".join(json.dumps(name) for name in package_names)
        package_outputs = self.nix_eval(
            "packages.x86_64-linux",
            "--apply",
            f"""
              packages: builtins.listToAttrs (map (name: {{
                inherit name;
                value = {{
                  inherit (packages.${{name}}) drvPath;
                  storePath = toString packages.${{name}};
                }};
              }}) [ {package_name_list} ])
            """,
        )
        package_drvs = {output["drvPath"] for output in package_outputs.values()}
        self.assertEqual(
            package_outputs["public-workspace"]["drvPath"],
            package_outputs["workspace"]["drvPath"],
            "the selected product currently promotes only public release outputs",
        )
        self.assertEqual(
            "0.16.0",
            self.nix_eval("packages.x86_64-linux.sccache-tools.sccacheVersion"),
            "the CI backend action must use the Nix-selected sccache protocol version",
        )
        selected_check_names = [
            "cognipilot-compliance",
            "cognipilot-contract-tests",
            "cognipilot-promotion-attestation",
            "cognipilot-promotion-record",
            "cognipilot-promotion-sbom",
            "cognipilot-workspace-policy",
            "launch-electrode_web--ground-station-config",
            "launch-electrode_web--simulation-config",
            "launch-electrode_web--simulation-stack-config",
            "launch-electrode_web--simulation-stack-mocap-config",
            "launch-synapse_qualisys_bridge--mocap-config",
            "nixspace-interface",
            "nixspace-standalone",
            "nixspace-west-plan",
        ]
        check_name_list = " ".join(json.dumps(name) for name in selected_check_names)
        selected_check_drvs = set(
            self.nix_eval(
                "checks.x86_64-linux",
                "--apply",
                f"checks: map (name: checks.${{name}}.drvPath) [ {check_name_list} ]",
            )
        )
        root_drv = self.nix_eval("packages.x86_64-linux.public-cache-root.drvPath")
        derivations = json.loads(
            self.run_command("nix", "derivation", "show", root_drv).stdout
        )
        if derivations.get("version") == 4:
            derivation = next(iter(derivations["derivations"].values()))
            actual_drvs = {
                f"/nix/store/{drv}" for drv in derivation["inputs"]["drvs"]
            }
        else:
            derivation = next(iter(derivations.values()))
            actual_drvs = set(derivation["inputDrvs"])
        self.assertTrue(package_drvs.issubset(actual_drvs))
        self.assertTrue(selected_check_drvs.issubset(actual_drvs))
        app_programs = self.nix_eval(
            "apps.x86_64-linux",
            "--apply",
            'apps: { nixspace-host = apps.nixspace-host.program; ws = apps.ws.program; }',
        )
        self.assertEqual(
            app_programs["nixspace-host"],
            f"{package_outputs['nixspace-host']['storePath']}/bin/nixspace-host",
        )
        self.assertEqual(
            app_programs["ws"],
            f"{package_outputs['ws']['storePath']}/bin/ws",
        )

    def test_synthetic_authority_inputs_are_not_selected_by_the_product(self) -> None:
        lock = json.loads((self.proof_root / "flake.lock").read_text())
        nodes = lock["nodes"]
        synthetic_inputs = {
            "external_definition",
            "external_source",
            "fork",
            "in_tree",
        }

        self.assertTrue(synthetic_inputs.isdisjoint(nodes))
        self.assertTrue(synthetic_inputs.isdisjoint(nodes["root"]["inputs"]))


if __name__ == "__main__":
    unittest.main()
