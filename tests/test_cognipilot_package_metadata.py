import json
import subprocess
import unittest
from pathlib import Path


class CognipilotPackageMetadataTest(unittest.TestCase):
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

    def test_explicit_package_compliance_metadata_normalizes(self) -> None:
        index = self.evaluated_json(
            self.fixtures / "golden" / "compliance-metadata.nix"
        )
        project = index["projects"]["flight-integration"]

        self.assertEqual(project["packageId"], "flight-control")
        self.assertEqual(project["aliases"], ["autopilot", "fc"])
        self.assertEqual(project["lifecycle"], "stable")
        self.assertEqual(
            project["softwareVersion"],
            {"source": "file", "file": "VERSION", "value": None},
        )
        self.assertEqual(project["deployability"], "deployable")
        self.assertEqual(
            project["targets"]["default"]["release"],
            {"provider": "flight-release", "package": "flight-control"},
        )
        self.assertEqual(project["owner"], "CogniPilot Foundation")
        self.assertEqual(
            project["license"], {"spdx": "(Apache-2.0 OR MIT) AND BSD-3-Clause"}
        )
        self.assertEqual(project["compliance"]["warnings"], [])
        self.assertEqual(project["compliance"]["warningCount"], 0)
        self.assertEqual(index["compliance"]["warningCount"], 0)

    def test_minimal_preset_has_stable_defaults_and_policy_warnings(self) -> None:
        index = self.evaluated_json(self.fixtures / "golden" / "cargo.nix")
        project = index["projects"]["example"]

        self.assertEqual(project["packageId"], "example")
        self.assertEqual(project["aliases"], [])
        self.assertEqual(project["lifecycle"], "experimental")
        self.assertEqual(
            project["softwareVersion"],
            {"source": "native", "file": None, "value": None},
        )
        self.assertEqual(project["deployability"], "local-only")
        self.assertIsNone(project["owner"])
        self.assertEqual(project["license"], {"spdx": None})
        self.assertEqual(
            project["compliance"]["warnings"],
            ["missing-owner", "missing-license"],
        )
        self.assertEqual(project["compliance"]["warningCount"], 2)
        self.assertEqual(index["compliance"]["warningCount"], 2)

    def test_integration_key_supplies_conventional_source_and_repository_defaults(self) -> None:
        index = self.evaluated_json(
            self.fixtures / "golden" / "resolution-locked.nix"
        )
        project = index["projects"]["runtime"]

        self.assertEqual(project["repositoryId"], "runtime")
        self.assertEqual(project["source"]["input"], "runtime_source")

        for path in sorted((self.root / "nix/project-definitions").glob("*/module.nix")):
            if path.parent.name in {"external", "fork", "in-tree"}:
                continue
            source = path.read_text(encoding="utf-8")
            self.assertNotIn("repositoryId =", source, path)
            self.assertNotIn("source.input =", source, path)
            self.assertNotIn("definition = {", source, path)

    def test_invalid_metadata_fails_with_specific_diagnostics(self) -> None:
        cases = {
            "package-metadata-id.nix": "package ID `Invalid Package` is invalid",
            "package-metadata-alias.nix": "alias ID `Invalid Alias` is invalid",
            "package-metadata-alias-duplicate.nix": "declares duplicate aliases",
            "package-metadata-alias-self.nix": "cannot also declare its package ID as an alias",
            "package-metadata-collision.nix": "package IDs and aliases collide: shared",
            "package-metadata-version-file.nix": "is not a safe relative path",
            "package-metadata-version-literal.nix": "incoherent `literal` software-version",
            "package-metadata-version-native.nix": "incoherent `native` software-version",
            "package-metadata-owner.nix": "owner must not be blank",
            "package-metadata-license.nix": "SPDX license expression",
            "package-metadata-license-parentheses.nix": "SPDX license expression",
            "release-deployable-missing.nix": "declares no immutable target release output",
            "release-qualification.nix": "cannot declare immutable target release outputs",
            "release-provider-id.nix": "release provider `Invalid Provider` is invalid",
            "release-package-id.nix": "release package `Invalid Package` is invalid",
        }

        for name, diagnostic in cases.items():
            with self.subTest(fixture=name):
                result = self.evaluate(self.fixtures / "invalid" / name)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(diagnostic, result.stderr)


if __name__ == "__main__":
    unittest.main()
