import json
import subprocess
import unittest
from pathlib import Path


class CognipilotResourcesTest(unittest.TestCase):
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

    def test_action_requirements_resources_and_executables_normalize(self) -> None:
        index = self.evaluated_json(self.fixtures / "golden" / "resources-actions.nix")
        project = index["projects"]["app"]

        self.assertEqual(
            project["targets"]["default"]["actions"]["build"]["requirements"],
            {
                "cpu": 2,
                "memoryMiB": 1024,
                "exclusiveLocks": ["cargo-target", "usb-device"],
            },
        )
        self.assertEqual(
            project["resources"]["default-config"],
            {"kind": "configuration", "path": "config/default.json"},
        )
        self.assertEqual(
            project["executables"]["app"],
            {
                "from": "app:default:cli",
                "argv": ["--config", "config/default.json"],
            },
        )

    def test_minimal_preset_gets_empty_normalized_defaults(self) -> None:
        index = self.evaluated_json(self.fixtures / "golden" / "cargo.nix")
        project = index["projects"]["example"]

        self.assertEqual(project["resources"], {})
        self.assertEqual(project["executables"], {})
        for action in project["targets"]["default"]["actions"].values():
            self.assertEqual(
                action["requirements"],
                {"cpu": None, "memoryMiB": None, "exclusiveLocks": []},
            )

    def test_invalid_resource_contracts_fail_with_specific_diagnostics(self) -> None:
        cases = {
            "action-requirement-reference.nix": "requirements reference unknown action",
            "action-requirement-lock.nix": "exclusive lock ID `Invalid Lock` is invalid",
            "action-requirement-duplicate-lock.nix": "has duplicate exclusive locks",
            "resource-id.nix": "resource ID `Invalid Resource` is invalid",
            "resource-path.nix": "is not a safe relative path",
            "resource-overlap.nix": "overlapping resource ownership",
            "executable-reference.nix": "has unresolved artifact reference",
            "executable-kind.nix": "must reference an executable artifact",
            "executable-argv.nix": "argv contains an empty entry",
            "export-collision.nix": "resource and executable export IDs collide",
            "executable-duplicate.nix": "exports the same executable artifact more than once",
        }

        for name, diagnostic in cases.items():
            with self.subTest(fixture=name):
                result = self.evaluate(self.fixtures / "invalid" / name)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(diagnostic, result.stderr)


if __name__ == "__main__":
    unittest.main()
