use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

fn workspace_index() -> Value {
    let port_parameter = json!({
        "type": "port",
        "description": "service port",
        "required": false,
        "default": 8000,
        "enumValues": [],
        "minimum": 1024,
        "maximum": 9000,
        "allowedRoots": [],
        "mustExist": false,
        "access": "read",
        "allocation": "fixed",
        "allocationHost": "127.0.0.1",
        "allocationTransport": "tcp"
    });
    let secret_parameter = json!({
        "type": "secret",
        "description": "service token",
        "required": true,
        "default": null,
        "enumValues": [],
        "minimum": null,
        "maximum": null,
        "allowedRoots": [],
        "mustExist": false,
        "access": "read",
        "allocation": "fixed",
        "allocationHost": "127.0.0.1",
        "allocationTransport": "tcp"
    });
    let path_parameter = json!({
        "type": "path",
        "description": "runtime configuration",
        "required": false,
        "default": "config/app.json",
        "enumValues": [],
        "minimum": null,
        "maximum": null,
        "allowedRoots": ["config"],
        "mustExist": true,
        "access": "read",
        "allocation": "fixed",
        "allocationHost": "127.0.0.1",
        "allocationTransport": "tcp"
    });
    let package = json!({
        "id": "app",
        "aliases": ["application"],
        "extensions": {
            "test.example/package-v1": {
                "title": "Example application",
                "arbitraryProviderData": {"nested": [true, 7]}
            }
        }
    });
    let target = json!({
        "coordinate": "app/default",
        "packageId": "app",
        "target": {
            "id": "default",
            "actions": {"build": {"kind": "build", "adapter": "cargo-v1"}},
            "artifacts": {"outputs": {"app-bin": {}}, "inputs": {}},
            "variants": {},
            "release": {}
        }
    });
    let graph = json!({
        "nodes": [
            {"id": "app/default", "type": "target", "package": "app", "target": "default"},
            {"id": "app/default/build", "type": "action", "package": "app", "target": "default", "action": "build", "kind": "build", "adapter": "cargo-v1"}
        ],
        "edges": []
    });
    json!({
        "apiVersion": "nixspace/v1",
        "kind": "Workspace",
        "interfaceVersion": 2,
        "catalog": {
            "packages": [package],
            "targets": [target],
            "artifacts": [{
                "coordinate": "app:default:app-bin",
                "packageId": "app",
                "targetId": "default",
                "artifactId": "app-bin",
                "kind": "executable",
                "path": "bin/app",
                "contract": {"name": "app-cli", "version": 1},
                "producedBy": "build"
            }],
            "resources": [{
                "coordinate": "app/config",
                "packageId": "app",
                "resourceId": "config",
                "kind": "configuration",
                "path": "config/app.json"
            }],
            "executables": [{
                "coordinate": "app/cli",
                "packageId": "app",
                "executableId": "cli",
                "from": "app:default:app-bin",
                "argv": ["--fixed"]
            }],
            "launches": [{
                "coordinate": "app/stack",
                "packageId": "app",
                "launchId": "stack",
                "launch": {
                    "description": "Run the application stack.",
                    "parameters": {
                        "service-port": port_parameter,
                        "service-token": secret_parameter,
                        "config": path_parameter
                    },
                    "requiredArtifacts": ["app:default:app-bin"],
                    "requiredResources": ["app/config"],
                    "sessionEnvironment": {
                        "STACK_STATE": {"base": "session", "path": "stack/state", "create": "directory"}
                    },
                    "processes": {},
                    "includes": {"service": {"launch": "app:service", "parameters": {}}},
                    "capabilities": {"provides": ["stack"], "requires": []}
                }
            }]
        },
        "graph": {
            "schemaVersion": 1,
            "all": graph,
            "packages": {"app": graph},
            "reverse": {"app": graph}
        },
        "actionPlans": {
            "schemaVersion": 1,
            "runner": {
                "kind": "devenv-task",
                "direct": {"argv": ["devenv-flake-tasks", "run"], "requiredEnvironment": ["DEVENV_TASK_FILE", "NIXSPACE_INDEX", "NIXSPACE_WORKSPACE_ROOT"]},
                "bootstrap": {"argv": ["nix", "develop", ".#default", "--command", "devenv-flake-tasks", "run"]}
            },
            "actions": {
                "build": {
                    "all": ["app:default:build"],
                    "packages": {"app": ["app:default:build"]}
                },
                "test": {"all": [], "packages": {"app": []}}
            }
        },
        "launchPlans": {
            "app/stack": {
                "launch": "app/stack",
                "parameters": {
                    "service-port": port_parameter,
                    "service-token": secret_parameter,
                    "config": path_parameter
                },
                "requiredArtifacts": ["app:default:app-bin"],
                "requiredResources": ["app/config"],
                "capabilities": {"provides": ["stack"], "requires": []},
                "sessionEnvironment": {
                    "STACK_STATE": {"base": "session", "path": "stack/state", "create": "directory"}
                },
                "instances": [
                    {
                        "instanceId": "app/stack",
                        "launch": "app/stack",
                        "includedBy": null,
                        "parameterBindings": {
                            "service-port": "service-port",
                            "service-token": "service-token",
                            "config": "config"
                        },
                        "parameters": {
                            "service-port": port_parameter,
                            "service-token": secret_parameter,
                            "config": path_parameter
                        }
                    },
                    {
                        "instanceId": "app/stack/service",
                        "launch": "app/service",
                        "includedBy": "app/stack",
                        "parameterBindings": {
                            "port": "service-port",
                            "token": "service-token"
                        },
                        "parameters": {
                            "port": port_parameter,
                            "token": secret_parameter
                        }
                    }
                ],
                "processes": [{
                    "id": "app/stack/service/server",
                    "instance": "app/stack/service",
                    "launch": "app/service",
                    "processId": "server",
                    "executable": "app:cli",
                    "artifact": "app:default:app-bin",
                    "executableArgv": ["--fixed"],
                    "argv": [
                        {"literal": "--port", "parameter": null, "prefix": "", "suffix": ""},
                        {"literal": null, "parameter": "port", "prefix": "", "suffix": ""}
                    ],
                    "environment": {
                        "SERVICE_TOKEN": {"literal": null, "parameter": "token", "prefix": "", "suffix": ""}
                    },
                    "workingDirectory": ".",
                    "dependencies": {},
                    "endpoints": {
                        "api": {"protocol": "http", "hostParameter": null, "portParameter": "port"}
                    },
                    "readiness": {"kind": "endpoint", "endpoint": "api", "timeoutMs": 30000},
                    "restart": {"policy": "never", "maxAttempts": 0, "backoffMs": 0},
                    "shutdown": {"signal": "SIGTERM", "timeoutMs": 10000, "killSignal": "SIGKILL"},
                    "required": true,
                    "onExit": "stop-launch",
                    "onReadinessLoss": "stop-launch"
                }]
            }
        }
    })
}

struct IndexFile(PathBuf);

impl IndexFile {
    fn new() -> Self {
        Self::from_value(&workspace_index())
    }

    fn from_value(index: &Value) -> Self {
        let sequence = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nixspace-query-{}-{sequence}.json",
            std::process::id()
        ));
        fs::write(
            &path,
            serde_json::to_vec(index).expect("fixture serializes"),
        )
        .expect("fixture is writable");
        Self(path)
    }
}

impl Drop for IndexFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn nixspace(arguments: &[&str]) -> Output {
    let index = IndexFile::new();
    nixspace_with_index(&index, arguments)
}

fn nixspace_with_index(index: &IndexFile, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nixspace"))
        .arg("--index")
        .arg(&index.0)
        .args(arguments)
        .output()
        .expect("nixspace starts")
}

fn json_output(arguments: &[&str]) -> Value {
    let output = nixspace(arguments);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

#[test]
fn cached_index_rejects_unknown_fields_and_ambiguous_package_aliases() {
    let base = workspace_index();
    for (path, diagnostic) in [
        (vec!["unexpected"], "unknown field `unexpected`"),
        (
            vec!["catalog", "packages", "0", "unexpected"],
            "unknown field `unexpected`",
        ),
    ] {
        let mut invalid = base.clone();
        let mut value = &mut invalid;
        for component in &path[..path.len() - 1] {
            value = if let Ok(index) = component.parse::<usize>() {
                &mut value.as_array_mut().unwrap()[index]
            } else {
                &mut value[*component]
            };
        }
        value[path[path.len() - 1]] = json!(true);
        let index = IndexFile::from_value(&invalid);
        let output = nixspace_with_index(&index, &["package", "list"]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains(diagnostic));
    }

    let mut ambiguous = base.clone();
    let mut second = ambiguous["catalog"]["packages"][0].clone();
    second["id"] = json!("other");
    second["aliases"] = json!(["application"]);
    ambiguous["catalog"]["packages"]
        .as_array_mut()
        .unwrap()
        .push(second);
    let index = IndexFile::from_value(&ambiguous);
    let output = nixspace_with_index(&index, &["package", "list"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("ambiguous"));

    let mut legacy = base.clone();
    legacy["interfaceVersion"] = json!(1);
    let index = IndexFile::from_value(&legacy);
    let output = nixspace_with_index(&index, &["package", "list"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("supports version 2"));

    let mut unnamespaced = base;
    let value = unnamespaced["catalog"]["packages"][0]["extensions"]
        .as_object_mut()
        .unwrap()
        .remove("test.example/package-v1")
        .unwrap();
    unnamespaced["catalog"]["packages"][0]["extensions"]["legacy"] = value;
    let index = IndexFile::from_value(&unnamespaced);
    let output = nixspace_with_index(&index, &["package", "list"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("provider namespaces"));
}

#[test]
fn cached_index_rejects_dangling_graph_edges() {
    let mut invalid = workspace_index();
    invalid["graph"]["all"]["edges"] = json!([{
        "from": "app/default",
        "to": "app/default/missing",
        "kind": "depends-on"
    }]);
    let index = IndexFile::from_value(&invalid);
    let output = nixspace_with_index(&index, &["graph", "--json"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("references a missing node"));
}

#[test]
fn cached_index_rejects_incomplete_catalog_and_launch_relationships() {
    type RelationshipCase = (fn(&mut Value), &'static str);
    let cases: [RelationshipCase; 5] = [
        (
            |index: &mut Value| index["catalog"]["packages"] = json!([]),
            "references missing package `app`",
        ),
        (
            |index: &mut Value| index["catalog"]["targets"] = json!([]),
            "references missing target `app/default`",
        ),
        (
            |index: &mut Value| index["catalog"]["launches"] = json!([]),
            "has no catalog launch",
        ),
        (
            |index: &mut Value| {
                index["launchPlans"]["app/stack"]["processes"][0]["artifact"] =
                    json!("app:default:other")
            },
            "does not equal executable",
        ),
        (
            |index: &mut Value| {
                index["launchPlans"]["app/stack"]["processes"][0]["executableArgv"] =
                    json!(["--different"])
            },
            "executableArgv does not equal",
        ),
    ];
    for (mutate, diagnostic) in cases {
        let mut invalid = workspace_index();
        mutate(&mut invalid);
        let index = IndexFile::from_value(&invalid);
        let output = nixspace_with_index(&index, &["package", "list"]);
        assert!(
            !output.status.success(),
            "invalid relationship was accepted"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(diagnostic),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn package_and_collection_queries_project_emitted_documents() {
    let packages = json_output(&["package", "list", "--json"]);
    assert_eq!(packages["count"], 1);
    assert_eq!(packages["interfaceVersion"], 2);
    assert_eq!(packages["packages"][0]["id"], "app");
    assert_eq!(
        packages["packages"][0]["extensions"]["test.example/package-v1"]["arbitraryProviderData"]
            ["nested"],
        json!([true, 7])
    );

    let package = json_output(&["package", "show", "application", "--json"]);
    assert_eq!(package["id"], "app");

    let target = json_output(&["target", "show", "app/default", "--json"]);
    assert_eq!(target["target"]["id"], "default");

    let artifacts = json_output(&["artifact", "list", "application", "--json"]);
    assert_eq!(
        artifacts["artifacts"][0]["coordinate"],
        "app:default:app-bin"
    );

    let executable = json_output(&["executable", "show", "app/cli", "--json"]);
    assert_eq!(executable["argv"], json!(["--fixed"]));
}

#[test]
fn workspace_index_rejects_missing_nix_normalized_launch_fields() {
    let cases = [
        (
            &[
                "launchPlans",
                "app/stack",
                "processes",
                "0",
                "argv",
                "0",
                "prefix",
            ][..],
            "prefix",
        ),
        (
            &[
                "launchPlans",
                "app/stack",
                "processes",
                "0",
                "argv",
                "0",
                "suffix",
            ][..],
            "suffix",
        ),
        (
            &[
                "launchPlans",
                "app/stack",
                "parameters",
                "service-port",
                "allocation",
            ][..],
            "allocation",
        ),
        (
            &[
                "launchPlans",
                "app/stack",
                "parameters",
                "service-port",
                "allocationHost",
            ][..],
            "allocationHost",
        ),
        (
            &[
                "launchPlans",
                "app/stack",
                "parameters",
                "service-port",
                "allocationTransport",
            ][..],
            "allocationTransport",
        ),
    ];

    for (path, field) in cases {
        let mut document = workspace_index();
        remove_field(&mut document, path);
        let index = IndexFile::from_value(&document);
        let output = nixspace_with_index(&index, &["launch", "list"]);
        assert!(!output.status.success(), "missing {field} was accepted");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(&format!("missing field `{field}`")),
            "missing {field}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn remove_field(document: &mut Value, path: &[&str]) {
    let (field, parents) = path.split_last().expect("field path is nonempty");
    let mut value = document;
    for segment in parents {
        value = if let Ok(index) = segment.parse::<usize>() {
            &mut value[index]
        } else {
            &mut value[*segment]
        };
    }
    value
        .as_object_mut()
        .expect("field parent is an object")
        .remove(*field)
        .expect("normalized field exists");
}

#[test]
fn graph_selects_the_nix_emitted_document_without_replanning() {
    let graph = json_output(&["graph", "application", "--reverse", "--json"]);
    assert_eq!(graph["nodes"][0]["id"], "app/default");
    assert_eq!(graph["nodes"][1]["id"], "app/default/build");

    let missing_package = nixspace(&["graph", "missing", "--json"]);
    assert!(!missing_package.status.success());
    assert!(String::from_utf8_lossy(&missing_package.stderr).contains("package `missing`"));

    let missing_reverse_root = nixspace(&["graph", "--reverse"]);
    assert!(!missing_reverse_root.status.success());
    assert!(String::from_utf8_lossy(&missing_reverse_root.stderr)
        .contains("reverse dependency graph requires a package"));
}

#[test]
fn completion_uses_normalized_coordinates_and_aliases() {
    let packages = nixspace(&["_complete", "target", "list", ""]);
    assert!(packages.status.success());
    assert_eq!(
        String::from_utf8_lossy(&packages.stdout),
        "--json\napp\napplication\n"
    );

    let targets = nixspace(&["_complete", "target", "show", ""]);
    assert!(targets.status.success());
    assert_eq!(String::from_utf8_lossy(&targets.stdout), "app/default\n");

    let resources = nixspace(&["_complete", "resource", ""]);
    assert!(resources.status.success());
    assert_eq!(
        String::from_utf8_lossy(&resources.stdout),
        "--json\napp/config\n"
    );

    let package = nixspace(&["_complete", "package", ""]);
    assert!(package.status.success());
    assert_eq!(
        String::from_utf8_lossy(&package.stdout),
        "list\nshow\nprefix\n"
    );
}

#[test]
fn clap_generates_shell_completion_adapters() {
    let bash = nixspace(&["completion", "bash"]);
    assert!(
        bash.status.success(),
        "{}",
        String::from_utf8_lossy(&bash.stderr)
    );
    let script = String::from_utf8(bash.stdout).expect("completion is UTF-8");
    assert!(script.contains("_nixspace"));
    assert!(script.contains("completion"));
    assert!(script.contains("west"));
}

#[test]
fn launch_plan_resolves_only_runtime_values_and_redacts_secrets() {
    let plan = json_output(&[
        "launch",
        "plan",
        "app/stack",
        "--set",
        "service-port=8123",
        "--json",
    ]);
    assert_eq!(plan["launch"], "app/stack");
    assert_eq!(plan["instances"][1]["includedBy"], "app/stack");
    assert_eq!(plan["parameters"]["service-port"]["source"], "set");
    assert_eq!(plan["parameters"]["service-token"]["value"], "<redacted>");
    assert_eq!(
        plan["processes"][0]["argv"],
        json!(["--fixed", "--port", "8123"])
    );
    assert_eq!(plan["processes"][0]["endpoints"]["api"]["port"], 8123);
    assert_eq!(
        plan["processes"][0]["environment"]["SERVICE_TOKEN"],
        json!({
            "secretParameter": "token",
            "resolved": false,
            "value": "<redacted>"
        })
    );
}

#[test]
fn launch_plan_rejects_secret_cli_values_without_echoing_them() {
    let secret = "must-never-appear";
    let output = nixspace(&[
        "launch",
        "plan",
        "app/stack",
        "--set",
        &format!("service-token={secret}"),
        "--json",
    ]);
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("secret parameter `service-token` cannot be passed"));
    assert!(!stdout.contains(secret));
    assert!(!stderr.contains(secret));
}

#[test]
fn launch_plan_validates_bounds_paths_and_duplicate_assignments() {
    let invalid_port = nixspace(&["launch", "plan", "app/stack", "--set", "service-port=9999"]);
    assert!(!invalid_port.status.success());
    assert!(String::from_utf8_lossy(&invalid_port.stderr).contains("above its maximum"));

    let invalid_path = nixspace(&["launch", "plan", "app/stack", "--set", "config=../secret"]);
    assert!(!invalid_path.status.success());
    assert!(String::from_utf8_lossy(&invalid_path.stderr).contains("invalid `path` value"));

    let duplicate = nixspace(&[
        "launch",
        "plan",
        "app/stack",
        "--set",
        "service-port=8001",
        "--set",
        "service-port=8002",
    ]);
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("set more than once"));
}
