import json
import subprocess
import unittest
from pathlib import Path


class CognipilotLaunchIrTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.root = Path(__file__).resolve().parents[1]
        cls.fixtures = cls.root / "tests" / "fixtures" / "project-flakes"

    def evaluate(self, fixture: Path) -> subprocess.CompletedProcess[str]:
        expression = f"""
          let
            pkgs = import <nixpkgs> {{ }};
            evaluated = pkgs.lib.evalModules {{ modules = [ {fixture} ]; }};
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

    def test_launch_ir_normalizes_without_supervisor_commands(self) -> None:
        index = self.evaluated_json(self.fixtures / "golden" / "launch-ir.nix")
        launches = index["projects"]["app"]["launches"]
        router = launches["router"]
        process = router["processes"]["router"]

        self.assertEqual(router["parameters"]["port"]["type"], "port")
        self.assertEqual(router["requiredArtifacts"], ["app:default:router-bin"])
        self.assertEqual(router["requiredResources"], ["app:router-config"])
        self.assertEqual(
            process["argv"][1],
            {"literal": None, "parameter": "host", "prefix": "", "suffix": ""},
        )
        self.assertEqual(process["environment"]["LOG_LEVEL"]["parameter"], "log-level")
        self.assertEqual(process["readiness"]["endpoint"], "http")
        self.assertEqual(
            process["endpoints"]["http"],
            {
                "protocol": "http",
                "hostParameter": "host",
                "portParameter": "port",
                "path": "/ready",
                "expectedStatus": 204,
            },
        )
        self.assertEqual(
            router["sessionEnvironment"]["ROUTER_STATE"],
            {"base": "session", "path": "router/state.json", "create": "parent"},
        )
        self.assertEqual(process["restart"]["policy"], "on-failure")
        self.assertEqual(process["shutdown"]["killSignal"], "SIGKILL")
        self.assertEqual(
            launches["stack"]["includes"]["base"]["parameters"]["host"],
            "router-host",
        )
        self.assertEqual(launches["stack"]["capabilities"]["requires"], ["router"])

    def test_packages_without_launches_keep_zero_boilerplate(self) -> None:
        index = self.evaluated_json(self.fixtures / "golden" / "cargo.nix")
        self.assertEqual(index["projects"]["example"]["launches"], {})

    def test_invalid_launch_ir_fails_with_specific_diagnostics(self) -> None:
        cases = {
            "launch-parameter.nix": "has an invalid default for type `port`",
            "launch-allocation.nix": "automatic parameter `host` must have type `port`",
            "launch-path-secret.nix": "path parameter `config` requires safe allowed roots",
            "launch-requirements.nix": "has unresolved required artifact",
            "launch-process-reference.nix": "has unresolved executable",
            "launch-process-cycle.nix": "process dependency cycle involves",
            "launch-endpoint.nix": "endpoint readiness has an invalid endpoint",
            "launch-restart.nix": "restart behavior with restart policy `never`",
            "launch-include-forward.nix": "does not forward required parameters",
            "launch-include-type.nix": "forwards incompatible parameter types",
            "launch-include-cycle.nix": "launch include cycle involves",
            "launch-collision-capability.nix": "process and include IDs collide",
            "launch-capability-missing.nix": (
                "requires capability `router` but selects no provider"
            ),
            "launch-capability-multiple.nix": (
                "requires capability `router` but selects multiple providers: "
                "example:first, example:second"
            ),
            "launch-secret-argv.nix": "secrets are environment-only",
            "launch-session-path.nix": "session environment `STATE_FILE` path `../escape.json` is unsafe",
        }

        for name, diagnostic in cases.items():
            with self.subTest(fixture=name):
                result = self.evaluate(self.fixtures / "invalid" / name)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(diagnostic, result.stderr)


if __name__ == "__main__":
    unittest.main()
