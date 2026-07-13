from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest


SCRIPT = Path(__file__).parents[1] / "scripts" / "workspace-west.py"
SPEC = importlib.util.spec_from_file_location("workspace_west", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
workspace_west = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(workspace_west)


def manifest(revision: str, *, url: str = "https://github.com/CogniPilot/csyn") -> str:
    return f"""\
manifest:
  projects:
    - name: csyn
      url: {url}
      revision: "{revision}"
      path: modules/lib/csyn
"""


class WorkspaceWestTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        for name in ("cerebri_cubs2", "cerebri_rdd2"):
            (self.root / "src" / name).mkdir(parents=True)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def write(self, cubs: str, rdd: str) -> None:
        (self.root / "src/cerebri_cubs2/west.yml").write_text(cubs)
        (self.root / "src/cerebri_rdd2/west.yml").write_text(rdd)

    def test_identical_projects_form_one_pinned_union(self) -> None:
        revision = "1" * 40
        self.write(manifest(revision), manifest(revision))
        merged, _, origins = workspace_west.validate(self.root)
        self.assertEqual(merged["manifest"]["projects"][0]["revision"], revision)
        self.assertEqual(len(origins["csyn"]), 2)

    def test_conflicting_duplicate_is_rejected(self) -> None:
        self.write(manifest("1" * 40), manifest("2" * 40))
        with self.assertRaisesRegex(workspace_west.WorkspaceError, "conflicts"):
            workspace_west.validate(self.root)

    def test_floating_revision_is_rejected(self) -> None:
        self.write(manifest("main"), manifest("main"))
        with self.assertRaisesRegex(workspace_west.WorkspaceError, "full commit pins"):
            workspace_west.validate(self.root)

    def test_source_view_links_cache_and_preserves_real_directories(self) -> None:
        cache = self.root / "cache"
        local = cache / "local"
        for relative in (".west", "manifest", "zephyr", "modules/lib/csyn"):
            (local / relative).mkdir(parents=True)
        editable = self.root / "src/modules/lib/csyn"
        editable.mkdir(parents=True)
        marker = editable / "local.txt"
        marker.write_text("keep\n", encoding="utf-8")
        merged = {
            "manifest": {
                "projects": [
                    {"name": "zephyr", "path": "zephyr"},
                    {"name": "csyn", "path": "modules/lib/csyn"},
                ]
            }
        }

        workspace_west.create_source_view(self.root, cache, merged)

        self.assertTrue((self.root / "src/.west").is_symlink())
        self.assertTrue((self.root / "src/manifest").is_symlink())
        self.assertTrue((self.root / "src/zephyr").is_symlink())
        self.assertFalse(editable.is_symlink())
        self.assertEqual(marker.read_text(encoding="utf-8"), "keep\n")

    def test_escaping_project_path_is_rejected(self) -> None:
        self.write(
            manifest("1" * 40).replace("modules/lib/csyn", "../outside"),
            manifest("1" * 40).replace("modules/lib/csyn", "../outside"),
        )
        with self.assertRaisesRegex(workspace_west.WorkspaceError, "unsafe"):
            workspace_west.validate(self.root)


if __name__ == "__main__":
    unittest.main()
