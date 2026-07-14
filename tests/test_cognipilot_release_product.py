from __future__ import annotations

import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests" / "fixtures" / "release-product"
SYSTEM = "x86_64-linux"
COMMAND_TIMEOUT_SECONDS = 90


@unittest.skipUnless(shutil.which("nix"), "Nix is required for release evaluation")
class CognipilotReleaseProductTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        shutil.copytree(FIXTURE, self.root, dirs_exist_ok=True)
        modules = self.root / "nix" / "cognipilot"
        modules.mkdir(parents=True)
        for name in (
            "action-presets.nix",
            "action-tool-profiles.nix",
            "flake-module.nix",
            "nixspace-module.nix",
            "product-flake-module.nix",
            "resolution-module.nix",
            "resolution-template.nix",
        ):
            shutil.copy2(ROOT / "nix" / "cognipilot" / name, modules / name)
        nixspace_modules = self.root / "nix" / "nixspace"
        nixspace_modules.mkdir(parents=True)
        for name in ("index-module.nix", "tool-module.nix"):
            shutil.copy2(ROOT / "nix" / "nixspace" / name, nixspace_modules / name)
        shutil.copytree(
            ROOT / "tools" / "nixspace",
            self.root / "tools" / "nixspace",
            ignore=shutil.ignore_patterns("target"),
        )
        shutil.copy2(ROOT / "flake.lock", self.root / "flake.lock")

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def run_command(
        self, *command: str, check: bool = True
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            command,
            cwd=self.root,
            text=True,
            capture_output=True,
            check=check,
            timeout=COMMAND_TIMEOUT_SECONDS,
        )

    def prepare(self, replacement: tuple[str, str] | None = None) -> None:
        if replacement is not None:
            flake = self.root / "flake.nix"
            source = flake.read_text(encoding="utf-8")
            self.assertIn(replacement[0], source)
            flake.write_text(
                source.replace(replacement[0], replacement[1], 1), encoding="utf-8"
            )
        self.run_command("nix", "flake", "lock", "--offline")
        self.run_command("git", "init", "-q")
        self.run_command("git", "add", ".")
        self.run_command(
            "git",
            "-c",
            "user.name=CogniPilot Test",
            "-c",
            "user.email=test@invalid",
            "commit",
            "-qm",
            "tracked release fixture",
        )

    def evaluate(self, attribute: str, *, raw: bool = False) -> object:
        result = self.run_command(
            "nix",
            "eval",
            "--offline",
            "--raw" if raw else "--json",
            f".#{attribute}",
        )
        return result.stdout if raw else json.loads(result.stdout)

    def derivation_inputs(self, drv_path: str) -> set[str]:
        derivation = self.derivation(drv_path)
        if "inputs" in derivation:
            return {
                f"/nix/store/{name}" for name in derivation["inputs"]["drvs"]
            }
        return set(derivation["inputDrvs"])

    def derivation(self, drv_path: str) -> dict[str, object]:
        result = self.run_command("nix", "derivation", "show", drv_path)
        document = json.loads(result.stdout)
        if document.get("version") == 4:
            return next(iter(document["derivations"].values()))
        return next(iter(document.values()))

    def input_package_drv(self, package: str) -> str:
        result = self.run_command(
            "nix",
            "eval",
            "--offline",
            "--impure",
            "--raw",
            "--expr",
            (
                "let flake = builtins.getFlake (toString ./.); "
                f'in flake.inputs.release_provider.packages.{SYSTEM}."{package}".drvPath'
            ),
        )
        return result.stdout.strip()

    def build(self, attribute: str) -> Path:
        target = f".#{attribute}"
        dry_run = self.run_command(
            "nix",
            "build",
            "--dry-run",
            "--no-link",
            target,
            check=False,
        )
        self.assertEqual(0, dry_run.returncode, dry_run.stderr)
        planned = re.findall(
            r"^\s+(/nix/store/\S+\.drv)$", dry_run.stderr, re.MULTILINE
        )
        nixspace_drv = self.evaluate(
            f"packages.{SYSTEM}.nixspace.drvPath", raw=True
        ).strip()
        nixspace_closure = set(
            self.run_command("nix-store", "--query", "--requisites", nixspace_drv)
            .stdout.splitlines()
        )
        for derivation in planned:
            if derivation in nixspace_closure:
                continue
            unexpected = (
                "release isolation fixture would build a non-fixture derivation:\n"
                f"{dry_run.stderr}"
            )
            self.assertRegex(
                derivation,
                (
                    r"-(?:alpha-release|alpha-runtime|alpha-config\.json|"
                    r"beta-firmware|release-fixture[^/]*)\.drv$"
                ),
                unexpected,
            )
        result = self.run_command(
            "nix",
            "build",
            "--no-link",
            "--print-out-paths",
            target,
            check=False,
        )
        self.assertEqual(0, result.returncode, result.stderr)
        paths = [Path(line) for line in result.stdout.splitlines() if line]
        self.assertEqual(1, len(paths), result.stdout)
        self.assertTrue(paths[0].exists(), paths[0])
        return paths[0]

    def recursive_path_info(self, path: Path) -> dict[str, dict[str, object]]:
        result = self.run_command(
            "nix",
            "path-info",
            "--json",
            "--recursive",
            str(path),
        )
        document = json.loads(result.stdout)
        if isinstance(document, list):
            return {entry["path"]: entry for entry in document}
        return {
            key: value | {"path": value.get("path", key)}
            for key, value in document.items()
        }

    def sri_hash(self, value: str) -> str:
        return self.run_command(
            "nix",
            "hash",
            "convert",
            "--hash-algo",
            "sha256",
            "--to",
            "sri",
            value,
        ).stdout.strip()

    def hex_hash(self, value: str) -> str:
        return self.run_command(
            "nix",
            "hash",
            "convert",
            "--hash-algo",
            "sha256",
            "--to",
            "base16",
            value,
        ).stdout.strip()

    def test_release_refs_normalize_without_conflating_source_authority(self) -> None:
        self.prepare()

        projects = self.evaluate("nixspaceIndex")["projects"]

        self.assertEqual(
            projects["alpha"]["targets"]["default"]["release"],
            {"provider": "release_provider", "package": "alpha-release"},
        )
        self.assertEqual(projects["alpha"]["source"]["input"], "alpha_source")
        self.assertNotEqual(
            projects["alpha"]["source"]["input"],
            projects["alpha"]["targets"]["default"]["release"]["provider"],
        )
        self.assertEqual(projects["observer"]["deployability"], "qualification")
        self.assertIsNone(projects["observer"]["targets"]["default"]["release"])

    def test_named_roots_default_and_workspace_reference_exact_derivations(self) -> None:
        self.prepare()

        alpha_provider = self.input_package_drv("alpha-release")
        beta_provider = self.input_package_drv("beta-firmware")
        alpha_target = self.evaluate(
            f"packages.{SYSTEM}.target-alpha--default.drvPath", raw=True
        ).strip()
        beta_target = self.evaluate(
            f"packages.{SYSTEM}.target-beta--firmware.drvPath", raw=True
        ).strip()
        default = self.evaluate(f"packages.{SYSTEM}.default.drvPath", raw=True).strip()
        workspace = self.evaluate(
            f"packages.{SYSTEM}.workspace.drvPath", raw=True
        ).strip()
        cache_root = self.evaluate(
            f"packages.{SYSTEM}.public-cache-root.drvPath", raw=True
        ).strip()
        product = self.evaluate(
            f"packages.{SYSTEM}.product-release-fixture.drvPath", raw=True
        ).strip()
        normalized = self.evaluate(
            f"packages.{SYSTEM}.nixspace-index.drvPath", raw=True
        ).strip()

        self.assertEqual(alpha_target, alpha_provider)
        self.assertEqual(beta_target, beta_provider)
        self.assertEqual(default, alpha_provider)
        self.assertEqual(product, workspace)
        self.assertTrue(
            {alpha_provider, normalized}.issubset(self.derivation_inputs(cache_root))
        )
        self.assertNotIn(beta_provider, self.derivation_inputs(cache_root))
        self.assertTrue(
            {alpha_provider, beta_provider}.issubset(self.derivation_inputs(product))
        )

    def test_promotion_record_is_derived_from_locked_selected_product(self) -> None:
        self.prepare()

        record = self.evaluate(
            f"packages.{SYSTEM}.promotion-record.normalizedRecord"
        )
        packages = {package["packageId"]: package for package in record["packages"]}

        self.assertEqual(2, record["schemaVersion"])
        self.assertEqual(
            {
                "id": "release-fixture",
                "interfaceVersion": 1,
                "system": SYSTEM,
            },
            {
                key: record["product"][key]
                for key in ("id", "interfaceVersion", "system")
            },
        )
        self.assertTrue(record["product"]["sourceIdentity"]["immutable"])
        self.assertRegex(record["selectionDigest"], r"^[0-9a-f]{64}$")
        self.assertTrue(
            record["builderIdentity"]["id"].startswith("urn:nix:derivation:")
        )
        self.assertTrue(
            record["builderIdentity"]["builderDependencies"][0]["annotations"][
                "nixOutput"
            ][
                "drvPath"
            ].endswith(".drv")
        )
        self.assertTrue(
            record["builderIdentity"]["builderDependencies"][0]["annotations"][
                "nixpkgs"
            ]["immutable"]
        )
        self.assertEqual([], record["allowedExternalSourceExceptions"])

        alpha = packages["alpha"]
        self.assertEqual("deployable", alpha["promotionStatus"])
        self.assertEqual(
            {"source": "literal", "value": "1.2.3", "file": None},
            alpha["softwareVersion"],
        )
        self.assertEqual(["cargo-v1"], alpha["adapters"])
        self.assertEqual(1, alpha["schema"]["projectInterfaceVersion"])
        self.assertEqual("alpha_source", alpha["source"]["identity"]["input"])
        self.assertEqual("public", alpha["source"]["visibility"])
        self.assertEqual(
            "alpha_definition", alpha["definition"]["identity"]["input"]
        )
        self.assertEqual("external", alpha["definition"]["origin"])
        self.assertTrue(alpha["source"]["identity"]["immutable"])
        self.assertTrue(alpha["definition"]["identity"]["immutable"])
        self.assertIsNotNone(alpha["source"]["identity"]["narHash"])
        self.assertIn(
            alpha["source"]["identity"]["revision"]["kind"],
            {"git-commit", "nix-nar"},
        )
        if alpha["source"]["identity"]["revision"]["kind"] == "git-commit":
            self.assertRegex(
                alpha["source"]["identity"]["revision"]["value"],
                r"^[0-9a-f]{40}$",
            )
        else:
            self.assertEqual(
                alpha["source"]["identity"]["narHash"],
                alpha["source"]["identity"]["revision"]["value"],
            )

        release = alpha["targets"][0]["release"]
        self.assertEqual(
            {"provider": "release_provider", "package": "alpha-release"},
            release["release"],
        )
        self.assertEqual(SYSTEM, release["system"])
        self.assertTrue(release["providerIdentity"]["immutable"])
        self.assertTrue(release["output"]["drvPath"].startswith("/nix/store/"))
        self.assertTrue(release["output"]["storePath"].startswith("/nix/store/"))

        observer = packages["observer"]
        self.assertEqual("private", observer["source"]["visibility"])
        self.assertEqual("qualification-only", observer["promotionStatus"])
        self.assertEqual("qualification", observer["deployability"])
        self.assertEqual("missing", observer["source"]["identity"]["status"])
        self.assertIsNone(observer["targets"][0]["release"])
        self.assertEqual("not-released", observer["targets"][0]["promotionStatus"])

        self.assertIsNone(record["immutableDependencyClosure"]["storePaths"])
        roots = record["immutableDependencyClosure"]["roots"]
        self.assertEqual(record["outputs"]["workspace"], roots[0])
        self.assertEqual(
            record["documents"]["sbom"]["output"], roots[1]
        )
        self.assertEqual(
            record["builderIdentity"]["builderDependencies"][0]["annotations"][
                "nixOutput"
            ],
            roots[2],
        )
        self.assertEqual("SPDX-2.3", record["documents"]["sbom"]["format"])
        self.assertEqual(
            "application/spdx+json", record["documents"]["sbom"]["mediaType"]
        )

    def test_promotion_package_and_check_share_one_unrealized_derivation(self) -> None:
        self.prepare()

        package = self.evaluate(
            f"packages.{SYSTEM}.promotion-record.drvPath", raw=True
        ).strip()
        check = self.evaluate(
            f"checks.{SYSTEM}.cognipilot-promotion-record.drvPath", raw=True
        ).strip()
        workspace = self.evaluate(
            f"packages.{SYSTEM}.workspace.drvPath", raw=True
        ).strip()
        package_names = json.loads(
            self.run_command(
                "nix",
                "eval",
                "--offline",
                "--json",
                f".#packages.{SYSTEM}",
                "--apply",
                "builtins.attrNames",
            ).stdout
        )

        self.assertEqual(package, check)
        self.assertIn(workspace, self.derivation_inputs(package))
        self.assertIn("promotion-record", package_names)
        self.assertIn("promotion-sbom", package_names)
        self.assertIn("promotion-attestation", package_names)
        default = self.evaluate(
            f"packages.{SYSTEM}.default.drvPath", raw=True
        ).strip()
        self.assertNotEqual(package, default)

    def test_promotion_builder_is_only_the_typed_nixspace_materializer(self) -> None:
        self.prepare()

        promotion = self.evaluate(
            f"packages.{SYSTEM}.promotion-record.drvPath", raw=True
        ).strip()
        nixspace = self.evaluate(
            f"packages.{SYSTEM}.nixspace.drvPath", raw=True
        ).strip()
        derivation = self.derivation(promotion)
        build_command = derivation["structuredAttrs"]["buildCommand"]

        self.assertIn(nixspace, self.derivation_inputs(promotion))
        self.assertEqual(1, build_command.count("_materialize-closure"))
        self.assertIn("--plan-file", build_command)
        self.assertIn("--structured-attrs", build_command)
        self.assertIn("--output", build_command)
        self.assertNotIn("jq", build_command)
        self.assertNotIn("mkdir", build_command)
        self.assertEqual([], derivation["structuredAttrs"]["nativeBuildInputs"])

    def test_spdx_sbom_is_a_deterministic_projection_of_the_product_graph(self) -> None:
        self.prepare()

        document = self.evaluate(f"packages.{SYSTEM}.promotion-sbom.document")
        promotion = self.evaluate(
            f"packages.{SYSTEM}.promotion-record.normalizedRecord"
        )
        packages = {package["name"]: package for package in document["packages"]}

        self.assertEqual("SPDX-2.3", document["spdxVersion"])
        self.assertEqual("CC0-1.0", document["dataLicense"])
        self.assertEqual("SPDXRef-DOCUMENT", document["SPDXID"])
        self.assertRegex(
            document["documentNamespace"],
            rf"^https://spdx.org/spdxdocs/release-fixture-{SYSTEM}-[0-9a-f]{{64}}$",
        )
        self.assertEqual(1, len(document["documentDescribes"]))
        self.assertEqual(
            {"release-fixture", "alpha", "beta", "observer"}, set(packages)
        )
        self.assertFalse(packages["alpha"]["filesAnalyzed"])
        self.assertEqual("1.2.3", packages["alpha"]["versionInfo"])
        alpha_source = next(
            reference
            for reference in packages["alpha"]["externalRefs"]
            if reference["referenceType"].endswith("#LocationRef-nix-input")
        )
        self.assertRegex(
            alpha_source["referenceLocator"],
            r"^urn:nix:input:alpha_source:sha256:[0-9a-f]{64}$",
        )
        output_refs = {
            reference["referenceLocator"]
            for reference in packages["alpha"]["externalRefs"]
            if reference["referenceType"].endswith(
                "#LocationRef-nix-store-output"
            )
        }
        self.assertEqual(
            {
                target["storePath"]
                for target in promotion["outputs"]["targets"]
                if target["packageId"] == "alpha"
            },
            output_refs,
        )
        self.assertEqual(3, len(document["relationships"]))
        self.assertTrue(
            all(
                relation["relationshipType"] == "DEPENDS_ON"
                for relation in document["relationships"]
            )
        )

        sbom = self.build(f"packages.{SYSTEM}.promotion-sbom")
        materialized = json.loads(
            (sbom / "share/cognipilot/sbom.spdx.json").read_text(encoding="utf-8")
        )
        self.assertEqual(document, materialized)

    def test_realized_in_toto_slsa_attestation_proves_outputs_and_byproducts(
        self,
    ) -> None:
        self.prepare()

        attestation = self.build(f"packages.{SYSTEM}.promotion-attestation")
        statement_path = (
            attestation / "share/cognipilot/provenance.intoto.json"
        )
        statement = json.loads(statement_path.read_text(encoding="utf-8"))
        promotion = self.evaluate(
            f"packages.{SYSTEM}.promotion-record.normalizedRecord"
        )

        self.assertEqual("https://in-toto.io/Statement/v1", statement["_type"])
        self.assertEqual(
            "https://slsa.dev/provenance/v1", statement["predicateType"]
        )
        predicate = statement["predicate"]
        builder = predicate["runDetails"]["builder"]
        self.assertEqual(promotion["builderIdentity"]["id"], builder["id"])
        self.assertEqual(
            promotion["builderIdentity"]["version"], builder["version"]
        )
        self.assertEqual(
            promotion["builderIdentity"]["builderDependencies"][0]["uri"],
            builder["builderDependencies"][0]["uri"],
        )
        self.assertRegex(
            builder["builderDependencies"][0]["digest"]["sha256"],
            r"^[0-9a-f]{64}$",
        )
        self.assertIn(
            "narHash",
            builder["builderDependencies"][0]["annotations"]["nixOutput"],
        )
        builder_output = builder["builderDependencies"][0]["annotations"][
            "nixOutput"
        ]
        self.assertEqual(
            self.hex_hash(builder_output["narHash"]),
            builder["builderDependencies"][0]["digest"]["sha256"],
        )
        self.assertEqual(
            promotion["selectionDigest"],
            predicate["buildDefinition"]["externalParameters"]["selectionDigest"],
        )
        dependencies = predicate["buildDefinition"]["resolvedDependencies"]
        self.assertTrue(dependencies)
        self.assertTrue(
            all(
                dependency["annotations"]["nix"]["revision"] is not None
                and re.fullmatch(
                    r"[0-9a-f]{64}", dependency["digest"]["nixNarSha256"]
                )
                for dependency in dependencies
            )
        )

        proven_resources = statement["subject"] + predicate["runDetails"][
            "byproducts"
        ]
        for resource in proven_resources:
            output = resource["annotations"]["nixOutput"]
            path_info = self.recursive_path_info(Path(output["storePath"]))[
                output["storePath"]
            ]
            self.assertEqual(path_info["narHash"], self.sri_hash(output["narHash"]))
            self.assertEqual(path_info["narSize"], output["narSize"])
            self.assertRegex(resource["digest"]["sha256"], r"^[0-9a-f]{64}$")
            self.assertEqual(
                self.hex_hash(output["narHash"]), resource["digest"]["sha256"]
            )

        def assert_digest_maps_are_hex(value: object) -> None:
            if isinstance(value, dict):
                if "digest" in value:
                    self.assertIsInstance(value["digest"], dict)
                    for digest in value["digest"].values():
                        self.assertIsInstance(digest, str)
                        self.assertRegex(digest, r"^(?:[0-9a-f]{2})+$")
                for nested in value.values():
                    assert_digest_maps_are_hex(nested)
            elif isinstance(value, list):
                for nested in value:
                    assert_digest_maps_are_hex(nested)

        assert_digest_maps_are_hex(statement)

        self.assertNotIn("invocationId", predicate["runDetails"]["metadata"])
        closure = predicate["runDetails"]["metadata"]["cognipilot_nixClosure"]
        self.assertEqual(
            sorted(entry["path"] for entry in closure),
            [entry["path"] for entry in closure],
        )
        closure_paths = {entry["path"] for entry in closure}
        for resource in proven_resources:
            self.assertIn(
                resource["annotations"]["nixOutput"]["storePath"], closure_paths
            )

        derivation = self.derivation(
            self.evaluate(
                f"packages.{SYSTEM}.promotion-attestation.drvPath", raw=True
            ).strip()
        )
        build_command = derivation["structuredAttrs"]["buildCommand"]
        self.assertEqual(1, build_command.count("_materialize-closure"))
        self.assertNotIn("jq", build_command)
        self.assertEqual([], derivation["structuredAttrs"]["nativeBuildInputs"])

    def test_realized_promotion_proves_closure_and_excludes_local_state(self) -> None:
        self.prepare()
        sentinels = {
            "source": "COGNIPILOT-DIRTY-SOURCE-7d13e024",
            "build": "COGNIPILOT-DIRTY-BUILD-6c6244f1",
            "devel": "COGNIPILOT-DIRTY-DEVEL-c3f92395",
        }
        sentinel_paths = {
            self.root / "sources" / "alpha" / "local-sentinel.txt": sentinels["source"],
            self.root / "build" / "local-sentinel.txt": sentinels["build"],
            self.root / "devel" / "local-sentinel.txt": sentinels["devel"],
        }
        for path, sentinel in sentinel_paths.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"{sentinel}\n", encoding="utf-8")

        dirty = self.run_command(
            "git", "status", "--porcelain", "--untracked-files=all"
        ).stdout
        for path in sentinel_paths:
            self.assertIn(f"?? {path.relative_to(self.root)}", dirty)

        promotion = self.build(f"packages.{SYSTEM}.promotion-record")
        record_path = promotion / "share" / "cognipilot" / "promotion-record.json"
        record_bytes = record_path.read_bytes()
        record = json.loads(record_bytes)
        workspace = Path(record["outputs"]["workspace"]["storePath"])
        closure_roots = [
            Path(root["storePath"])
            for root in record["immutableDependencyClosure"]["roots"]
        ]
        nix_closure: dict[str, dict[str, object]] = {}
        for root in closure_roots:
            nix_closure.update(self.recursive_path_info(root))
        recorded_closure = record["immutableDependencyClosure"]["storePaths"]
        recorded_paths = [entry["path"] for entry in recorded_closure]

        self.assertEqual(sorted(recorded_paths), recorded_paths)
        self.assertEqual(set(nix_closure), set(recorded_paths))
        for entry in recorded_closure:
            actual = nix_closure[entry["path"]]
            self.assertEqual(actual["narHash"], self.sri_hash(entry["narHash"]))
            self.assertEqual(actual["narSize"], entry["narSize"])
            self.assertEqual(
                sorted(actual.get("references", [])), entry["references"]
            )

        proven_outputs = [record["outputs"]["workspace"]]
        if record["outputs"]["product"] is not None:
            proven_outputs.append(record["outputs"]["product"])
        proven_outputs.extend(record["outputs"]["targets"])
        proven_outputs.extend(
            target["release"]["output"]
            for package in record["packages"]
            for target in package["targets"]
            if target["release"] is not None
        )
        for output in proven_outputs:
            actual = nix_closure[output["storePath"]]
            self.assertEqual(actual["narHash"], self.sri_hash(output["narHash"]))
            self.assertEqual(actual["narSize"], output["narSize"])

        forbidden = [value.encode() for value in sentinels.values()]
        forbidden.append(str(self.root).encode())
        realized_files = [record_path]
        for store_path in map(Path, recorded_paths):
            if store_path.is_file():
                realized_files.append(store_path)
            elif store_path.is_dir():
                realized_files.extend(
                    candidate
                    for candidate in store_path.rglob("*")
                    if candidate.is_file()
                )
        for path in realized_files:
            contents = path.read_bytes()
            for value in forbidden:
                self.assertNotIn(value, contents, path)

    def test_locked_resource_executable_and_launch_survive_absent_checkouts(self) -> None:
        self.prepare()

        plan = self.evaluate(
            f"packages.{SYSTEM}.nixspace-resolution-plan.document"
        )
        alpha_plan = plan["packagePlans"]["alpha"]
        artifact = plan["artifacts"]["alpha:default:runtime"]
        self.assertEqual("locked", alpha_plan["selectedScope"])
        self.assertEqual(
            {"alpha": "locked"},
            alpha_plan["commandScopes"]["locked"]["selectedCandidates"],
        )
        # A release-only composition has no editable task generation.  The
        # selected immutable candidate is exact; no local fallback is emitted.
        self.assertIsNone(artifact["candidates"]["local"])
        self.assertEqual("nix-store", artifact["candidates"]["locked"]["kind"])

        result = self.run_command(
            "nix",
            "build",
            "--offline",
            "--no-link",
            "--print-out-paths",
            f".#packages.{SYSTEM}.target-alpha--default",
            f".#packages.{SYSTEM}.ws",
            check=False,
        )
        self.assertEqual(0, result.returncode, result.stderr)
        outputs = [Path(line) for line in result.stdout.splitlines() if line]
        target = next(path for path in outputs if (path / "bin/alpha-runtime").exists())
        client = next(path for path in outputs if (path / "bin/ws").exists())

        expected_resource = target / "share/alpha/config.json"
        self.assertEqual(
            {"source": "immutable-release"},
            json.loads(expected_resource.read_text(encoding="utf-8")),
        )
        target_closure = self.recursive_path_info(target)
        self.assertIn(str(target), target_closure)

        # Make both locked input source directories unavailable.  Everything
        # below invokes the already-built store interface directly and cannot
        # reevaluate or recover either checkout.
        (self.root / "sources").rename(self.root / "sources.unavailable")
        (self.root / "definitions").rename(self.root / "definitions.unavailable")
        self.assertFalse((self.root / "sources").exists())
        self.assertFalse((self.root / "definitions").exists())
        self.assertFalse((self.root / "editable-checkouts").exists())

        prefix_result = self.run_command(
            str(client / "bin/ws"), "package", "prefix", "alpha", "--json"
        )
        prefix = json.loads(prefix_result.stdout)
        self.assertEqual("locked", prefix["selectedCandidate"])
        self.assertEqual(str(target), prefix["path"])

        resource_result = self.run_command(
            str(client / "bin/ws"), "resource", "alpha/config", "--json"
        )
        resource = json.loads(resource_result.stdout)
        self.assertEqual("locked", resource["selectedCandidate"])
        self.assertEqual(str(expected_resource), resource["path"])

        launch_result = self.run_command(
            str(client / "bin/ws"),
            "launch",
            "plan",
            "alpha/release",
            "--json",
        )
        launch = json.loads(launch_result.stdout)
        self.assertEqual(["alpha:default:runtime"], launch["requiredArtifacts"])
        self.assertEqual(["alpha/config"], launch["requiredResources"])
        process = launch["processes"][0]
        self.assertEqual("alpha/runtime", process["executable"])

        execution = self.run_command(
            str(client / "bin/ws"),
            "run",
            "--selection-root",
            "alpha",
            process["executable"],
            "--",
            *process["argv"],
        )
        self.assertEqual(
            "alpha immutable release:--from-release-launch\n", execution.stdout
        )

    def test_release_only_plan_does_not_relax_selected_local_generation(self) -> None:
        self.prepare(
            (
                'selectedScopes.alpha = "locked";',
                'selectedScopes.alpha = "local";',
            )
        )

        result = self.run_command(
            "nix",
            "eval",
            "--offline",
            "--json",
            f".#packages.{SYSTEM}.nixspace-resolution-plan.document",
            check=False,
        )

        self.assertNotEqual(0, result.returncode)
        self.assertIn(
            "has no ActionTask v2 generation for `alpha:default:build`",
            result.stderr,
        )

    def test_deployable_promotion_rejects_missing_locked_input_identities(self) -> None:
        cases = (
            (
                'input = "alpha_source";',
                'input = "missing_alpha_source";',
                "package `alpha` source input `missing_alpha_source` is missing",
            ),
            (
                'input = "alpha_definition";',
                'input = "missing_alpha_definition";',
                (
                    "package `alpha` definition input `missing_alpha_definition` "
                    "is missing"
                ),
            ),
        )
        for old, new, diagnostic in cases:
            with self.subTest(diagnostic=diagnostic):
                self.tearDown()
                self.setUp()
                self.prepare((old, new))

                result = self.run_command(
                    "nix",
                    "eval",
                    "--offline",
                    "--raw",
                    f".#packages.{SYSTEM}.promotion-record.drvPath",
                    check=False,
                )

                self.assertNotEqual(0, result.returncode)
                self.assertIn("CogniPilot promotion record violations", result.stderr)
                self.assertIn(diagnostic, result.stderr)

    def test_dirty_product_source_cannot_emit_deployable_promotion(self) -> None:
        self.prepare()
        source = self.root / "sources" / "alpha" / "source.txt"
        source.write_text("mutable alpha source\n", encoding="utf-8")

        result = self.run_command(
            "nix",
            "eval",
            "--offline",
            "--raw",
            f".#packages.{SYSTEM}.promotion-record.drvPath",
            check=False,
        )

        self.assertNotEqual(0, result.returncode)
        self.assertIn("CogniPilot promotion record violations", result.stderr)
        self.assertIn(
            "product `release-fixture` input `self` is mutable", result.stderr
        )

    def test_default_is_absent_without_explicit_natural_target(self) -> None:
        self.prepare(
            (
                '        defaultTarget = {\n          packageId = "alpha";\n          targetId = "default";\n        };\n',
                "",
            )
        )

        names = self.run_command(
            "nix",
            "eval",
            "--offline",
            "--json",
            f".#packages.{SYSTEM}",
            "--apply",
            "builtins.attrNames",
        )

        self.assertNotIn("default", json.loads(names.stdout))

    def test_missing_provider_and_output_fail_with_actionable_coordinates(self) -> None:
        cases = (
            (
                'provider = "release_provider";',
                'provider = "missing_provider";',
                "missing root flake input `missing_provider`",
            ),
            (
                'package = "alpha-release";',
                'package = "missing-output";',
                f"does not export packages.{SYSTEM}.missing-output",
            ),
        )
        for old, new, diagnostic in cases:
            with self.subTest(diagnostic=diagnostic):
                self.tearDown()
                self.setUp()
                self.prepare((old, new))

                result = self.run_command(
                    "nix",
                    "eval",
                    "--offline",
                    "--raw",
                    f".#packages.{SYSTEM}.target-alpha--default.drvPath",
                    check=False,
                )

                self.assertNotEqual(result.returncode, 0)
                self.assertIn("alpha:default", result.stderr)
                self.assertIn(diagnostic, result.stderr)


if __name__ == "__main__":
    unittest.main()
