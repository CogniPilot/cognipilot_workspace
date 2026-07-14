from __future__ import annotations

import json
import pathlib
import shutil
import subprocess
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests" / "fixtures" / "compliance"


@unittest.skipUnless(shutil.which("nix"), "Nix is required for module evaluation")
class CognipilotComplianceTests(unittest.TestCase):
    def evaluate(
        self, fixture: pathlib.Path, result_expression: str
    ) -> subprocess.CompletedProcess[str]:
        expression = f"""
          let
            pkgs = import <nixpkgs> {{ }};
            evaluated = pkgs.lib.evalModules {{
              modules = [
                {{
                  options.perSystem = pkgs.lib.mkOption {{
                    type = pkgs.lib.types.raw;
                  }};
                }}
                {fixture}
              ];
            }};
          in {result_expression}
        """
        return subprocess.run(
            ["nix", "eval", "--impure", "--json", "--expr", expression],
            check=False,
            capture_output=True,
            text=True,
            timeout=30,
        )

    def evaluate_product_composition(self) -> subprocess.CompletedProcess[str]:
        expression = f"""
          let
            pkgs = import <nixpkgs> {{ }};
            lib = pkgs.lib;
            root = lib.evalModules {{
              modules = [
                {{
                  options = {{
                    perSystem = lib.mkOption {{
                      type = lib.types.deferredModule;
                      default = {{ }};
                    }};
                    flake.nixspaceIndex = lib.mkOption {{
                      type = lib.types.attrs;
                    }};
                  }};
                }}
                {FIXTURES / "valid.nix"}
                {ROOT / "nix" / "cognipilot" / "product-flake-module.nix"}
              ];
            }};
            system = lib.evalModules {{
              specialArgs = {{ inherit pkgs; }};
              modules = [
                root.config.perSystem
                {{
                  options = {{
                    packages = lib.mkOption {{
                      type = lib.types.lazyAttrsOf lib.types.package;
                      default = {{ }};
                    }};
                    checks = lib.mkOption {{
                      type = lib.types.lazyAttrsOf lib.types.package;
                      default = {{ }};
                    }};
                    devShells = lib.mkOption {{
                      type = lib.types.lazyAttrsOf lib.types.package;
                      default = {{ }};
                    }};
                    apps = lib.mkOption {{
                      type = lib.types.attrs;
                      default = {{ }};
                    }};
                    formatter = lib.mkOption {{
                      type = lib.types.nullOr lib.types.package;
                      default = null;
                    }};
                  }};
                }}
              ];
            }};
          in {{
            checks = builtins.attrNames system.config.checks;
            indexMatches =
              root.config.flake.nixspaceIndex
              == root.config.cognipilot.validatedIndex;
          }}
        """
        return subprocess.run(
            ["nix", "eval", "--impure", "--json", "--expr", expression],
            check=False,
            capture_output=True,
            text=True,
            timeout=30,
        )

    def test_strict_product_defaults_emit_a_json_safe_report(self) -> None:
        result = self.evaluate(
            FIXTURES / "valid.nix",
            "evaluated.config.cognipilot.complianceReport",
        )
        self.assertEqual(0, result.returncode, result.stderr)

        report = json.loads(result.stdout)
        self.assertEqual(1, report["schemaVersion"])
        self.assertTrue(report["compliant"])
        self.assertEqual(
            {
                "allowedDeployability": ["qualification", "deployable"],
                "allowedLifecycles": ["stable"],
                "approvedBespokeActions": [],
                "enforcedVisibilities": ["private", "public"],
                "maximumBespokeAdapters": 0,
                "requireOwner": True,
                "requireSpdxLicense": True,
            },
            report["policy"],
        )
        self.assertEqual(
            {
                "bespokeAdapterCount": 0,
                "selectedPackageCount": 1,
                "warningCount": 0,
            },
            report["summary"],
        )
        self.assertEqual(
            {"approvals": [], "findings": []},
            report["bespoke"],
        )
        self.assertEqual(
            {
                "bespokeAdapterCount": 0,
                "compliant": True,
                "deployability": "deployable",
                "enforced": True,
                "licenseSpdx": "Apache-2.0",
                "lifecycle": "stable",
                "owner": "CogniPilot Foundation",
                "packageId": "flight-control",
                "projectId": "flight-control",
            },
            report["packages"]["flight-control"],
        )

    def test_compliance_check_metadata_evaluates_without_realization(self) -> None:
        result = self.evaluate(
            FIXTURES / "valid.nix",
            "let output = evaluated.config.perSystem { inherit pkgs; }; in { name = output.checks.cognipilot-compliance.name; report = evaluated.config.cognipilot.complianceReport; }",
        )
        self.assertEqual(0, result.returncode, result.stderr)

        evaluated = json.loads(result.stdout)
        self.assertEqual("cognipilot-compliance", evaluated["name"])
        self.assertTrue(evaluated["report"]["compliant"])

    def test_product_composition_keeps_all_named_checks(self) -> None:
        result = self.evaluate_product_composition()
        self.assertEqual(0, result.returncode, result.stderr)

        evaluated = json.loads(result.stdout)
        self.assertEqual(
            [
                "cognipilot-compliance",
                "cognipilot-promotion-attestation",
                "cognipilot-promotion-record",
                "cognipilot-promotion-sbom",
                "nixspace-interface",
                "nixspace-standalone",
            ],
            evaluated["checks"],
        )
        self.assertTrue(evaluated["indexMatches"])

    def test_exact_root_approval_records_the_bespoke_action_finding(self) -> None:
        result = self.evaluate(
            FIXTURES / "approved-bespoke.nix",
            "evaluated.config.cognipilot.complianceReport",
        )
        self.assertEqual(0, result.returncode, result.stderr)

        report = json.loads(result.stdout)
        coordinate = "flight-control:default:package"
        self.assertEqual([coordinate], report["bespoke"]["approvals"])
        self.assertEqual(
            [
                {
                    "actionId": "package",
                    "approved": True,
                    "coordinate": coordinate,
                    "packageId": "flight-control",
                    "projectKey": "flight-control",
                    "targetId": "default",
                }
            ],
            report["bespoke"]["findings"],
        )

    def test_invalid_duplicate_and_stale_approvals_are_rejected(self) -> None:
        result = self.evaluate(
            FIXTURES / "invalid-approvals.nix",
            "evaluated.config.cognipilot.complianceReport",
        )
        self.assertNotEqual(0, result.returncode)
        for diagnostic in (
            "duplicate bespoke action approvals",
            "bespoke action approval `not-a-coordinate` is invalid",
            "bespoke action approval `flight-control:default:missing` is stale",
        ):
            with self.subTest(diagnostic=diagnostic):
                self.assertIn(diagnostic, result.stderr)

    def test_invalid_selection_reports_every_actionable_package_violation(self) -> None:
        result = self.evaluate(
            FIXTURES / "invalid.nix",
            "evaluated.config.cognipilot.complianceReport",
        )
        self.assertNotEqual(0, result.returncode)
        for diagnostic in (
            "package `legacy-flight-control` (project `legacy-flight`) is missing the owner",
            "package `legacy-flight-control` (project `legacy-flight`) is missing the SPDX license",
            "lifecycle `deprecated` is not allowed",
            "deployability `local-only` is not allowed",
            "bespoke action `legacy-flight-control:default:package` is not approved",
            "policy-enforced packages declare 1 bespoke adapters",
            "exceeding `cognipilot.compliancePolicy.maximumBespokeAdapters` (0)",
            "actions: legacy-flight-control:default:package",
        ):
            with self.subTest(diagnostic=diagnostic):
                self.assertIn(diagnostic, result.stderr)

    def test_policy_can_exclude_explicitly_private_non_promotable_projects(self) -> None:
        result = self.evaluate(
            FIXTURES / "private-excluded.nix",
            "evaluated.config.cognipilot.complianceReport",
        )
        self.assertEqual(0, result.returncode, result.stderr)

        report = json.loads(result.stdout)
        package = report["packages"]["legacy-flight"]
        self.assertTrue(report["compliant"])
        self.assertFalse(package["enforced"])
        self.assertEqual(["public"], report["policy"]["enforcedVisibilities"])


if __name__ == "__main__":
    unittest.main()
