import json
import subprocess
import unittest
from pathlib import Path


class CognipilotResolutionTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.root = Path(__file__).resolve().parents[1]
        cls.fixtures = cls.root / "tests" / "fixtures" / "project-flakes"

    def template(self, fixture: str) -> dict[str, object]:
        path = self.fixtures / "golden" / fixture
        expression = f"""
          let
            pkgs = import <nixpkgs> {{ }};
            evaluated = pkgs.lib.evalModules {{ modules = [ {path} ]; }};
          in evaluated.config.cognipilot.validatedIndex.resolutionTemplate
        """
        result = subprocess.run(
            ["nix", "eval", "--impure", "--json", "--expr", expression],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def test_local_plan_precomputes_selection_closures_and_generation_protocol(self) -> None:
        plan = self.template("semantic-dag.nix")

        self.assertEqual(plan["apiVersion"], "nixspace/v1")
        self.assertEqual(plan["kind"], "WorkspaceResolutionTemplate")
        self.assertEqual(plan["interfaceVersion"], 1)

        flight = plan["packagePlans"]["flight"]
        self.assertEqual(flight["selectedScope"], "local")
        self.assertEqual(flight["dependencyClosure"], ["codegen", "flight"])
        self.assertEqual(
            flight["commandScopes"]["local"]["selectedCandidates"],
            {"codegen": "local", "flight": "local"},
        )
        self.assertFalse(flight["commandScopes"]["local"]["override"]["refused"])
        self.assertIn("flight", plan["packagePlans"]["codegen"]["reverseClosure"])

        headers = plan["artifacts"]["codegen:schema:headers"]
        self.assertEqual(headers["contract"], {"name": "synapse-api", "version": 1})
        self.assertIsNone(headers["candidates"]["locked"])
        self.assertEqual(
            headers["candidates"]["local"]["generation"]["producerTask"],
            "codegen:schema:build",
        )
        self.assertEqual(
            headers["candidates"]["local"]["generation"]["pointer"],
            {
                "apiVersion": "nixspace/v1",
                "kind": "ActionGenerationPointer",
                "interfaceVersion": 1,
                "file": "current",
                "identity": {
                    "kind": "action-task-input",
                    "task": "codegen:schema:build",
                },
            },
        )

    def test_locked_candidates_come_only_from_deployable_release_outputs(self) -> None:
        plan = self.template("resolution-locked.nix")

        package = plan["packages"]["runtime"]
        self.assertEqual(package["candidates"]["local"]["deployable"], False)
        self.assertEqual(
            package["candidates"]["locked"],
            {
                "kind": "nix-output-reference",
                "deployable": True,
                "installable": ".#target-runtime--default",
                "provider": "runtime_release",
                "package": "runtime",
                "relativePath": ".",
                "targetId": "default",
                "provenance": {
                    "kind": "locked-output",
                    "label": "LOCKED",
                    "provider": "runtime_release",
                    "package": "runtime",
                },
            },
        )
        self.assertEqual(
            plan["artifacts"]["runtime:default:cli"]["candidates"]["locked"][
                "relativePath"
            ],
            "bin/runtime",
        )
        self.assertEqual(
            plan["resources"]["runtime/config"]["candidates"]["locked"][
                "relativePath"
            ],
            "share/runtime/config.json",
        )
        self.assertEqual(
            plan["executables"]["runtime/runtime"]["candidates"]["locked"],
            {
                "kind": "artifact-candidate",
                "artifact": "runtime:default:cli",
                "candidate": "locked",
            },
        )
        self.assertEqual(
            plan["packagePlans"]["runtime"]["commandScopes"]["locked"][
                "selectedCandidates"
            ],
            {"runtime": "locked"},
        )

    def test_action_environment_binding_is_scoped_and_contract_typed(self) -> None:
        plan = self.template("artifact-argv.nix")
        binding = plan["actionBindings"]["consumer:default:bind"]

        self.assertEqual(binding["scope"], "action")
        self.assertEqual(
            binding["artifacts"]["api"]["contract"],
            {"name": "generated-api", "version": 1},
        )
        self.assertIsNone(binding["artifacts"]["api"]["environment"])

    def test_launch_children_join_the_root_command_selection_closure(self) -> None:
        plan = self.template("resolution-launch-closure.nix")
        app = plan["packagePlans"]["app"]

        self.assertEqual(app["dependencyClosure"], ["app", "worker"])
        self.assertEqual(
            app["commandScopes"]["local"]["selectedCandidates"],
            {"app": "local", "worker": "local"},
        )
        self.assertIn("app", plan["packagePlans"]["worker"]["reverseClosure"])


if __name__ == "__main__":
    unittest.main()
