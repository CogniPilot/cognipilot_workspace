#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    index: PathBuf,
    resolution: PathBuf,
    helper: PathBuf,
    record: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("nixspace-action-{}-{sequence}", std::process::id()));
        fs::create_dir(&root).expect("fixture root is created");
        fs::create_dir(root.join("project")).expect("action cwd is created");
        let helper = root.join("action-helper");
        fs::write(
            &helper,
            r#"#!/bin/sh
{
  printf 'cwd=%s\n' "$PWD"
  printf 'env=%s\n' "${ACTION_TEST_VALUE-unset}"
  printf 'relative-path=%s\n' "${ACTION_TEST_RELATIVE_PATH-unset}"
  printf 'absolute-path=%s\n' "${ACTION_TEST_ABSOLUTE_PATH-unset}"
  printf 'arg=%s\n' "$@"
} > "$ACTION_TEST_RECORD"
if [ -n "${ACTION_TEST_RELEASE_FILE:-}" ]; then
  while [ ! -e "$ACTION_TEST_RELEASE_FILE" ]; do
    sleep 0.01
  done
fi
printf 'runner stdout\n'
printf 'runner stderr\n' >&2
exit "${ACTION_TEST_EXIT:-0}"
"#,
        )
        .expect("helper is written");
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755))
            .expect("helper is executable");
        let record = root.join("record");
        let index = root.join("index.json");
        fs::write(
            &index,
            serde_json::to_vec(&workspace_index(&helper)).expect("index serializes"),
        )
        .expect("index is written");
        let resolution = root.join("resolution.json");
        fs::write(
            &resolution,
            serde_json::to_vec(&resolution_plan()).expect("resolution serializes"),
        )
        .expect("resolution is written");
        Self {
            root,
            index,
            resolution,
            helper,
            record,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_nixspace"));
        command
            .arg("--workspace-root")
            .arg(&self.root)
            .arg("--index")
            .arg(&self.index)
            .arg("--resolution-plan")
            .arg(&self.resolution)
            .env("ACTION_TEST_RECORD", &self.record)
            .env("DEVENV_TASK_FILE", "fixture-tasks.json")
            .env("NIXSPACE_INDEX", "fixture-index.json")
            .env("NIXSPACE_WORKSPACE_ROOT", &self.root)
            .env_remove("ACTION_TEST_EXIT")
            .env_remove("ACTION_TEST_VALUE")
            .env_remove("DEVENV_TASK_OUTPUT_FILE");
        command
    }

    fn run(&self, arguments: &[&str]) -> Output {
        self.command()
            .args(arguments)
            .output()
            .expect("nixspace starts")
    }
}

fn resolution_plan() -> Value {
    let package = |id: &str| {
        json!({
            "packageId": id,
            "candidates": {
                "local": {
                    "kind": "local-worktree",
                    "prefix": {
                        "kind": "source-relative",
                        "sourceInput": format!("{id}-source"),
                        "repositoryId": format!("{id}-repository"),
                        "sourceRoot": ".",
                        "workspacePath": "project"
                    },
                    "provenance": {
                        "kind": "local-git",
                        "cleanLabel": "working tree clean",
                        "dirtyLabel": "working tree modified",
                        "sourceInput": format!("{id}-source"),
                        "inspection": {
                            "workingDirectory": "project",
                            "dirty": {"argv": ["git", "status"], "cleanWhen": "stdout-empty"},
                            "revision": {"argv": ["git", "rev-parse", "HEAD"]}
                        }
                    }
                },
                "locked": null
            }
        })
    };
    let plan = |id: &str| {
        json!({
            "packageId": id,
            "dependencyClosure": [id],
            "reverseClosure": [id],
            "compiledReverseClosure": [id],
            "selectedScope": "local",
            "commandScopes": {
                "local": {
                    "commands": ["build", "test"],
                    "selectedCandidates": {(id): "local"},
                    "override": {
                        "refused": false,
                        "blockedCommands": [],
                        "affectedReverseClosure": [id],
                        "requiredRebuild": [],
                        "refusalReason": null
                    }
                },
                "locked": null
            }
        })
    };
    json!({
        "apiVersion": "nixspace/v1",
        "kind": "WorkspaceResolution",
        "interfaceVersion": 2,
        "roots": {"workspace": ".", "taskState": ".state"},
        "packagePlans": {"app": plan("app"), "library": plan("library")},
        "packages": {"app": package("app"), "library": package("library")},
        "artifacts": {},
        "resources": {},
        "executables": {},
        "actionBindings": {}
    })
}

#[test]
fn build_refuses_the_exact_blocked_resolution_before_starting_runner() {
    let fixture = Fixture::new();
    let mut plan: Value =
        serde_json::from_slice(&fs::read(&fixture.resolution).expect("resolution is readable"))
            .expect("resolution is JSON");
    let policy = &mut plan["packagePlans"]["app"]["commandScopes"]["local"]["override"];
    policy["refused"] = json!(true);
    policy["blockedCommands"] = json!(["build"]);
    policy["requiredRebuild"] = json!(["app"]);
    policy["refusalReason"] = json!("compiled reverse closure is locked");
    fs::write(&fixture.resolution, serde_json::to_vec(&plan).unwrap()).unwrap();

    let output = fixture.run(&["build", "application"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("compiled reverse closure"));
    assert!(
        !fixture.record.exists(),
        "runner must not start after refusal"
    );
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn workspace_index(helper: &Path) -> Value {
    json!({
        "apiVersion": "nixspace/v1",
        "kind": "Workspace",
        "interfaceVersion": 2,
        "catalog": {
            "packages": [
                package("app", &["application"]),
                package("library", &[])
            ],
            "targets": [],
            "artifacts": [],
            "resources": [],
            "executables": [],
            "launches": []
        },
        "graph": {
            "schemaVersion": 1,
            "all": {"nodes": [], "edges": []},
            "packages": {
                "app": {"nodes": [], "edges": []},
                "library": {"nodes": [], "edges": []}
            },
            "reverse": {
                "app": {"nodes": [], "edges": []},
                "library": {"nodes": [], "edges": []}
            }
        },
        "launchPlans": {},
        "actionPlans": {
            "schemaVersion": 1,
            "runner": {
                "kind": "devenv-task",
                "direct": {
                    "argv": [helper, "fixed-runner-argument"],
                    "requiredEnvironment": ["DEVENV_TASK_FILE", "NIXSPACE_INDEX", "NIXSPACE_WORKSPACE_ROOT"]
                },
                "bootstrap": {"argv": [helper, "bootstrap-runner-argument"]}
            },
            "actions": {
                "build": {
                    "all": ["app:default:build", "library:core:build"],
                    "packages": {
                        "app": ["app:default:build"],
                        "library": ["library:core:build"]
                    }
                },
                "test": {
                    "all": ["app:default:test"],
                    "packages": {"app": ["app:default:test"], "library": []}
                }
            }
        }
    })
}

fn package(id: &str, aliases: &[&str]) -> Value {
    json!({
        "id": id,
        "aliases": aliases,
        "extensions": {}
    })
}

fn action_task(helper: &Path, result: Value) -> Value {
    json!({
        "apiVersion": "nixspace/v1",
        "kind": "ActionTask",
        "interfaceVersion": 3,
        "cwd": "project",
        "argv": [
            helper,
            "literal;argument",
            "$(must-not-run)",
            {"path": "artifacts/app.bin", "prefix": "--input=", "suffix": ".checked"}
        ],
        "environment": {
            "ACTION_TEST_VALUE": "exact value $HOME",
            "ACTION_TEST_EXIT": "0"
        },
        "environmentPaths": {
            "ACTION_TEST_RELATIVE_PATH": "artifacts/app.bin",
            "ACTION_TEST_ABSOLUTE_PATH": "/external/artifacts/schema.bin"
        },
        "pathPrefixes": [],
        "locks": ["state/locks/action.lock"],
        "outputs": [],
        "generation": generation_store(),
        "result": result
    })
}

fn generation_store() -> Value {
    json!({
        "apiVersion": "nixspace/v1",
        "kind": "ActionGenerationStore",
        "interfaceVersion": 2,
        "root": "state/devel/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "layout": {
            "apiVersion": "nixspace/v1",
            "kind": "ActionGenerationLayout",
            "interfaceVersion": 1,
            "publicationLock": "locks/publication.lock",
            "generations": "records",
            "pointer": "selected.json",
            "manifest": "record.json"
        },
        "identity": {
            "apiVersion": "nixspace/v1",
            "kind": "ActionTaskIdentity",
            "interfaceVersion": 1,
            "declaration": {"fixture": "action-task"}
        }
    })
}

fn generation_root(fixture: &Fixture, task: &Value) -> PathBuf {
    fixture
        .root
        .join(task["generation"]["root"].as_str().unwrap())
}

fn active_generation(fixture: &Fixture, task: &Value) -> (Value, Value) {
    let root = generation_root(fixture, task);
    let current: Value = serde_json::from_slice(
        &fs::read(root.join(task["generation"]["layout"]["pointer"].as_str().unwrap())).unwrap(),
    )
    .unwrap();
    let manifest: Value = serde_json::from_slice(
        &fs::read(root.join(current["manifest"].as_str().unwrap())).unwrap(),
    )
    .unwrap();
    (current, manifest)
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn build_canonicalizes_alias_and_invokes_only_the_exact_emitted_task_roots() {
    let fixture = Fixture::new();
    let output = fixture.run(&["build", "application"]);
    assert_success(&output);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "runner stdout\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "runner stderr\n");
    assert_eq!(
        fs::read_to_string(&fixture.record).unwrap(),
        format!(
            concat!(
                "cwd={}\n",
                "env=unset\n",
                "relative-path=unset\n",
                "absolute-path=unset\n",
                "arg=fixed-runner-argument\n",
                "arg=app:default:build\n"
            ),
            fixture.root.display()
        )
    );
}

#[test]
fn unqualified_build_uses_the_exact_all_list_and_propagates_runner_status() {
    let fixture = Fixture::new();
    let mut command = fixture.command();
    let output = command
        .env("ACTION_TEST_EXIT", "23")
        .arg("build")
        .output()
        .expect("nixspace starts");
    assert_eq!(output.status.code(), Some(23));
    assert_eq!(
        fs::read_to_string(&fixture.record).unwrap(),
        format!(
            concat!(
                "cwd={}\n",
                "env=unset\n",
                "relative-path=unset\n",
                "absolute-path=unset\n",
                "arg=fixed-runner-argument\n",
                "arg=app:default:build\n",
                "arg=library:core:build\n"
            ),
            fixture.root.display()
        )
    );
}

#[test]
fn plan_json_renders_nix_selection_without_starting_the_runner() {
    let fixture = Fixture::new();
    let output = fixture.run(&["test", "application", "--plan", "--json"]);
    assert_success(&output);
    assert!(!fixture.record.exists());
    let plan: Value = serde_json::from_slice(&output.stdout).expect("plan is JSON");
    assert_eq!(plan["apiVersion"], "nixspace/v1");
    assert_eq!(plan["kind"], "ActionInvocation");
    assert_eq!(plan["action"], "test");
    assert_eq!(plan["package"], "app");
    assert_eq!(plan["runner"]["kind"], "devenv-task");
    assert_eq!(plan["tasks"], json!(["app:default:test"]));
}

#[test]
fn action_selection_rejects_unknown_packages_and_empty_declared_selections() {
    let fixture = Fixture::new();
    let missing = fixture.run(&["build", "missing"]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr)
        .contains("package `missing` is not in the cached workspace index"));

    let empty = fixture.run(&["test", "library"]);
    assert!(!empty.status.success());
    assert!(String::from_utf8_lossy(&empty.stderr)
        .contains("no `test` tasks are declared for package `library`"));
    assert!(!fixture.record.exists());
}

#[test]
fn run_task_executes_exact_argv_cwd_and_environment_then_atomically_writes_result() {
    let fixture = Fixture::new();
    let result = json!({
        "task": "app:default:build",
        "artifacts": {"binary": "dist/app"}
    });
    let task = action_task(&fixture.helper, result);
    let plan = serde_json::to_string(&task).unwrap();
    let output_path = fixture.root.join("task-result.json");
    let mut command = fixture.command();
    let output = command
        .env("DEVENV_TASK_OUTPUT_FILE", &output_path)
        .args(["_run-task", "--plan-json", &plan])
        .output()
        .expect("nixspace starts");
    assert_success(&output);
    assert_eq!(
        fs::read_to_string(&fixture.record).unwrap(),
        format!(
            concat!(
                "cwd={}/project\n",
                "env=exact value $HOME\n",
                "relative-path={}/artifacts/app.bin\n",
                "absolute-path=/external/artifacts/schema.bin\n",
                "arg=literal;argument\n",
                "arg=$(must-not-run)\n",
                "arg=--input={}/artifacts/app.bin.checked\n"
            ),
            fixture.root.display(),
            fixture.root.display(),
            fixture.root.display()
        )
    );
    assert_eq!(
        fs::read_to_string(&output_path).unwrap(),
        "{\"artifacts\":{\"binary\":\"dist/app\"},\"outputs\":[],\"task\":\"app:default:build\"}\n"
    );
    assert!(fixture.root.join("state/locks/action.lock").is_file());
    let (current, manifest) = active_generation(&fixture, &task);
    assert_eq!(current["apiVersion"], "nixspace/v1");
    assert_eq!(current["kind"], "ActionGenerationPointer");
    assert_eq!(current["interfaceVersion"], 1);
    assert_eq!(current["identity"], task["generation"]["identity"]);
    assert_eq!(manifest["kind"], "ActionGeneration");
    assert_eq!(manifest["generation"], current["generation"]);
    assert_eq!(manifest["identity"]["declared"], current["identity"]);
    assert_eq!(manifest["result"]["outputs"], json!([]));
    assert_eq!(manifest["execution"]["exitStatus"], 0);
    assert!(fs::read_dir(&fixture.root).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".tmp")
    }));
}

#[test]
fn run_task_prepends_only_the_nix_generated_tool_paths() {
    let fixture = Fixture::new();
    let profile = fixture.root.join("profile/bin");
    fs::create_dir_all(&profile).unwrap();
    let executable = profile.join("profile-action");
    fs::copy(&fixture.helper, &executable).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();

    let mut task = action_task(&fixture.helper, json!({"task": "profile"}));
    task["argv"] = json!(["profile-action", "from-profile"]);
    task["pathPrefixes"] = json!([profile]);
    let output_path = fixture.root.join("profile-result.json");
    let output = fixture
        .command()
        .env("DEVENV_TASK_OUTPUT_FILE", &output_path)
        .args(["_run-task", "--plan-json", &task.to_string()])
        .output()
        .unwrap();

    assert_success(&output);
    assert_eq!(
        fs::read_to_string(&fixture.record).unwrap(),
        format!(
            concat!(
                "cwd={}/project\n",
                "env=exact value $HOME\n",
                "relative-path={}/artifacts/app.bin\n",
                "absolute-path=/external/artifacts/schema.bin\n",
                "arg=from-profile\n"
            ),
            fixture.root.display(),
            fixture.root.display()
        )
    );
}

#[test]
fn run_task_accepts_plan_file_and_never_publishes_a_failed_result() {
    let fixture = Fixture::new();
    let successful = action_task(&fixture.helper, json!({"sequence": "prior"}));
    let successful_output = fixture.root.join("successful-result.json");
    let first = fixture
        .command()
        .env("DEVENV_TASK_OUTPUT_FILE", &successful_output)
        .args(["_run-task", "--plan-json", &successful.to_string()])
        .output()
        .unwrap();
    assert_success(&first);
    let generation_root = generation_root(&fixture, &successful);
    let layout = successful["generation"]["layout"].clone();
    let prior_current =
        fs::read(generation_root.join(layout["pointer"].as_str().unwrap())).unwrap();
    let prior_generation_count =
        fs::read_dir(generation_root.join(layout["generations"].as_str().unwrap()))
            .unwrap()
            .count();

    let mut task = successful;
    task["result"] = json!({"must": "not be written"});
    task["environment"]["ACTION_TEST_EXIT"] = json!("17");
    let plan_path = fixture.root.join("action-task.json");
    fs::write(&plan_path, serde_json::to_vec(&task).unwrap()).unwrap();
    let output_path = fixture.root.join("failed-result.json");
    let mut command = fixture.command();
    let output = command
        .env("DEVENV_TASK_OUTPUT_FILE", &output_path)
        .arg("_run-task")
        .arg("--plan-file")
        .arg(&plan_path)
        .output()
        .expect("nixspace starts");
    assert_eq!(output.status.code(), Some(17));
    assert!(!output_path.exists());
    assert_eq!(
        fs::read(generation_root.join(layout["pointer"].as_str().unwrap())).unwrap(),
        prior_current
    );
    assert_eq!(
        fs::read_dir(generation_root.join(layout["generations"].as_str().unwrap()))
            .unwrap()
            .count(),
        prior_generation_count
    );
}

#[test]
fn run_task_validates_then_hashes_every_declared_output_and_publishes_proofs() {
    let fixture = Fixture::new();
    let artifacts = fixture.root.join("artifacts");
    fs::create_dir_all(artifacts.join("tree")).unwrap();
    fs::write(artifacts.join("file.txt"), "file").unwrap();
    fs::write(artifacts.join("program"), "program").unwrap();
    fs::set_permissions(artifacts.join("program"), fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink("program", artifacts.join("program-link")).unwrap();
    let hasher = fixture.root.join("nix-hash");
    let hash_log = fixture.root.join("hash.log");
    fs::write(
        &hasher,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$ACTION_TEST_HASH_LOG\"\nprintf 'sha256-proof-%s\\n' \"$(basename \"$7\")\"\n",
    )
    .unwrap();
    fs::set_permissions(&hasher, fs::Permissions::from_mode(0o755)).unwrap();
    let proof = |coordinate: &str, path: &str, kind: &str| {
        json!({
            "coordinate": coordinate,
            "path": path,
            "kind": kind,
            "contract": {"name": coordinate, "version": 1},
            "proof": {
                "kind": "nix-nar-sha256",
                "argvPrefix": [hasher, "hash", "path", "--type", "sha256", "--sri", "--"]
            }
        })
    };
    let mut task = action_task(&fixture.helper, json!({"task": "proof"}));
    task["outputs"] = json!([
        proof("app:default:file", "artifacts/file.txt", "file"),
        proof(
            "app:default:program",
            "artifacts/program-link",
            "executable"
        ),
        proof("app:default:tree", "artifacts/tree", "directory"),
        proof("app:default:link", "artifacts/program-link", "symlink")
    ]);
    // The same path cannot be declared twice even with different kinds.
    task["outputs"][1]["path"] = json!("artifacts/program");
    let output_path = fixture.root.join("proof-result.json");
    let output = fixture
        .command()
        .env("ACTION_TEST_HASH_LOG", &hash_log)
        .env("DEVENV_TASK_OUTPUT_FILE", &output_path)
        .args(["_run-task", "--plan-json", &task.to_string()])
        .output()
        .unwrap();
    assert_success(&output);
    let result: Value = serde_json::from_slice(&fs::read(&output_path).unwrap()).unwrap();
    let outputs = result["outputs"].as_array().unwrap();
    assert_eq!(outputs.len(), 4);
    assert_eq!(outputs[0]["proof"]["kind"], "nix-nar-sha256");
    assert_eq!(outputs[0]["proof"]["digest"], "sha256-proof-file.txt");
    assert_eq!(outputs[1]["mode"], 0o755);
    assert_eq!(outputs[3]["symlinkTarget"], "program");
    assert_eq!(fs::read_to_string(&hash_log).unwrap().lines().count(), 4);
    let generation_root = generation_root(&fixture, &task);
    let layout = task["generation"]["layout"].clone();
    let current_before_rejection =
        fs::read(generation_root.join(layout["pointer"].as_str().unwrap())).unwrap();
    let generations_before_rejection =
        fs::read_dir(generation_root.join(layout["generations"].as_str().unwrap()))
            .unwrap()
            .count();
    let (_, manifest) = active_generation(&fixture, &task);
    assert_eq!(manifest["outputs"], result["outputs"]);
    assert_eq!(
        manifest["outputs"][0]["path"],
        artifacts.join("file.txt").display().to_string()
    );
    let active_directory = generation_root.join(
        manifest["generation"]
            .as_str()
            .map(|generation| format!("{}/{generation}", layout["generations"].as_str().unwrap()))
            .unwrap(),
    );
    assert_eq!(
        fs::read_dir(active_directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>(),
        vec![std::ffi::OsString::from(
            layout["manifest"].as_str().unwrap(),
        )],
        "generation metadata must point at stable native outputs instead of copying build trees"
    );

    fs::remove_file(&hash_log).unwrap();
    task["outputs"][0]["kind"] = json!("directory");
    let rejected = fixture
        .command()
        .env("ACTION_TEST_HASH_LOG", &hash_log)
        .env(
            "DEVENV_TASK_OUTPUT_FILE",
            fixture.root.join("rejected.json"),
        )
        .args(["_run-task", "--plan-json", &task.to_string()])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(
        !hash_log.exists(),
        "hashing began before complete validation"
    );
    assert_eq!(
        fs::read(generation_root.join(layout["pointer"].as_str().unwrap())).unwrap(),
        current_before_rejection,
        "failed validation replaced the prior active generation"
    );
    assert_eq!(
        fs::read_dir(generation_root.join(layout["generations"].as_str().unwrap()))
            .unwrap()
            .count(),
        generations_before_rejection,
        "failed validation published a generation"
    );
}

#[test]
fn run_task_rejects_invalid_contract_before_starting_the_action() {
    let fixture = Fixture::new();
    let output_path = fixture.root.join("invalid-result.json");
    let cases = [
        (
            "obsolete interface",
            {
                let mut task = action_task(&fixture.helper, json!({}));
                task["interfaceVersion"] = json!(1);
                task
            },
            "ActionTask interface version 1 is unsupported",
        ),
        (
            "absolute generation root",
            {
                let mut task = action_task(&fixture.helper, json!({}));
                task["generation"]["root"] = json!("/tmp/not-portable");
                task
            },
            "root must be a portable workspace-relative path",
        ),
        (
            "parent generation root",
            {
                let mut task = action_task(&fixture.helper, json!({}));
                task["generation"]["root"] = json!("state/../not-contained");
                task
            },
            "root must be a portable workspace-relative path",
        ),
        (
            "empty generation identity",
            {
                let mut task = action_task(&fixture.helper, json!({}));
                task["generation"]["identity"]["declaration"] = json!({});
                task
            },
            "ActionTaskIdentity declaration must be a nonempty object emitted by Nix",
        ),
        (
            "generation and output overlap",
            {
                let mut task = action_task(&fixture.helper, json!({}));
                task["outputs"] = json!([{
                    "coordinate": "fixture:default:state",
                    "path": "state",
                    "kind": "directory",
                    "contract": {"name": "fixture-state", "version": 1},
                    "proof": {
                        "kind": "nix-nar-sha256",
                        "argvPrefix": [fixture.helper]
                    }
                }]);
                task
            },
            "overlaps ActionGenerationStore root",
        ),
        (
            "wrong kind",
            json!({
                "apiVersion": "nixspace/v1", "kind": "SomethingElse", "interfaceVersion": 3,
                "cwd": "project", "argv": [fixture.helper],
                "environment": {}, "environmentPaths": {}, "pathPrefixes": [], "locks": [],
                "outputs": [],
                "generation": generation_store(), "result": {}
            }),
            "task kind `SomethingElse` is unsupported",
        ),
        (
            "empty argv",
            json!({
                "apiVersion": "nixspace/v1", "kind": "ActionTask", "interfaceVersion": 3,
                "cwd": "project", "argv": [],
                "environment": {}, "environmentPaths": {}, "pathPrefixes": [], "locks": [],
                "outputs": [],
                "generation": generation_store(), "result": {}
            }),
            "ActionTask argv must declare a nonempty executable",
        ),
        (
            "empty argv path",
            json!({
                "apiVersion": "nixspace/v1", "kind": "ActionTask", "interfaceVersion": 3,
                "cwd": "project",
                "argv": [fixture.helper, {"path": "", "prefix": "", "suffix": ""}],
                "environment": {}, "environmentPaths": {}, "pathPrefixes": [], "locks": [],
                "outputs": [],
                "generation": generation_store(), "result": {}
            }),
            "argv path at index 1 must be nonempty",
        ),
        (
            "invalid environment",
            json!({
                "apiVersion": "nixspace/v1", "kind": "ActionTask", "interfaceVersion": 3,
                "cwd": "project", "argv": [fixture.helper], "environment": {"BAD=KEY": "value"},
                "environmentPaths": {}, "pathPrefixes": [], "locks": [],
                "outputs": [], "generation": generation_store(), "result": {}
            }),
            "environment key `BAD=KEY`",
        ),
        (
            "colliding literal and path environment",
            json!({
                "apiVersion": "nixspace/v1", "kind": "ActionTask", "interfaceVersion": 3,
                "cwd": "project", "argv": [fixture.helper],
                "environment": {"COLLISION": "literal"},
                "environmentPaths": {"COLLISION": "artifact.bin"},
                "pathPrefixes": [], "locks": [],
                "outputs": [], "generation": generation_store(), "result": {}
            }),
            "declared as both a literal and a path",
        ),
        (
            "duplicate lock path",
            json!({
                "apiVersion": "nixspace/v1", "kind": "ActionTask", "interfaceVersion": 3,
                "cwd": "project", "argv": [fixture.helper],
                "environment": {}, "environmentPaths": {}, "pathPrefixes": [],
                "locks": ["state/lock", "state/lock"], "outputs": [],
                "generation": generation_store(), "result": {}
            }),
            "lock path `state/lock` is declared more than once",
        ),
        (
            "relative tool path",
            json!({
                "apiVersion": "nixspace/v1", "kind": "ActionTask", "interfaceVersion": 3,
                "cwd": "project", "argv": [fixture.helper],
                "environment": {}, "environmentPaths": {},
                "pathPrefixes": ["relative/bin"], "locks": [], "outputs": [],
                "generation": generation_store(), "result": {}
            }),
            "pathPrefixes entry at index 0 must be an absolute path",
        ),
        (
            "literal path collision",
            json!({
                "apiVersion": "nixspace/v1", "kind": "ActionTask", "interfaceVersion": 3,
                "cwd": "project", "argv": [fixture.helper],
                "environment": {"PATH": "/literal/bin"},
                "environmentPaths": {}, "pathPrefixes": ["/profile/bin"], "locks": [],
                "outputs": [],
                "generation": generation_store(), "result": {}
            }),
            "cannot combine pathPrefixes with a literal PATH",
        ),
    ];
    for (name, plan, expected) in cases {
        let mut command = fixture.command();
        let output = command
            .env("DEVENV_TASK_OUTPUT_FILE", &output_path)
            .args(["_run-task", "--plan-json", &plan.to_string()])
            .output()
            .expect("nixspace starts");
        assert!(!output.status.success(), "{name}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(!fixture.record.exists());
    assert!(!output_path.exists());
}

#[test]
fn run_task_rejects_missing_nix_normalized_fields() {
    let fixture = Fixture::new();
    let output_path = fixture.root.join("missing-field-result.json");

    for field in ["environment", "environmentPaths", "pathPrefixes", "locks"] {
        let mut task = action_task(&fixture.helper, json!({}));
        task.as_object_mut().unwrap().remove(field).unwrap();
        let output = fixture
            .command()
            .env("DEVENV_TASK_OUTPUT_FILE", &output_path)
            .args(["_run-task", "--plan-json", &task.to_string()])
            .output()
            .expect("nixspace starts");
        assert!(!output.status.success(), "missing {field} was accepted");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(&format!("missing field `{field}`")),
            "missing {field}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    for field in ["prefix", "suffix"] {
        let mut task = action_task(&fixture.helper, json!({}));
        task["argv"][3]
            .as_object_mut()
            .unwrap()
            .remove(field)
            .unwrap();
        let output = fixture
            .command()
            .env("DEVENV_TASK_OUTPUT_FILE", &output_path)
            .args(["_run-task", "--plan-json", &task.to_string()])
            .output()
            .expect("nixspace starts");
        assert!(
            !output.status.success(),
            "missing argv {field} was accepted"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("did not match any variant of untagged enum ActionArgument"),
            "missing argv {field}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert!(!fixture.record.exists());
    assert!(!output_path.exists());
}

#[test]
fn run_task_requires_devenv_output_only_after_a_successful_action() {
    let fixture = Fixture::new();
    let mut failed = action_task(&fixture.helper, json!({"task": "failed"}));
    failed["environment"]["ACTION_TEST_EXIT"] = json!("19");
    let output = fixture.run(&["_run-task", "--plan-json", &failed.to_string()]);
    assert_eq!(output.status.code(), Some(19));
    assert!(fixture.record.exists());

    fs::remove_file(&fixture.record).unwrap();
    let successful = action_task(&fixture.helper, json!({"task": "unpublished"}));
    let output = fixture.run(&["_run-task", "--plan-json", &successful.to_string()]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("DEVENV_TASK_OUTPUT_FILE"));
    assert!(fixture.record.exists());
}

#[test]
fn run_task_acquires_declared_locks_in_stable_order_and_holds_them_through_the_action() {
    let fixture = Fixture::new();
    let release = fixture.root.join("release-first-action");
    let first_record = fixture.root.join("first-record");
    let second_record = fixture.root.join("second-record");
    let first_output = fixture.root.join("first-result.json");
    let second_output = fixture.root.join("second-result.json");

    let mut first_plan = action_task(&fixture.helper, json!({"sequence": 1}));
    first_plan["environment"]["ACTION_TEST_RELEASE_FILE"] = json!(release.display().to_string());
    first_plan["locks"] = json!(["state/locks/second.lock", "state/locks/first.lock"]);
    let mut first = fixture.command();
    let first_child = first
        .env("ACTION_TEST_RECORD", &first_record)
        .env("DEVENV_TASK_OUTPUT_FILE", &first_output)
        .args(["_run-task", "--plan-json", &first_plan.to_string()])
        .spawn()
        .unwrap();

    for _ in 0..200 {
        if first_record.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        first_record.exists(),
        "first action never acquired its locks"
    );

    let mut second_plan = action_task(&fixture.helper, json!({"sequence": 2}));
    second_plan["locks"] = json!(["state/locks/first.lock", "state/locks/second.lock"]);
    let mut second = fixture.command();
    let mut second_child = second
        .env("ACTION_TEST_RECORD", &second_record)
        .env("DEVENV_TASK_OUTPUT_FILE", &second_output)
        .args(["_run-task", "--plan-json", &second_plan.to_string()])
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_millis(100));
    assert!(second_child.try_wait().unwrap().is_none());
    assert!(
        !second_record.exists(),
        "second action started without the lock"
    );

    fs::write(&release, "release").unwrap();
    assert!(first_child.wait_with_output().unwrap().status.success());
    assert!(second_child.wait_with_output().unwrap().status.success());
    assert!(second_record.exists());
    assert!(first_output.exists());
    assert!(second_output.exists());
}

#[test]
fn independent_tasks_run_concurrently_when_nix_declares_no_shared_lock() {
    let fixture = Fixture::new();
    let release = fixture.root.join("release-independent");
    let records = [
        fixture.root.join("variant-a"),
        fixture.root.join("variant-b"),
    ];
    let outputs = [fixture.root.join("result-a"), fixture.root.join("result-b")];
    let locks = ["state/locks/variant-a.lock", "state/locks/variant-b.lock"];
    let mut children = Vec::new();
    for index in 0..2 {
        let mut task = action_task(&fixture.helper, json!({"variant": index}));
        task["locks"] = json!([locks[index]]);
        task["generation"]["root"] = json!(format!("state/devel/{index:064x}"));
        task["generation"]["identity"]["declaration"] = json!({"variant": index});
        let child = fixture
            .command()
            .env("ACTION_TEST_RECORD", &records[index])
            .env("ACTION_TEST_RELEASE_FILE", &release)
            .env("DEVENV_TASK_OUTPUT_FILE", &outputs[index])
            .args(["_run-task", "--plan-json", &task.to_string()])
            .spawn()
            .unwrap();
        children.push(child);
    }
    for _ in 0..100 {
        if records.iter().all(|record| record.exists()) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        records.iter().all(|record| record.exists()),
        "independent actions did not both start while the other was running"
    );
    assert!(children
        .iter_mut()
        .all(|child| child.try_wait().unwrap().is_none()));
    fs::write(&release, "release").unwrap();
    for child in children {
        assert!(child.wait_with_output().unwrap().status.success());
    }
    assert!(outputs.iter().all(|output| output.exists()));
}
