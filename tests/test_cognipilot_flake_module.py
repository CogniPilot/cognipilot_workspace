import json
import subprocess
import unittest
from pathlib import Path


class CognipilotFlakeModuleTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.root = Path(__file__).resolve().parents[1]
        cls.module = cls.root / "nix" / "cognipilot" / "flake-module.nix"
        cls.fixtures = cls.root / "tests" / "fixtures" / "project-flakes"

    def evaluate(self, fixture: Path) -> subprocess.CompletedProcess[str]:
        expression = f"""
          let
            pkgs = import <nixpkgs> {{ }};
            evaluated = pkgs.lib.evalModules {{
              modules = [ {fixture} ];
            }};
          in evaluated.config.cognipilot.validatedIndex
        """
        return subprocess.run(
            ["nix", "eval", "--impure", "--json", "--expr", expression],
            check=False,
            capture_output=True,
            text=True,
        )

    def evaluated_json(self, fixture: Path) -> dict[str, object]:
        result = self.evaluate(fixture)
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def test_each_shared_preset_has_a_minimal_zero_boilerplate_fixture(self) -> None:
        expected_actions = {
            "cargo": {"build", "test"},
            "cargo-npm": {
                "npm-install",
                "cargo-build",
                "npm-build",
                "npm-test",
                "cargo-test",
            },
            "rumoca": {
                "compiler-build",
                "python-build",
                "javascript-build",
                "test",
            },
            "npm": {"build", "test"},
            "cmake": {"configure", "build", "test"},
            "west": {"build"},
            "zephyr-native-sim": {"build", "test"},
            "twister": {"build", "test"},
        }

        for preset, actions in expected_actions.items():
            with self.subTest(preset=preset):
                fixture = self.fixtures / "golden" / f"{preset}.nix"
                index = self.evaluated_json(fixture)
                self.assertEqual("nixspace/v1", index["apiVersion"])
                self.assertEqual("Workspace", index["kind"])
                project = index["projects"]["example"]
                self.assertEqual(set(project["targets"]["default"]["actions"]), actions)
                self.assertEqual(project["compliance"]["bespokeAdapterCount"], 0)
                fixture_text = fixture.read_text(encoding="utf-8")
                self.assertEqual(fixture_text.count("imports ="), 1)
                for forbidden in ("customActions", "cache", "state", "task", "command"):
                    self.assertNotIn(forbidden, fixture_text)

    def test_action_plans_are_nix_precomputed_devenv_task_roots(self) -> None:
        index = self.evaluated_json(self.fixtures / "golden" / "semantic-dag.nix")
        plans = index["actionPlans"]

        self.assertEqual(1, plans["schemaVersion"])
        self.assertEqual(
            {
                "kind": "devenv-task",
                "direct": {
                    "argv": ["devenv-flake-tasks", "run"],
                    "requiredEnvironment": [
                        "DEVENV_TASK_FILE",
                        "NIXSPACE_INDEX",
                        "NIXSPACE_WORKSPACE_ROOT",
                    ],
                },
                "bootstrap": {
                    "argv": [
                        "nix",
                        "develop",
                        "--no-pure-eval",
                        ".#default",
                        "--command",
                        "devenv-flake-tasks",
                        "run",
                    ]
                },
            },
            plans["runner"],
        )
        self.assertEqual(
            ["codegen:default:build", "codegen:schema:build"],
            plans["actions"]["build"]["packages"]["codegen"],
        )
        self.assertEqual(
            ["flight:default:build", "flight:firmware:build"],
            plans["actions"]["build"]["packages"]["flight"],
        )
        self.assertEqual(
            ["codegen:default:test", "codegen:schema:test"],
            plans["actions"]["test"]["packages"]["codegen"],
        )
        self.assertEqual([], plans["actions"]["test"]["packages"]["flight"])

    def test_external_and_in_tree_authorities_normalize_identically(self) -> None:
        in_tree = self.evaluated_json(self.fixtures / "golden" / "cargo.nix")
        external = self.evaluated_json(self.fixtures / "golden" / "external-cargo.nix")
        self.assertEqual(in_tree, external)
        self.assertEqual(
            in_tree["projects"]["example"]["source"]["visibility"], "private"
        )
        self.assertEqual(in_tree["projects"]["example"]["repositoryId"], "example")
        self.assertEqual(
            in_tree["projects"]["example"]["source"]["input"], "example_source"
        )

    def test_external_definition_uses_the_conventional_input_default(self) -> None:
        fixture = self.fixtures / "golden" / "external-cargo.nix"
        expression = f"""
          let
            pkgs = import <nixpkgs> {{ }};
            evaluated = pkgs.lib.evalModules {{ modules = [ {fixture} ]; }};
          in evaluated.config.cognipilot.projects.example.definition
        """
        result = subprocess.run(
            ["nix", "eval", "--impure", "--json", "--expr", expression],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            json.loads(result.stdout),
            {"origin": "external", "input": "example_definition"},
        )

    def test_custom_action_is_counted_as_a_bespoke_adapter(self) -> None:
        expression = f"""
          let
            pkgs = import <nixpkgs> {{ }};
            evaluated = pkgs.lib.evalModules {{
              modules = [
                {self.module}
                {{
                  cognipilot.projects.example = {{
                    repositoryId = "example";
                    source.input = "example-source";
                    preset = "cargo-v1";
                    customActions.package = {{
                      kind = "other";
                      argv = [ "package-project" ];
                      dependsOn = [ "build" ];
                    }};
                  }};
                }}
              ];
            }};
          in evaluated.config.cognipilot.validatedIndex
        """
        result = subprocess.run(
            ["nix", "eval", "--impure", "--json", "--expr", expression],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        index = json.loads(result.stdout)
        self.assertEqual(index["compliance"]["bespokeAdapterCount"], 1)
        self.assertEqual(
            index["projects"]["example"]["targets"]["default"]["actions"]["package"][
                "adapter"
            ],
            "bespoke-v1",
        )

    def test_targets_variants_and_artifact_contracts_normalize(self) -> None:
        index = self.evaluated_json(self.fixtures / "golden" / "semantic-dag.nix")

        codegen = index["projects"]["codegen"]
        flight = index["projects"]["flight"]
        self.assertEqual(codegen["packageId"], "codegen")
        self.assertIn("default", codegen["targets"])
        self.assertEqual(
            codegen["targets"]["schema"]["variants"]["dimensions"]["language"],
            {"default": "c", "values": ["c", "cpp"]},
        )
        self.assertEqual(
            flight["targets"]["firmware"]["artifacts"]["inputs"]["synapse"],
            {
                "consumedBy": ["build"],
                "environment": None,
                "from": "codegen:schema:headers",
                "contract": {"name": "synapse-api", "version": 1},
            },
        )
        self.assertEqual(
            codegen["targets"]["schema"]["artifacts"]["outputs"]["headers"][
                "producedBy"
            ],
            "build",
        )
        self.assertEqual(
            flight["targets"]["firmware"]["artifacts"]["outputs"]["image"][
                "producedBy"
            ],
            "build",
        )

    def test_explicit_target_variants_have_non_colliding_literal_paths(self) -> None:
        index = self.evaluated_json(
            self.fixtures / "golden" / "variant-output-paths.nix"
        )
        targets = index["projects"]["firmware"]["targets"]

        self.assertEqual(
            targets["native-sim"]["variants"]["dimensions"]["board"]["default"],
            "native-sim",
        )
        self.assertEqual(
            targets["native-sim-64"]["variants"]["dimensions"]["board"][
                "default"
            ],
            "native-sim-64",
        )
        paths = {
            targets[target]["artifacts"]["outputs"]["image"]["path"]
            for target in ("native-sim", "native-sim-64")
        }
        self.assertEqual(
            paths,
            {
                "build/native-sim/zephyr/zephyr.bin",
                "build/native-sim-64/zephyr/zephyr.bin",
            },
        )

    def test_custom_actions_have_explicit_artifact_ownership(self) -> None:
        index = self.evaluated_json(
            self.fixtures / "golden" / "explicit-artifact-actions.nix"
        )
        producer = index["projects"]["producer"]["targets"]["default"]
        consumer = index["projects"]["consumer"]["targets"]["default"]

        self.assertEqual(
            producer["artifacts"]["outputs"]["api"]["producedBy"], "build"
        )
        self.assertEqual(
            producer["artifacts"]["outputs"]["documentation"]["producedBy"],
            "docs",
        )
        self.assertEqual(
            consumer["artifacts"]["inputs"]["api"]["consumedBy"], ["build"]
        )

    def test_artifact_argv_segments_remain_typed_in_the_normalized_index(self) -> None:
        index = self.evaluated_json(
            self.fixtures / "golden" / "artifact-argv.nix"
        )
        argv = index["projects"]["consumer"]["targets"]["default"]["actions"][
            "bind"
        ]["argv"]

        self.assertEqual(
            argv,
            [
                "bind-api",
                {
                    "artifactInput": "api",
                    "prefix": "--api=",
                    "suffix": "/schema",
                },
            ],
        )

    def test_source_dependencies_are_typed_transitive_acquisition_edges(self) -> None:
        index = self.evaluated_json(
            self.fixtures / "golden" / "source-dependencies.nix"
        )

        self.assertEqual(
            ["middleware"],
            index["projects"]["application"]["source"]["dependencies"],
        )
        self.assertEqual(
            {"application", "middleware", "schema"},
            {
                node["package"]
                for node in index["graph"]["packages"]["application"]["nodes"]
                if node["type"] == "target"
            },
        )
        self.assertEqual(
            {
                ("middleware/default", "application/default", "source"),
                ("schema/default", "middleware/default", "source"),
                ("schema/schema", "middleware/default", "source"),
            },
            {
                (edge["from"], edge["to"], edge["kind"])
                for edge in index["graph"]["packages"]["application"]["edges"]
                if edge["kind"] == "source"
            },
        )

    def test_source_dependency_and_visibility_boundaries_fail_closed(self) -> None:
        cases = {
            "source-dependency-reference.nix": "unresolved source dependency `missing`",
            "public-private-source.nix": "cannot depend on private source",
            "public-private-artifact.nix": "cannot consume private artifact",
        }

        for name, diagnostic in cases.items():
            with self.subTest(fixture=name):
                result = self.evaluate(self.fixtures / "invalid" / name)
                self.assertNotEqual(0, result.returncode)
                self.assertIn(diagnostic, result.stderr)

    def test_invalid_contract_fixtures_fail_with_specific_diagnostics(self) -> None:
        cases = {
            "action-cycle.nix": "action dependency cycle involving",
            "action-dependency.nix": "unresolved action dependencies",
            "action-artifact-argv-executable.nix": (
                "must provide a non-empty literal executable as argv[0]"
            ),
            "action-artifact-argv-reference.nix": (
                "argv references unknown artifact input `missing`"
            ),
            "action-artifact-argv-consumer.nix": (
                "argv artifact input `api` must list the action in `consumedBy`"
            ),
            "artifact-contract.nix": "contract does not match",
            "artifact-cycle.nix": "artifact/action dependency cycle involves",
            "artifact-action-ambiguous.nix": "must declare exact `producedBy`",
            "artifact-action-id.nix": (
                "producing action ID `Invalid Producer` is invalid"
            ),
            "artifact-consumer-action-duplicate.nix": (
                "declares duplicate consuming actions"
            ),
            "artifact-consumer-action-reference.nix": (
                "references unknown consuming action"
            ),
            "artifact-consumption-duplicate.nix": (
                "duplicate artifact consumption declarations"
            ),
            "artifact-environment-name.nix": "environment `invalid-name` is invalid",
            "artifact-environment-collision.nix": (
                "environment `RESULT_PATH` collides with action `package`"
            ),
            "artifact-producer-action-reference.nix": (
                "references unknown producing action"
            ),
            "artifact-id.nix": "artifact output ID `Invalid Artifact` is invalid",
            "artifact-overlap.nix": "overlapping artifact output ownership",
            "artifact-reference.nix": "has unresolved reference",
            "duplicate-package.nix": "defined multiple times",
            "external-authority.nix": "external definition must name an input distinct",
            "generic-plumbing.nix": "option `cognipilot.projects.example.cache' does not exist",
            "interface-version.nix": "cognipilot.interfaceVersion",
            "path-escape.nix": "is not a safe relative path",
            "preset-replacement.nix": "replaces preset behavior",
            "project-id.nix": "project ID `Invalid Project` is invalid",
            "public-private-artifact.nix": "cannot consume private artifact",
            "public-private-source.nix": "cannot depend on private source",
            "source-dependency-reference.nix": "unresolved source dependency `missing`",
            "target-id.nix": "target ID `Invalid Target` is invalid",
            "variant-constraint.nix": "allowed variant combinations must assign exactly",
            "variant-default.nix": "default `unknown` is not an allowed value",
            "variant-value.nix": "value `native_sim;touch-pwned` is unsafe",
        }

        for name, diagnostic in cases.items():
            with self.subTest(fixture=name):
                result = self.evaluate(self.fixtures / "invalid" / name)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(diagnostic, result.stderr)

    def test_ambiguous_custom_actions_require_both_artifact_directions(self) -> None:
        result = self.evaluate(
            self.fixtures / "invalid" / "artifact-action-ambiguous.nix"
        )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must declare exact `producedBy`", result.stderr)
        self.assertIn("must declare exact `consumedBy`", result.stderr)

    def test_invalid_action_ids_are_reported_on_both_artifact_directions(self) -> None:
        result = self.evaluate(self.fixtures / "invalid" / "artifact-action-id.nix")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn(
            "producing action ID `Invalid Producer` is invalid", result.stderr
        )
        self.assertIn(
            "consuming action ID `Invalid Consumer` is invalid", result.stderr
        )


if __name__ == "__main__":
    unittest.main()
