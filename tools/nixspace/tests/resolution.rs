#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    plan: PathBuf,
    arguments: PathBuf,
    local_executable: PathBuf,
    locked_executable: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "nixspace-resolution-{}-{sequence}",
            std::process::id()
        ));
        let local_prefix = root.join("src/app");
        let local_executable = local_prefix.join("bin/app");
        let locked_prefix = root.join("locked-app");
        let locked_executable = locked_prefix.join("bin/app");
        let arguments = root.join("arguments.txt");
        fs::create_dir_all(local_prefix.join("bin")).expect("local bin exists");
        fs::create_dir_all(local_prefix.join("config")).expect("local config exists");
        fs::create_dir_all(locked_prefix.join("bin")).expect("locked bin exists");
        fs::create_dir_all(locked_prefix.join("config")).expect("locked config exists");
        fs::write(local_prefix.join("config/default.json"), "local\n")
            .expect("local resource exists");
        fs::write(locked_prefix.join("config/default.json"), "locked\n")
            .expect("locked resource exists");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\n",
            arguments.display()
        );
        for executable in [&local_executable, &locked_executable] {
            fs::write(executable, &script).expect("executable exists");
            fs::set_permissions(executable, fs::Permissions::from_mode(0o755))
                .expect("executable mode is set");
        }

        let generation_root = root.join(".state/devel/app");
        let generation = "1-2-3";
        fs::create_dir_all(generation_root.join(format!("generations/{generation}")))
            .expect("generation exists");
        fs::write(generation_root.join(".publish.lock"), "").expect("generation lock exists");
        let identity = json!({
            "apiVersion": "nixspace/v1",
            "kind": "ActionTaskIdentity",
            "interfaceVersion": 1,
            "declaration": {"task": "app:default:build"}
        });
        let proof = Command::new("sha256sum")
            .arg(&local_executable)
            .output()
            .expect("sha256sum runs");
        assert!(proof.status.success());
        let digest = String::from_utf8(proof.stdout)
            .expect("proof is UTF-8")
            .trim_end()
            .to_owned();
        let declared_output = json!({
            "coordinate": "app:default:cli",
            "path": "src/app/bin/app",
            "kind": "executable",
            "contract": {"name": "app-cli", "version": 1},
            "proof": {"kind": "sha256", "argvPrefix": ["sha256sum"]}
        });
        let resolved_output = json!({
            "coordinate": "app:default:cli",
            "path": local_executable,
            "kind": "executable",
            "contract": {"name": "app-cli", "version": 1},
            "proof": {"kind": "sha256", "digest": digest},
            "mode": 493
        });
        let manifest = json!({
            "apiVersion": "nixspace/v1",
            "kind": "ActionGeneration",
            "interfaceVersion": 1,
            "generation": generation,
            "identity": {
                "declared": identity,
                "cwd": "src/app",
                "argv": ["build"],
                "environment": {},
                "environmentPaths": {},
                "pathPrefixes": [],
                "locks": [],
                "outputs": [declared_output]
            },
            "execution": {
                "startedUnixMillis": 1,
                "finishedUnixMillis": 2,
                "durationMillis": 1,
                "exitStatus": 0
            },
            "outputs": [resolved_output.clone()],
            "result": {"outputs": [resolved_output]}
        });
        fs::write(
            generation_root.join(format!("generations/{generation}/manifest.json")),
            serde_json::to_vec(&manifest).expect("manifest serializes"),
        )
        .expect("manifest exists");
        let pointer = json!({
            "apiVersion": "nixspace/v1",
            "kind": "ActionGenerationPointer",
            "interfaceVersion": 1,
            "generation": generation,
            "manifest": format!("generations/{generation}/manifest.json"),
            "identity": identity
        });
        fs::write(
            generation_root.join("current"),
            serde_json::to_vec(&pointer).expect("pointer serializes"),
        )
        .expect("pointer exists");

        let locked = |relative_path: &str| {
            json!({
                "kind": "nix-store",
                "deployable": true,
                "installable": ".#target-app--default",
                "provider": "app_release",
                "package": "default",
                "targetId": "default",
                "storePath": locked_prefix,
                "relativePath": relative_path,
                "provenance": {
                    "kind": "locked-output",
                    "label": "LOCKED",
                    "provider": "app_release",
                    "package": "default"
                }
            })
        };
        let scope = |candidate: &str| {
            json!({
                "commands": ["build", "env", "launch", "package-prefix", "resource", "run", "test"],
                "selectedCandidates": {"app": candidate},
                "override": {
                    "refused": false,
                    "blockedCommands": [],
                    "affectedReverseClosure": if candidate == "local" { json!(["app"]) } else { json!([]) },
                    "requiredRebuild": [],
                    "refusalReason": null
                }
            })
        };
        let plan_document = json!({
            "apiVersion": "nixspace/v1",
            "kind": "WorkspaceResolution",
            "interfaceVersion": 1,
            "roots": {"workspace": ".", "taskState": ".state"},
            "packagePlans": {
                "app": {
                    "packageId": "app",
                    "dependencyClosure": ["app"],
                    "reverseClosure": ["app"],
                    "compiledReverseClosure": ["app"],
                    "selectedScope": "local",
                    "commandScopes": {"local": scope("local"), "locked": scope("locked")}
                }
            },
            "packages": {
                "app": {
                    "packageId": "app",
                    "candidates": {
                        "local": {
                            "kind": "local-worktree",
                            "deployable": false,
                            "prefix": {
                                "kind": "source-relative",
                                "sourceInput": "app_source",
                                "repositoryId": "app",
                                "sourceRoot": ".",
                                "workspacePath": "src/app"
                            },
                            "provenance": {
                                "kind": "local-git",
                                "cleanLabel": "LOCAL commit",
                                "dirtyLabel": "LOCAL dirty",
                                "sourceInput": "app_source",
                                "inspection": {
                                    "workingDirectory": "src/app",
                                    "dirty": {"argv": ["sh", "-c", "printf ''"], "cleanWhen": "stdout-empty"},
                                    "revision": {"argv": ["sh", "-c", "printf 'abc123\\n'"]}
                                }
                            }
                        },
                        "locked": locked(".")
                    }
                }
            },
            "artifacts": {
                "app:default:cli": {
                    "coordinate": "app:default:cli",
                    "packageId": "app",
                    "targetId": "default",
                    "artifactId": "cli",
                    "kind": "executable",
                    "contract": {"name": "app-cli", "version": 1},
                    "candidates": {
                        "local": {
                            "kind": "published-generation",
                            "relativePath": "bin/app",
                            "workspacePath": "src/app/bin/app",
                            "generation": {
                                "producerTask": "app:default:build",
                                "store": {"kind": "workspace-relative", "workspacePath": ".state/devel/app"},
                                "pointer": {
                                    "apiVersion": "nixspace/v1",
                                    "kind": "ActionGenerationPointer",
                                    "interfaceVersion": 1,
                                    "file": "current",
                                    "identity": identity
                                }
                            }
                        },
                        "locked": locked("bin/app")
                    },
                    "consumers": ["app:default:build"]
                }
            },
            "resources": {
                "app/config": {
                    "coordinate": "app/config",
                    "packageId": "app",
                    "resourceId": "config",
                    "kind": "configuration",
                    "candidates": {
                        "local": {
                            "kind": "source-relative",
                            "sourceInput": "app_source",
                            "repositoryId": "app",
                            "sourceRoot": ".",
                            "relativePath": "config/default.json",
                            "workspacePath": "src/app/config/default.json"
                        },
                        "locked": locked("config/default.json")
                    }
                }
            },
            "executables": {
                "app/app": {
                    "coordinate": "app/app",
                    "packageId": "app",
                    "executableId": "app",
                    "artifact": "app:default:cli",
                    "argv": ["--fixed"],
                    "candidates": {
                        "local": {"kind": "artifact-candidate", "artifact": "app:default:cli", "candidate": "local"},
                        "locked": {"kind": "artifact-candidate", "artifact": "app:default:cli", "candidate": "locked"}
                    }
                }
            },
            "actionBindings": {
                "app:default:build": {
                    "packageId": "app",
                    "targetId": "default",
                    "actionId": "build",
                    "scope": "action",
                    "artifacts": {
                        "cli": {
                            "artifact": "app:default:cli",
                            "contract": {"name": "app-cli", "version": 1},
                            "environment": {
                                "name": "APP_PATH",
                                "value": {"kind": "selected-artifact-path", "artifact": "app:default:cli"}
                            }
                        }
                    }
                }
            }
        });
        let plan = root.join("resolution.json");
        fs::write(
            &plan,
            serde_json::to_vec(&plan_document).expect("plan serializes"),
        )
        .expect("plan exists");
        Self {
            root,
            plan,
            arguments,
            local_executable,
            locked_executable,
        }
    }

    fn document(&self) -> Value {
        serde_json::from_slice(&fs::read(&self.plan).expect("plan is readable"))
            .expect("plan is JSON")
    }

    fn write_document(&self, document: &Value) {
        fs::write(
            &self.plan,
            serde_json::to_vec(document).expect("plan serializes"),
        )
        .expect("plan is writable");
    }

    fn command(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_nixspace"))
            .arg("--workspace-root")
            .arg(&self.root)
            .arg("--resolution-plan")
            .arg(&self.plan)
            .args(arguments)
            .output()
            .expect("nixspace starts")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn parse_json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

#[test]
fn every_command_consumes_one_exact_local_plan() {
    let fixture = Fixture::new();
    let explanation = parse_json(&fixture.command(&["env", "app", "--explain", "--json"]));
    assert_eq!(
        explanation["selections"][0]["provenance"]["label"],
        "LOCAL commit"
    );
    assert_eq!(
        explanation["selections"][0]["provenance"]["revision"],
        "abc123"
    );
    assert_eq!(explanation["actionBindings"][0]["environment"], "APP_PATH");
    assert_eq!(
        explanation["actionBindings"][0]["path"],
        fixture.local_executable.display().to_string()
    );
    assert_eq!(
        explanation["actionBindings"][0]["expectedContract"],
        explanation["actionBindings"][0]["actualContract"]
    );

    let prefix = parse_json(&fixture.command(&["package", "prefix", "app", "--json"]));
    assert_eq!(
        prefix["path"],
        fixture.root.join("src/app").display().to_string()
    );
    let resource = parse_json(&fixture.command(&["resource", "app/config", "--json"]));
    assert_eq!(
        resource["path"],
        fixture
            .root
            .join("src/app/config/default.json")
            .display()
            .to_string()
    );

    let run = fixture.command(&["run", "--selection-root", "app", "app/app", "--", "added"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        fs::read_to_string(&fixture.arguments).expect("arguments recorded"),
        "--fixed\nadded\n"
    );
}

#[test]
fn selected_local_failure_never_uses_available_locked_candidate() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.root.join(".state/devel/app/current"))
        .expect("current pointer removed");
    let output = fixture.command(&["run", "app/app"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("locked fallback is forbidden"), "{stderr}");
    assert!(!fixture.arguments.exists());
}

#[test]
fn locked_scope_resolves_only_concrete_store_candidates() {
    let fixture = Fixture::new();
    let mut document = fixture.document();
    document["packagePlans"]["app"]["selectedScope"] = json!("locked");
    fixture.write_document(&document);

    let prefix = parse_json(&fixture.command(&["package", "prefix", "app", "--json"]));
    assert_eq!(prefix["selectedCandidate"], "locked");
    assert_eq!(prefix["provenance"]["label"], "LOCKED");
    assert_eq!(
        prefix["path"],
        fixture.root.join("locked-app").display().to_string()
    );
    let run = fixture.command(&["run", "app/app", "--", "locked"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        fs::read_to_string(&fixture.arguments).expect("arguments recorded"),
        "--fixed\nlocked\n"
    );
    assert!(fixture.locked_executable.exists());
}

#[test]
fn absolute_root_owned_source_bindings_may_live_outside_workspace_root() {
    let fixture = Fixture::new();
    let mut document = fixture.document();
    let prefix = fixture.root.join("src/app").display().to_string();
    let resource = fixture
        .root
        .join("src/app/config/default.json")
        .display()
        .to_string();
    document["packages"]["app"]["candidates"]["local"]["prefix"]["workspacePath"] = json!(prefix);
    document["packages"]["app"]["candidates"]["local"]["provenance"]["inspection"]
        ["workingDirectory"] = json!(fixture.root.join("src/app"));
    document["resources"]["app/config"]["candidates"]["local"]["workspacePath"] = json!(resource);
    fixture.write_document(&document);

    let prefix = fixture.command(&["package", "prefix", "app"]);
    assert!(
        prefix.status.success(),
        "{}",
        String::from_utf8_lossy(&prefix.stderr)
    );
    let resource = fixture.command(&["resource", "app/config"]);
    assert!(
        resource.status.success(),
        "{}",
        String::from_utf8_lossy(&resource.stderr)
    );
}

#[test]
fn locked_selection_accepts_an_intentionally_absent_local_artifact() {
    let fixture = Fixture::new();
    let mut document = fixture.document();
    document["packagePlans"]["app"]["selectedScope"] = json!("locked");
    document["artifacts"]["app:default:cli"]["candidates"]["local"] = Value::Null;
    document["executables"]["app/app"]["candidates"]["local"] = Value::Null;
    fixture.write_document(&document);
    fs::remove_file(fixture.root.join(".state/devel/app/current")).unwrap();

    let explanation = fixture.command(&["env", "app", "--explain", "--json"]);
    assert!(
        explanation.status.success(),
        "{}",
        String::from_utf8_lossy(&explanation.stderr)
    );
    let run = fixture.command(&["run", "app/app"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn explicit_selection_root_controls_cross_package_candidate_choice() {
    let fixture = Fixture::new();
    let mut document = fixture.document();
    let mut bundle = document["packages"]["app"].clone();
    bundle["packageId"] = json!("bundle");
    document["packages"]["bundle"] = bundle;
    let mut bundle_plan = document["packagePlans"]["app"].clone();
    bundle_plan["packageId"] = json!("bundle");
    bundle_plan["dependencyClosure"] = json!(["app", "bundle"]);
    bundle_plan["reverseClosure"] = json!(["bundle"]);
    bundle_plan["compiledReverseClosure"] = json!(["bundle"]);
    bundle_plan["selectedScope"] = json!("locked");
    bundle_plan["commandScopes"]["locked"]["selectedCandidates"] =
        json!({"app": "locked", "bundle": "locked"});
    document["packagePlans"]["bundle"] = bundle_plan;
    fixture.write_document(&document);
    fs::remove_file(fixture.root.join(".state/devel/app/current")).unwrap();

    let direct = fixture.command(&["run", "app/app"]);
    assert!(!direct.status.success(), "owner plan still selects local");
    let bundled = fixture.command(&[
        "run",
        "--selection-root",
        "bundle",
        "app/app",
        "--",
        "bundled",
    ]);
    assert!(
        bundled.status.success(),
        "{}",
        String::from_utf8_lossy(&bundled.stderr)
    );
    assert_eq!(
        fs::read_to_string(&fixture.arguments).unwrap(),
        "--fixed\nbundled\n"
    );
}

#[test]
fn refusal_and_generation_identity_are_enforced_without_fallback() {
    let fixture = Fixture::new();
    let mut refused = fixture.document();
    let policy = &mut refused["packagePlans"]["app"]["commandScopes"]["local"]["override"];
    policy["refused"] = json!(true);
    policy["blockedCommands"] = json!(["run"]);
    policy["refusalReason"] = json!("compiled reverse closure must be rebuilt");
    policy["requiredRebuild"] = json!(["app"]);
    fixture.write_document(&refused);
    let output = fixture.command(&["run", "app/app"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("compiled reverse closure"));

    let mut identity = fixture.document();
    identity["packagePlans"]["app"]["commandScopes"]["local"]["override"] = json!({
        "refused": false,
        "blockedCommands": [],
        "affectedReverseClosure": ["app"],
        "requiredRebuild": [],
        "refusalReason": null
    });
    identity["artifacts"]["app:default:cli"]["candidates"]["local"]["generation"]["pointer"]
        ["identity"]["declaration"]["task"] = json!("other");
    fixture.write_document(&identity);
    let output = fixture.command(&["run", "app/app"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("identity"));
}

#[test]
fn resolution_plan_environment_and_exact_kind_are_required() {
    let fixture = Fixture::new();
    let output = Command::new(env!("CARGO_BIN_EXE_nixspace"))
        .arg("--workspace-root")
        .arg(&fixture.root)
        .env("NIXSPACE_RESOLUTION_PLAN", &fixture.plan)
        .args(["resource", "app/config"])
        .output()
        .expect("nixspace starts");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut wrong = fixture.document();
    wrong["kind"] = json!("WorkspaceResolutionTemplate");
    fixture.write_document(&wrong);
    let output = fixture.command(&["resource", "app/config"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("kind"));
}

#[test]
fn stale_native_output_is_reproved_before_execution() {
    let fixture = Fixture::new();
    fs::write(&fixture.local_executable, "#!/bin/sh\nexit 0\n").expect("output mutates");
    fs::set_permissions(&fixture.local_executable, fs::Permissions::from_mode(0o755))
        .expect("executable mode remains");
    let output = fixture.command(&["run", "app/app"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("published generation proof"));
}
