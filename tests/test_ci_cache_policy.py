from __future__ import annotations

import json
from pathlib import Path
import re
import shutil
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = ROOT / ".github" / "workflows"
PINNED_ACTION = re.compile(r"^\s*(?:-\s+)?uses:\s+[^\s@]+@([0-9a-f]{40})(?:\s+#.*)?$")
ANY_ACTION = re.compile(r"^\s*(?:-\s+)?uses:\s+([^\s]+)")


class CiCachePolicyTests(unittest.TestCase):
    def workflow(self, name: str) -> str:
        return (WORKFLOWS / name).read_text(encoding="utf-8")

    def test_every_third_party_action_is_commit_pinned(self) -> None:
        for workflow in sorted(WORKFLOWS.glob("*.yml")):
            for line_number, line in enumerate(
                workflow.read_text(encoding="utf-8").splitlines(), start=1
            ):
                if ANY_ACTION.match(line):
                    self.assertRegex(
                        line,
                        PINNED_ACTION,
                        f"{workflow.relative_to(ROOT)}:{line_number} must use a commit SHA",
                    )

    def test_pull_requests_are_read_only_and_main_receives_the_token(self) -> None:
        workflow = self.workflow("ci.yml")

        main_guard = (
            "if: github.event_name == 'push' && github.ref == 'refs/heads/main'"
        )
        publish_start = workflow.index("- name: Publish the explicit public closure")
        publish_end = workflow.index("- name: Retain bounded main revisions")
        publish_step = workflow[publish_start:publish_end]

        self.assertEqual(workflow.count(main_guard), 4)
        self.assertIn(main_guard, publish_step)
        self.assertEqual(workflow.count("secrets.CACHIX_AUTH_TOKEN"), 2)
        self.assertEqual(workflow.count("CACHIX_AUTH_TOKEN:"), 2)
        self.assertIn(
            "CACHIX_AUTH_TOKEN: ${{ secrets.CACHIX_AUTH_TOKEN }}", publish_step
        )
        self.assertIn(
            "run: cachix push cognipilot ./result-public-cache-root", publish_step
        )
        self.assertIn("skipPush: true", workflow)
        self.assertIn("if: github.event_name == 'pull_request'", workflow)
        self.assertNotIn("matrix.publish", workflow)
        self.assertNotIn("authToken:", workflow)
        self.assertNotIn("pathsToPush:", workflow)
        self.assertNotIn("useDaemon:", workflow)
        self.assertIn("--out-link result-public-cache-root", workflow)
        self.assertLess(workflow.index("skipPush: true"), workflow.index("nix eval"))
        self.assertLess(workflow.index("skipPush: true"), workflow.index("nix build"))

    def test_overlapping_runs_cannot_regress_rolling_main_pins(self) -> None:
        workflow = self.workflow("ci.yml")

        self.assertIn("concurrency:", workflow)
        self.assertIn("group: nix-workspace-${{ github.ref }}", workflow)
        self.assertIn("cancel-in-progress: true", workflow)
        self.assertIn("pins are mutable", workflow)

    def test_native_matrix_covers_every_supported_flake_system(self) -> None:
        workflow = self.workflow("ci.yml")
        entries = (
            ("x86_64-linux", "ubuntu-24.04"),
            ("aarch64-linux", "ubuntu-24.04-arm"),
            ("aarch64-darwin", "macos-15"),
        )

        self.assertIn("runs-on: ${{ matrix.runner }}", workflow)
        self.assertIn("fail-fast: false", workflow)
        for system, runner in entries:
            entry = (
                f"- system: {system}\n"
                f"            runner: {runner}"
            )
            self.assertEqual(workflow.count(entry), 2)
        self.assertNotIn("publish:", workflow)
        self.assertNotIn("matrix.publish", workflow)
        self.assertNotIn("x86_64-darwin", workflow)
        self.assertNotIn("macos-15-intel", workflow)

    def test_matrix_evaluates_and_realizes_only_the_public_system_root(self) -> None:
        workflow = self.workflow("ci.yml")

        root = '".#packages.${{ matrix.system }}.public-cache-root'
        self.assertIn(f"{root}.drvPath\"", workflow)
        self.assertIn(f"{root}\"", workflow)
        self.assertEqual(workflow.count(root), 3)
        self.assertIn("kvm: false", workflow)
        self.assertNotIn("qemu", workflow.lower())
        self.assertNotIn("firmware", workflow.lower())

    def test_ci_realizes_explicit_cache_and_contract_roots(self) -> None:
        workflow = self.workflow("ci.yml")

        self.assertIn(".#nixspaceIndex", workflow)
        self.assertIn("packages.${{ matrix.system }}.public-cache-root", workflow)
        self.assertEqual(
            workflow.count("cachix push cognipilot ./result-public-cache-root"), 1
        )
        self.assertNotIn("nix flake archive", workflow.replace("`nix flake archive`", ""))
        self.assertNotIn("cachix watch", workflow)
        self.assertIn("nix flake check --accept-flake-config --no-build", workflow)
        self.assertNotIn("run: |", workflow)

    def test_blocking_public_root_upload_precedes_coverage_reporting(self) -> None:
        workflow = self.workflow("ci.yml")
        action_lines = [line.strip() for line in workflow.splitlines() if ANY_ACTION.match(line)]

        self.assertEqual(2, sum("cachix/cachix-action@" in line for line in action_lines))
        self.assertEqual(3, sum("actions/upload-artifact@" in line for line in action_lines))
        self.assertEqual(workflow.count("cachix push"), 1)
        self.assertNotIn("pathsToPush:", workflow)
        self.assertNotIn("skipPush: false", workflow)
        self.assertLess(
            workflow.index("run: cachix push cognipilot ./result-public-cache-root"),
            workflow.index("cachix pin cognipilot"),
        )
        self.assertLess(
            workflow.index("cachix pin cognipilot"),
            workflow.index("Verify main cache coverage"),
        )

    def test_publication_archives_only_the_visibility_filtered_root(self) -> None:
        workflow = self.workflow("ci.yml")

        self.assertIn(
            "public flake source and definition inputs", workflow
        )
        self.assertIn("separately locked private FastDyn source", workflow)
        publish_start = workflow.index("- name: Publish the explicit public closure")
        publish_end = workflow.index("- name: Retain bounded main revisions")
        self.assertEqual(workflow[publish_start:publish_end].count("./result-public-cache-root"), 1)
        self.assertNotIn("nix flake archive --json", workflow)
        self.assertNotIn("nix path-info --all", workflow)
        self.assertNotIn("nix-store --query --requisites", workflow)

    def test_ci_retains_structured_cache_coverage_without_claiming_transfers(self) -> None:
        workflow = self.workflow("ci.yml")

        self.assertEqual(workflow.count("cache coverage --json"), 3)
        self.assertEqual(workflow.count("--require-complete"), 2)
        self.assertIn("if: github.event_name == 'pull_request'", workflow)
        self.assertIn("--output cache-coverage.json", workflow)
        self.assertIn('--summary "$GITHUB_STEP_SUMMARY"', workflow)
        self.assertIn("path: cache-coverage.json", workflow)
        self.assertIn("retention-days: 30", workflow)
        self.assertIn("not bytes transferred by the preceding build", workflow)
        self.assertNotIn("transferred bytes", workflow.lower())
        self.assertNotIn("bytes uploaded", workflow.lower())
        self.assertNotIn("CACHIX_AUTH_TOKEN }} cache coverage", workflow)

    def test_main_roots_have_bounded_cachix_retention(self) -> None:
        workflow = self.workflow("ci.yml")
        pin_start = workflow.index("- name: Retain bounded main revisions")
        pin_end = workflow.index("- name: Report pull-request cache coverage")
        pin_step = workflow[pin_start:pin_end]

        self.assertIn(
            "if: github.event_name == 'push' && github.ref == 'refs/heads/main'",
            pin_step,
        )
        self.assertIn('cachix pin cognipilot "main-${{ matrix.system }}"', pin_step)
        self.assertIn('"$(nix path-info ./result-public-cache-root)"', pin_step)
        self.assertIn("--keep-revisions 30", pin_step)
        self.assertIn("CACHIX_AUTH_TOKEN: ${{ secrets.CACHIX_AUTH_TOKEN }}", pin_step)

    def test_main_proves_fresh_signed_substitution_without_builders(self) -> None:
        workflow = self.workflow("ci.yml")
        proof = workflow[workflow.index("  substitute:") :]

        self.assertIn("needs: validate", proof)
        self.assertIn("Prove fresh signed substitution", proof)
        self.assertIn("--max-jobs 0 --builders \"\" --option fallback false", proof)
        self.assertIn(
            '"https://cache.nixos.org https://cognipilot.cachix.org"', proof
        )
        self.assertIn("nix path-info --json --recursive", proof)
        self.assertIn(
            "nix store verify --recursive --no-contents --sigs-needed 1 --verbose",
            proof,
        )
        self.assertIn("--substituter https://cache.nixos.org", proof)
        self.assertIn("--substituter https://cognipilot.cachix.org", proof)
        self.assertIn("substitution-closure.json", proof)
        self.assertIn("substitution-cache-coverage.json", proof)
        self.assertIn("substitution-trust.log", proof)
        self.assertIn("retention-days: 30", proof)
        self.assertNotIn("CACHIX_AUTH_TOKEN", proof)
        self.assertIn("shell: bash", proof)

    def test_sccache_pilot_uses_locked_nix_tools_and_proves_miss_then_hit(self) -> None:
        workflow = self.workflow("ci.yml")
        action = (
            "uses: mozilla-actions/sccache-action@"
            "9e7fa8a12102821edf02ca5dbea1acd0f89a2696 # v0.0.10"
        )

        self.assertIn(action, workflow)
        self.assertIn('version: "v0.16.0"', workflow)
        self.assertGreaterEqual(
            workflow.count("if: matrix.system == 'x86_64-linux'"), 11
        )
        self.assertEqual(workflow.count('CARGO_INCREMENTAL: "0"'), 2)
        self.assertEqual(workflow.count("sccache-tools/rustc/bin/rustc"), 2)
        self.assertEqual(workflow.count("sccache-tools/sccache/bin/sccache"), 6)
        self.assertEqual(workflow.count("cargo/bin/cargo clean"), 2)
        self.assertEqual(workflow.count("cargo/bin/cargo build --locked"), 2)
        self.assertIn("SCCACHE_GHA_ENABLED: \"true\"", workflow)
        self.assertIn("sccache/bin/sccache --stop-server", workflow)
        self.assertIn("sccache-cold.json", workflow)
        self.assertIn("sccache-warm.json", workflow)
        self.assertIn(".[0].stats.cache_misses.counts[]", workflow)
        self.assertIn(".[1].stats.cache_hits.counts[]", workflow)
        self.assertTrue((ROOT / "tests/fixtures/sccache-pilot/src/lib.rs").is_file())

    def test_codeowners_requires_enforceable_platform_review(self) -> None:
        codeowners = (ROOT / ".github" / "CODEOWNERS").read_text(encoding="utf-8")
        self.assertIn("/.github/CODEOWNERS @CogniPilot/admins", codeowners)
        self.assertIn("/flake.nix @CogniPilot/admins", codeowners)
        self.assertIn("/flake.lock @CogniPilot/admins", codeowners)
        self.assertIn("/nix/cognipilot/ @CogniPilot/admins", codeowners)
        self.assertIn("/nix/project-definitions/ @CogniPilot/admins", codeowners)
        self.assertIn("/tools/nixspace/ @CogniPilot/admins", codeowners)
        self.assertIn(
            "/.github/workflows/ @CogniPilot/admins",
            codeowners,
        )
        self.assertNotIn("@CogniPilot/credentials", codeowners)

    def test_no_legacy_or_non_nix_release_workflow_remains(self) -> None:
        self.assertFalse((WORKFLOWS / "release-cubs2-container.yml").exists())
        workflow = self.workflow("ci.yml")
        for legacy in (
            "devenv shell",
            "devenv --profile",
            "workspace.lock.json",
            "scripts/",
            "ws graph",
            "docker/",
        ):
            self.assertNotIn(legacy, workflow)

    @unittest.skipUnless(shutil.which("nix"), "Nix is required for policy evaluation")
    def test_one_nix_policy_drives_flake_reads_and_host_trust(self) -> None:
        result = subprocess.run(
            [
                "nix",
                "eval",
                "--impure",
                "--json",
                "--expr",
                "import ./nix/cognipilot/cache-policy.nix",
            ],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        )
        policy = json.loads(result.stdout)
        settings = policy["hostPlan"]["nix"]["settings"]
        flake_config_result = subprocess.run(
            [
                "nix",
                "eval",
                "--impure",
                "--json",
                "--expr",
                "(import ./flake.nix).nixConfig",
            ],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        )
        flake_config = json.loads(flake_config_result.stdout)

        self.assertEqual(
            policy["substituters"][1:],
            policy["flakeNixConfig"]["extra-substituters"],
        )
        self.assertEqual(policy["substituters"], settings["extra-substituters"])
        self.assertEqual(
            policy["publicKeys"][1:],
            policy["flakeNixConfig"]["extra-trusted-public-keys"],
        )
        self.assertEqual(policy["flakeNixConfig"], flake_config)
        self.assertEqual(policy["publicKeys"], settings["extra-trusted-public-keys"])
        self.assertIn("https://cognipilot.cachix.org", policy["substituters"])
        self.assertIn("https://cache.nixos.org", policy["substituters"])
        self.assertIn(
            "cognipilot.cachix.org-1:TSm+h3sAVYP0B+D+1uG5hvgI/4JFAS3LFvv/yhTwvK8=",
            policy["publicKeys"],
        )
        self.assertIn(
            "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY=",
            policy["publicKeys"],
        )
        self.assertEqual(["*"], settings["trusted-users"])
        self.assertEqual(4, policy["hostPlan"]["interfaceVersion"])
        self.assertEqual(
            [{"name": "public-cache-root", "path": "result-public-cache-root"}],
            policy["hostPlan"]["readiness"]["cache"]["roots"],
        )
        self.assertEqual(
            "union", policy["hostPlan"]["readiness"]["cache"]["coverageMode"]
        )
        self.assertEqual(
            [
                {
                    "name": "nixos",
                    "uri": "https://cache.nixos.org",
                },
                {
                    "name": "cognipilot",
                    "uri": "https://cognipilot.cachix.org",
                }
            ],
            policy["hostPlan"]["readiness"]["cache"]["stores"],
        )
        self.assertTrue(settings["accept-flake-config"])
        self.assertEqual(["nix-command", "flakes"], settings["experimental-features"])
        self.assertEqual("devenv", policy["hostPlan"]["tools"]["devenv"]["executable"])
        self.assertEqual("2.1.2", policy["hostPlan"]["tools"]["devenv"]["expectedVersion"])
        self.assertEqual(
            "github:cachix/devenv/407080febcc800abfd0fd688a0d513884aad620c",
            policy["hostPlan"]["tools"]["devenv"]["installArgv"][-1],
        )


if __name__ == "__main__":
    unittest.main()
