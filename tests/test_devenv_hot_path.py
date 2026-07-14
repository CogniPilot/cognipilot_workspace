from __future__ import annotations

from pathlib import Path
import json
import unittest


ROOT = Path(__file__).resolve().parents[1]


class DevenvHotPathTests(unittest.TestCase):
    def test_cubs2_build_uses_only_artifact_dependencies(self) -> None:
        components = (ROOT / "nix" / "components" / "default.nix").read_text(
            encoding="utf-8"
        )
        cubs2 = components.split("    cerebri_cubs2 = {", 1)[1].split(
            "    cerebri_rdd2 = {", 1
        )[0]

        self.assertIn(
            'buildDependencies = [\n        "synapse_fbs"\n        "rumoca"\n      ];',
            cubs2,
        )
        self.assertNotIn('"csyn"\n      ];\n      needsWest', cubs2)
        self.assertNotIn("-p always", cubs2)
        self.assertIn("-p auto", cubs2)
        self.assertIn("scripts/workspace-cmake-build", cubs2)

    def test_local_synapse_build_does_not_invoke_ci(self) -> None:
        components = (ROOT / "nix" / "components" / "default.nix").read_text(
            encoding="utf-8"
        )
        synapse = components.split("    synapse_fbs = {", 1)[1].split(
            "    rumoca = {", 1
        )[0]
        local_build = synapse.split("local = {", 1)[1].split("release.build =", 1)[0]

        self.assertIn("-- build --release-name local", local_build)
        self.assertNotIn("-- ci", local_build)

    def test_synapse_cache_validates_complete_export_roots(self) -> None:
        components = (ROOT / "nix" / "components" / "default.nix").read_text(
            encoding="utf-8"
        )
        synapse = components.split("    synapse_fbs = {", 1)[1].split(
            "    rumoca = {", 1
        )[0]

        for export in ("rust", "python", "js", "c"):
            self.assertIn(f'"devel:synapse_fbs/{export}"', synapse)

    def test_modelica_build_orders_local_rumoca_artifact(self) -> None:
        components = (ROOT / "nix" / "components" / "default.nix").read_text(
            encoding="utf-8"
        )
        modelica = components.split("    modelica_models = {", 1)[1].split(
            "    csyn = {", 1
        )[0]

        self.assertIn('buildDependencies = [ "rumoca" ];', modelica)

    def test_electrode_build_declares_every_launch_artifact(self) -> None:
        components = (ROOT / "nix" / "components" / "default.nix").read_text(
            encoding="utf-8"
        )
        electrode = components.split("    electrode_web = {", 1)[1].split(
            "    cerebri_cubs2 = {", 1
        )[0]

        self.assertIn('"repo:target/debug/electrode-ground-station"', electrode)
        self.assertIn('"repo:target/debug/electrode-fake-sim"', electrode)

    def test_task_edges_use_build_dependencies(self) -> None:
        tasks = (ROOT / "nix" / "tasks.nix").read_text(encoding="utf-8")

        self.assertIn("component.buildDependencies", tasks)
        self.assertIn("scripts/workspace-cached-task", tasks)
        self.assertNotIn(
            'map (dependency: "local:build:${dependency}") component.dependencies',
            tasks,
        )

    def test_release_tasks_use_isolated_environment(self) -> None:
        tasks = (ROOT / "nix" / "tasks.nix").read_text(encoding="utf-8")

        self.assertIn('if mode == "release"', tasks)
        self.assertIn("scripts/workspace-release-environment", tasks)

    def test_tasks_support_isolated_benchmark_state(self) -> None:
        tasks = (ROOT / "nix" / "tasks.nix").read_text(encoding="utf-8")

        self.assertIn("COGNIPILOT_STATE_ROOT_OVERRIDE", tasks)
        self.assertIn(
            'COGNIPILOT_BUILD_ROOT="$COGNIPILOT_STATE_ROOT_OVERRIDE/build"', tasks
        )

    def test_main_benchmark_targets_have_warm_budgets(self) -> None:
        thresholds = json.loads(
            (ROOT / "nix" / "workspace-benchmark-thresholds.json").read_text(
                encoding="utf-8"
            )
        )
        expected = {
            "FastDyn",
            "cerebri_cubs2",
            "cerebri_modules",
            "csyn",
            "electrode_web",
            "modelica_models",
            "rumoca",
            "synapse_fbs",
            "synapse_ppm_bridge",
        }

        self.assertEqual(expected, set(thresholds["warm_max_seconds"]))
        self.assertTrue(
            all(value > 0 for value in thresholds["warm_max_seconds"].values())
        )


if __name__ == "__main__":
    unittest.main()
