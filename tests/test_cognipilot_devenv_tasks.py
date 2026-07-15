from __future__ import annotations

import hashlib
import json
from pathlib import Path
import shlex
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[1]
GENERATOR = ROOT / "nix" / "cognipilot" / "devenv-task-generator.nix"
MODULE = ROOT / "nix" / "cognipilot" / "devenv-task-module.nix"
TOOL_PROFILES = ROOT / "nix" / "cognipilot" / "action-tool-profiles.nix"
INDEX = ROOT / "tests" / "fixtures" / "devenv-tasks" / "index.nix"
NIXSPACE_EXECUTABLE = (
    "/nix/store/00000000000000000000000000000000-nixspace/bin/nixspace"
)


def evaluate(expression: str) -> dict[str, object]:
    result = subprocess.run(
        [
            "nix",
            "eval",
            "--offline",
            "--impure",
            "--json",
            "--expr",
            expression,
        ],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise AssertionError(result.stderr)
    return json.loads(result.stdout)


def parse_action_plan(script: str) -> dict[str, object]:
    invocation = shlex.split(script)
    if invocation[:3] != [NIXSPACE_EXECUTABLE, "_run-task", "--plan-json"]:
        raise AssertionError(f"unexpected task invocation: {invocation!r}")
    if len(invocation) != 4:
        raise AssertionError(f"task invocation is not one command: {invocation!r}")
    return json.loads(invocation[3])


class CognipilotDevenvTaskTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.tasks = evaluate(
            f"""
            let
              pkgs = import <nixpkgs> {{ }};
              lib = pkgs.lib;
              lock = builtins.fromJSON (builtins.readFile {ROOT / "flake.lock"});
              pinnedSource = (builtins.fetchTree lock.nodes.devenv.locked).outPath;
              pinnedTasksModule = pinnedSource + "/src/modules/tasks.nix";
              generated = (import {GENERATOR} {{ inherit lib; }}) {{
                index = import {INDEX};
                nixspaceExecutable = "{NIXSPACE_EXECUTABLE}";
                taskStateRoot = ".state/tasks";
                workspaceRoot = "/workspace";
                sourceBindings.alpha-source = "/checkouts/alpha";
              }};
              support = {{ lib, ... }}: {{
                options = {{
                  assertions = lib.mkOption {{
                    type = lib.types.listOf lib.types.anything;
                    default = [ ];
                  }};
                  env = lib.mkOption {{
                    type = lib.types.attrsOf lib.types.anything;
                    default = {{ }};
                  }};
                  infoSections = lib.mkOption {{
                    type = lib.types.attrsOf lib.types.anything;
                    default = {{ }};
                  }};
                  enterShell = lib.mkOption {{
                    type = lib.types.lines;
                    default = "";
                  }};
                  enterTest = lib.mkOption {{
                    type = lib.types.lines;
                    default = "";
                  }};
                  processes = lib.mkOption {{
                    type = lib.types.attrs;
                    default = {{ }};
                  }};
                  process.manager.implementation = lib.mkOption {{
                    type = lib.types.str;
                    default = "native";
                  }};
                  devenv.cli.version = lib.mkOption {{
                    type = lib.types.nullOr lib.types.str;
                    default = "2.1.2";
                  }};
                  devenv.dotfile = lib.mkOption {{
                    type = lib.types.str;
                    default = ".devenv";
                  }};
                  devenv.runtime = lib.mkOption {{
                    type = lib.types.str;
                    default = ".devenv/run";
                  }};
                }};
              }};
              evaluated = lib.evalModules {{
                specialArgs = {{ inherit pkgs; }};
                modules = [ support pinnedTasksModule {{ tasks = generated; }} ];
              }};
              cognipilotTasks = lib.filterAttrs
                (name: _: !(lib.hasPrefix "devenv:" name))
                evaluated.config.tasks;
            in
            builtins.mapAttrs (_: task: {{
              inherit (task) after cwd exec execIfModified input;
            }}) cognipilotTasks
            """
        )

    def test_stable_names_and_dependency_edges_validate_with_pinned_devenv(
        self,
    ) -> None:
        self.assertEqual(
            sorted(self.tasks),
            [
                "alpha:default:build",
                "alpha:default:docs",
                "alpha:default:generate",
                "alpha:default:test",
                "beta:default:build",
                "beta:default:test",
                "firmware:default:build",
                "qualification:default:test",
                "web:default:build",
                "web:default:test",
            ],
        )
        self.assertEqual(
            self.tasks["alpha:default:generate"]["after"],
            ["alpha:default:build"],
        )
        self.assertEqual(
            self.tasks["beta:default:build"]["after"],
            ["alpha:default:build"],
        )
        self.assertEqual(
            self.tasks["beta:default:test"]["after"], ["beta:default:build"]
        )

    def test_task_input_is_canonical_and_locks_are_explicit_runtime_paths(self) -> None:
        task = self.tasks["alpha:default:build"]
        task_input = task["input"]

        self.assertEqual(task_input["packageId"], "alpha")
        self.assertEqual(task_input["targetId"], "default")
        self.assertEqual(task_input["actionId"], "build")
        self.assertEqual(task_input["adapter"], "cargo-v1")
        self.assertEqual(task_input["environment"], {})
        self.assertEqual(task_input["environmentPaths"], {})
        self.assertEqual(
            task_input["argv"],
            [
                "nix",
                "develop",
                "--no-pure-eval",
                ".",
                "-c",
                "cargo",
                "build",
                "--workspace",
            ],
        )
        self.assertEqual(task_input["source"]["coordinate"], "alpha-source:.")
        self.assertEqual(task_input["source"]["localPath"], "/checkouts/alpha")
        self.assertEqual(task_input["source"]["visibility"], "private")
        self.assertEqual(task["cwd"], "/checkouts/alpha")
        self.assertEqual(task["execIfModified"], ["/checkouts/alpha"])
        self.assertEqual(
            task_input["requirements"]["exclusiveLocks"],
            ["shared-cache", "cargo-target"],
        )
        self.assertEqual(
            task_input["locks"],
            [
                ".state/tasks/locks/cargo-target.lock",
                ".state/tasks/locks/shared-cache.lock",
            ],
        )
        self.assertIn("variants", task_input)
        self.assertIn("artifacts", task_input)
        self.assertEqual(
            task_input["artifacts"],
            {
                "inputs": {},
                "outputs": {
                    "bundle": {
                        "producedBy": "build",
                        "kind": "directory",
                        "path": "dist",
                        "contract": {"name": "alpha-api", "version": 1},
                    }
                },
            },
        )
        self.assertEqual(
            task_input["outputs"],
            [
                {
                    "coordinate": "alpha:default:bundle",
                    "path": "/checkouts/alpha/dist",
                    "kind": "directory",
                    "contract": {"name": "alpha-api", "version": 1},
                    "proof": {
                        "kind": "nix-nar-sha256",
                        "argvPrefix": [
                            "nix",
                            "hash",
                            "path",
                            "--type",
                            "sha256",
                            "--sri",
                            "--",
                        ],
                    },
                }
            ],
        )
        self.assertEqual(
            parse_action_plan(task["exec"])["locks"],
            [
                ".state/tasks/locks/cargo-target.lock",
                ".state/tasks/locks/shared-cache.lock",
            ],
        )

    def test_only_declared_consumers_receive_artifact_edges_and_inputs(self) -> None:
        beta_build = self.tasks["beta:default:build"]
        beta_test = self.tasks["beta:default:test"]
        alpha_test = self.tasks["alpha:default:test"]
        alpha_docs = self.tasks["alpha:default:docs"]
        alpha_generate = self.tasks["alpha:default:generate"]

        self.assertEqual(beta_build["after"], ["alpha:default:build"])
        self.assertEqual(
            beta_build["execIfModified"],
            ["/workspace/src/beta-repo/firmware", "/checkouts/alpha/dist"],
        )
        self.assertEqual(
            beta_build["input"]["artifacts"]["inputs"]["alpha"]["consumedBy"],
            ["build"],
        )
        self.assertEqual(beta_build["input"]["artifacts"]["outputs"], {})
        self.assertEqual(beta_test["after"], ["beta:default:build"])
        self.assertEqual(beta_test["input"]["artifacts"], {"inputs": {}, "outputs": {}})
        self.assertEqual(
            alpha_test["input"]["artifacts"], {"inputs": {}, "outputs": {}}
        )
        self.assertEqual(alpha_docs["after"], [])
        self.assertEqual(
            alpha_docs["input"]["artifacts"], {"inputs": {}, "outputs": {}}
        )
        self.assertEqual(
            alpha_generate["input"]["artifacts"], {"inputs": {}, "outputs": {}}
        )

    def test_each_task_publishes_only_its_declared_outputs(self) -> None:
        build_result = parse_action_plan(self.tasks["alpha:default:build"]["exec"])[
            "result"
        ]
        docs_result = parse_action_plan(self.tasks["alpha:default:docs"]["exec"])[
            "result"
        ]
        test_result = parse_action_plan(self.tasks["alpha:default:test"]["exec"])[
            "result"
        ]
        generate_result = parse_action_plan(
            self.tasks["alpha:default:generate"]["exec"]
        )["result"]

        self.assertEqual(build_result["artifacts"]["bundle"]["producedBy"], "build")
        self.assertEqual(docs_result["artifacts"], {})
        self.assertEqual(test_result["artifacts"], {})
        self.assertEqual(generate_result["artifacts"], {})

    def test_docs_and_test_declaration_changes_do_not_touch_build_consumers(
        self,
    ) -> None:
        result = evaluate(
            f"""
            let
              pkgs = import <nixpkgs> {{ }};
              generator = import {GENERATOR} {{ lib = pkgs.lib; }};
              base = import {INDEX};
              alpha = base.projects."alpha-definition";
              target = alpha.targets.default;
              withActions = actions: base // {{
                projects = base.projects // {{
                  "alpha-definition" = alpha // {{
                    targets = alpha.targets // {{
                      default = target // {{ inherit actions; }};
                    }};
                  }};
                }};
              }};
              docsChanged = withActions (target.actions // {{
                docs = target.actions.docs // {{
                  argv = [ "changed-doc-command" ];
                }};
              }});
              testChanged = withActions (target.actions // {{
                test = target.actions.test // {{
                  requirements = target.actions.test.requirements // {{
                    memoryMiB = 2048;
                  }};
                }};
              }});
              tasksFor = index: generator {{
                inherit index;
                nixspaceExecutable = "{NIXSPACE_EXECUTABLE}";
                taskStateRoot = ".state/tasks";
                workspaceRoot = "/workspace";
                sourceBindings.alpha-source = "/checkouts/alpha";
              }};
              baseTasks = tasksFor base;
              docsTasks = tasksFor docsChanged;
              testTasks = tasksFor testChanged;
            in {{
              buildUnaffectedByDocs =
                baseTasks."alpha:default:build" == docsTasks."alpha:default:build";
              consumerUnaffectedByDocs =
                baseTasks."beta:default:build" == docsTasks."beta:default:build";
              buildUnaffectedByTest =
                baseTasks."alpha:default:build" == testTasks."alpha:default:build";
              consumerUnaffectedByTest =
                baseTasks."beta:default:build" == testTasks."beta:default:build";
              docsChanged =
                baseTasks."alpha:default:docs" != docsTasks."alpha:default:docs";
              testChanged =
                baseTasks."alpha:default:test" != testTasks."alpha:default:test";
            }}
            """
        )

        self.assertEqual(
            result,
            {
                "buildUnaffectedByDocs": True,
                "consumerUnaffectedByDocs": True,
                "buildUnaffectedByTest": True,
                "consumerUnaffectedByTest": True,
                "docsChanged": True,
                "testChanged": True,
            },
        )

    def test_shared_commands_and_output_publication_are_fixed(self) -> None:
        commands = {
            "alpha:default:build": [
                "nix",
                "develop",
                "--no-pure-eval",
                ".",
                "-c",
                "cargo",
                "build",
                "--workspace",
            ],
            "alpha:default:test": [
                "nix",
                "develop",
                "--no-pure-eval",
                ".",
                "-c",
                "cargo",
                "test",
                "--workspace",
            ],
            "beta:default:build": [
                "nix",
                "develop",
                "--no-pure-eval",
                ".",
                "-c",
                "cmake",
                "--build",
                "build",
                {
                    "path": "/checkouts/alpha/dist",
                    "prefix": "-DALPHA_BUNDLE=",
                    "suffix": "",
                },
            ],
            "beta:default:test": [
                "nix",
                "develop",
                "--no-pure-eval",
                ".",
                "-c",
                "ctest",
                "--test-dir",
                "build",
            ],
            "web:default:build": [
                "nix",
                "develop",
                "--no-pure-eval",
                ".",
                "-c",
                "npm",
                "run",
                "build",
            ],
            "web:default:test": [
                "nix",
                "develop",
                "--no-pure-eval",
                ".",
                "-c",
                "npm",
                "test",
            ],
            "firmware:default:build": [
                "nix",
                "develop",
                "--no-pure-eval",
                ".",
                "-c",
                "west",
                "build",
            ],
            "qualification:default:test": [
                "nix",
                "develop",
                "--no-pure-eval",
                ".",
                "-c",
                "west",
                "twister",
            ],
        }
        for task_name, argv in commands.items():
            with self.subTest(task=task_name):
                script = self.tasks[task_name]["exec"]
                plan = parse_action_plan(script)
                self.assertEqual(plan["apiVersion"], "nixspace/v1")
                self.assertEqual(plan["kind"], "ActionTask")
                self.assertEqual(plan["interfaceVersion"], 3)
                self.assertEqual(plan["argv"], argv)
                self.assertEqual(plan["result"]["task"], task_name)
                self.assertEqual(plan["cwd"], self.tasks[task_name]["cwd"])
                expected_environment_paths = (
                    {"ALPHA_BUNDLE": "/checkouts/alpha/dist"}
                    if task_name == "beta:default:build"
                    else {}
                )
                self.assertEqual(plan["environment"], {})
                self.assertEqual(plan["environmentPaths"], expected_environment_paths)
                expected_locks = {
                    "alpha:default:build": [
                        ".state/tasks/locks/cargo-target.lock",
                        ".state/tasks/locks/shared-cache.lock",
                    ],
                    "firmware:default:build": [
                        ".state/tasks/locks/west-workspace.lock"
                    ],
                }.get(task_name, [])
                self.assertEqual(plan["locks"], expected_locks)
                self.assertEqual(
                    plan["outputs"], self.tasks[task_name]["input"]["outputs"]
                )
                generation = plan["generation"]
                self.assertEqual(
                    generation["root"],
                    ".state/tasks/devel/"
                    + hashlib.sha256(task_name.encode()).hexdigest(),
                )
                self.assertEqual(
                    generation, self.tasks[task_name]["input"]["generation"]
                )
                self.assertEqual(
                    generation["layout"],
                    {
                        "apiVersion": "nixspace/v1",
                        "kind": "ActionGenerationLayout",
                        "interfaceVersion": 1,
                        "publicationLock": ".publish.lock",
                        "generations": "generations",
                        "pointer": "current",
                        "manifest": "manifest.json",
                    },
                )
                declaration = dict(self.tasks[task_name]["input"])
                declaration.pop("generation")
                self.assertEqual(
                    generation["identity"],
                    {
                        "apiVersion": "nixspace/v1",
                        "kind": "ActionTaskIdentity",
                        "interfaceVersion": 1,
                        "declaration": declaration,
                    },
                )
                self.assertEqual(
                    self.tasks[task_name]["input"]["environment"],
                    {},
                )
                self.assertEqual(
                    self.tasks[task_name]["input"]["environmentPaths"],
                    expected_environment_paths,
                )
                self.assertNotIn("DEVENV_TASK_OUTPUT_FILE", script)
                self.assertNotIn("\n", script)

    def test_bespoke_argv_is_shell_escaped_without_interpolation(self) -> None:
        script = self.tasks["alpha:default:generate"]["exec"]
        plan = parse_action_plan(script)

        self.assertEqual(
            plan["argv"],
            [
                "cargo",
                "xtask",
                "generate; touch /tmp/not-allowed",
                "$(not-allowed)",
            ],
        )
        self.assertEqual(
            plan["environment"],
            {"DANGEROUS_LITERAL": "$(not-expanded); still literal"},
        )
        self.assertEqual(plan["environmentPaths"], {})
        self.assertEqual(
            self.tasks["alpha:default:generate"]["input"]["environment"],
            plan["environment"],
        )

    def test_artifact_environment_path_is_workspace_root_relative_when_unbound(
        self,
    ) -> None:
        result = evaluate(
            f"""
            let
              pkgs = import <nixpkgs> {{ }};
              tasks = (import {GENERATOR} {{ lib = pkgs.lib; }}) {{
                index = import {INDEX};
                nixspaceExecutable = "{NIXSPACE_EXECUTABLE}";
                taskStateRoot = ".nixspace/state/tasks";
                workspaceRoot = ".";
              }};
            in {{
              exec = tasks."beta:default:build".exec;
              input = tasks."beta:default:build".input;
            }}
            """
        )
        plan = parse_action_plan(result["exec"])

        self.assertEqual(
            plan["environmentPaths"],
            {"ALPHA_BUNDLE": "src/alpha-repo/dist"},
        )
        self.assertEqual(plan["cwd"], "src/beta-repo/firmware")
        self.assertEqual(plan["outputs"], [])
        self.assertEqual(result["input"]["environmentPaths"], plan["environmentPaths"])

    def test_task_state_root_must_be_a_portable_workspace_relative_path(self) -> None:
        with self.assertRaisesRegex(
            AssertionError,
            "taskStateRoot must be a portable workspace-relative path",
        ):
            evaluate(
                f"""
                let
                  pkgs = import <nixpkgs> {{ }};
                  tasks = (import {GENERATOR} {{ lib = pkgs.lib; }}) {{
                    index = import {INDEX};
                    nixspaceExecutable = "{NIXSPACE_EXECUTABLE}";
                    taskStateRoot = "/absolute/state";
                    workspaceRoot = "/workspace";
                  }};
                in tasks."alpha:default:build".exec
                """
            )

    def test_versioned_tool_profiles_resolve_to_exact_nix_store_inputs(self) -> None:
        profiles = evaluate(
            f"""
            let
              pkgs = import <nixpkgs> {{ }};
              providers = import {TOOL_PROFILES} {{ inherit (pkgs) lib; }};
            in builtins.mapAttrs (_: provider: provider pkgs) providers
            """
        )

        self.assertEqual(
            set(profiles),
            {
                "clang-tools-v1",
                "cmake-v1",
                "fastdyn-qemu-v1",
                "meson-glib-cjson-v1",
                "rust-libudev-sccache-v1",
            },
        )
        for profile in profiles.values():
            self.assertTrue(profile["pathPrefixes"])
            self.assertTrue(
                all(
                    prefix.startswith("/nix/store/") and prefix.endswith("/bin")
                    for prefix in profile["pathPrefixes"]
                )
            )
        sccache = profiles["rust-libudev-sccache-v1"]
        self.assertIn("PKG_CONFIG_PATH", sccache["environment"])
        self.assertTrue(sccache["environment"]["RUSTC_WRAPPER"].endswith("/bin/sccache"))
        self.assertEqual("0", sccache["environment"]["CARGO_INCREMENTAL"])
        self.assertEqual(
            sccache["environmentPaths"],
            {"SCCACHE_DIR": ".nixspace/state/sccache"},
        )
        self.assertIn(
            "PKG_CONFIG_PATH", profiles["meson-glib-cjson-v1"]["environment"]
        )
        fastdyn = profiles["fastdyn-qemu-v1"]
        self.assertGreaterEqual(len(fastdyn["pathPrefixes"]), 18)
        self.assertIn("distlib", fastdyn["environment"]["PYTHONPATH"])
        self.assertIn("zlib", fastdyn["environment"]["LD_LIBRARY_PATH"])

    def test_generator_embeds_only_the_resolved_tool_profile_data(self) -> None:
        result = evaluate(
            f"""
            let
              pkgs = import <nixpkgs> {{ }};
              lib = pkgs.lib;
              index = lib.recursiveUpdate (import {INDEX}) {{
                projects."alpha-definition".targets.default.actions.build.toolProfile = "fixture-v1";
              }};
              tasks = (import {GENERATOR} {{ inherit lib; }}) {{
                inherit index;
                nixspaceExecutable = "{NIXSPACE_EXECUTABLE}";
                taskStateRoot = ".state/tasks";
                workspaceRoot = "/workspace";
                toolProfiles.fixture-v1 = {{
                  pathPrefixes = [
                    "/nix/store/00000000000000000000000000000000-compiler/bin"
                    "/nix/store/11111111111111111111111111111111-formatter/bin"
                  ];
                  environment.PKG_CONFIG_PATH =
                    "/nix/store/22222222222222222222222222222222-library/lib/pkgconfig";
                  environmentPaths.SCCACHE_DIR = ".state/sccache";
                }};
              }};
            in {{
              exec = tasks."alpha:default:build".exec;
              input = tasks."alpha:default:build".input;
            }}
            """
        )
        plan = parse_action_plan(result["exec"])

        self.assertEqual(result["input"]["toolProfileId"], "fixture-v1")
        self.assertEqual(plan["pathPrefixes"], result["input"]["pathPrefixes"])
        self.assertEqual(
            plan["pathPrefixes"],
            [
                "/nix/store/00000000000000000000000000000000-compiler/bin",
                "/nix/store/11111111111111111111111111111111-formatter/bin",
            ],
        )
        self.assertEqual(
            plan["environment"],
            {
                "PKG_CONFIG_PATH": "/nix/store/22222222222222222222222222222222-library/lib/pkgconfig"
            },
        )
        self.assertEqual(
            plan["environmentPaths"],
            {"SCCACHE_DIR": ".state/sccache"},
        )
        self.assertEqual(
            result["input"]["environmentPaths"], plan["environmentPaths"]
        )

    def test_flake_parts_module_exposes_root_owned_source_binding(self) -> None:
        result = evaluate(
            f"""
            let
              pkgs = import <nixpkgs> {{ }};
              lib = pkgs.lib;
              support = {{ lib, ... }}: {{
                options.cognipilot.validatedIndex = lib.mkOption {{
                  type = lib.types.attrs;
                }};
                options.perSystem = lib.mkOption {{ type = lib.types.raw; }};
              }};
              evaluated = lib.evalModules {{
                modules = [
                  support
                  {MODULE}
                  {{
                    cognipilot.validatedIndex = import {INDEX};
                    cognipilot.devenvTasks = {{
                      enable = true;
                      workspaceRoot = "/workspace";
                      taskStateRoot = "task-state";
                      sourceBindings.alpha-source = "/checkouts/alpha";
                    }};
                  }}
                ];
              }};
              nixspace = pkgs.writeShellScriptBin "nixspace" "";
              tasks = (evaluated.config.perSystem {{
                inherit pkgs;
                config.packages.nixspace = nixspace;
              }}).devenv.shells.default.tasks;
            in
            {{
              alpha = tasks."alpha:default:build".cwd;
              alphaExec = tasks."alpha:default:build".exec;
              alphaLocks = tasks."alpha:default:build".input.locks;
              beta = tasks."beta:default:build".cwd;
              names = builtins.attrNames tasks;
            }}
            """
        )

        self.assertEqual(result["alpha"], "/checkouts/alpha")
        self.assertEqual(result["beta"], "/workspace/src/beta-repo/firmware")
        self.assertEqual(
            result["alphaLocks"],
            [
                "task-state/locks/cargo-target.lock",
                "task-state/locks/shared-cache.lock",
            ],
        )
        self.assertIn("alpha:default:build", result["names"])
        module_invocation = shlex.split(result["alphaExec"])
        self.assertTrue(module_invocation[0].startswith("/nix/store/"))
        self.assertTrue(module_invocation[0].endswith("/bin/nixspace"))
        self.assertEqual(module_invocation[1:3], ["_run-task", "--plan-json"])


if __name__ == "__main__":
    unittest.main()
