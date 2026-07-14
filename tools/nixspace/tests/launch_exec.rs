#![cfg(unix)]

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use serde_json::{json, Value};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    index: PathBuf,
    plan: PathBuf,
    manager: PathBuf,
    log: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("nixspace-launch-{}-{sequence}", std::process::id()));
        fs::create_dir_all(root.join("bin")).unwrap();
        let manager = root.join("bin/manager");
        let log = root.join("manager.log");
        write_executable(
            &manager,
            r#"#!/bin/sh
printf 'argv' >> "$NIXSPACE_TEST_MANAGER_LOG"
printf ' <%s>' "$@" >> "$NIXSPACE_TEST_MANAGER_LOG"
if [ -n "${TEST_SECRET:-}" ]; then secret_set=yes; else secret_set=no; fi
printf '\nport=<%s> secret-set=<%s> session=<%s> mapping=<%s>\n' \
  "$TEST_PORT" "$secret_set" "$NIXSPACE_SESSION_DIR" "$TEST_MAPPING" \
  >> "$NIXSPACE_TEST_MANAGER_LOG"
exit "${NIXSPACE_TEST_MANAGER_EXIT:-0}"
"#,
        );
        let index = root.join("index.json");
        let plan = root.join("launch-plan.json");
        fs::write(&index, serde_json::to_vec(&workspace_index()).unwrap()).unwrap();
        fs::write(
            &plan,
            serde_json::to_vec(&execution_plan(&manager)).unwrap(),
        )
        .unwrap();
        Self {
            root,
            index,
            plan,
            manager,
            log,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_nixspace"));
        command
            .arg("--workspace-root")
            .arg(&self.root)
            .arg("--index")
            .arg(&self.index)
            .arg("--launch-plan")
            .arg(&self.plan)
            .env("NIXSPACE_TEST_MANAGER_LOG", &self.log)
            .env("TEST_SECRET", "top-secret-value")
            .env_remove("NIXSPACE_TEST_MANAGER_EXIT");
        command
    }

    fn run(&self, arguments: &[&str]) -> Output {
        self.command().args(arguments).output().unwrap()
    }

    fn session(&self, id: &str) -> PathBuf {
        self.root.join("state/sessions").join(id)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn parameter(parameter_type: &str, required: bool, default: Value) -> Value {
    json!({
        "type": parameter_type,
        "description": "runtime value",
        "required": required,
        "default": default,
        "enumValues": [],
        "minimum": null,
        "maximum": null,
        "allowedRoots": [],
        "mustExist": false,
        "access": "read",
        "allocation": if parameter_type == "port" { "automatic" } else { "fixed" },
        "allocationHost": "127.0.0.1",
        "allocationTransport": "tcp"
    })
}

fn workspace_index() -> Value {
    let port = parameter("port", false, json!(8000));
    let secret = parameter("secret", true, Value::Null);
    let graph = json!({"nodes": [], "edges": []});
    json!({
        "apiVersion": "nixspace/v1",
        "kind": "Workspace",
        "interfaceVersion": 1,
        "catalog": {
            "packages": [], "targets": [], "resources": [], "launches": [],
            "artifacts": [{
                "coordinate": "app:default:program", "packageId": "app",
                "targetId": "default", "artifactId": "program", "kind": "executable",
                "path": "bin/program", "contract": {"name": "program", "version": 1},
                "producedBy": "build"
            }],
            "executables": [{
                "coordinate": "app/program", "packageId": "app", "executableId": "program",
                "from": "app:default:program", "argv": ["fixed"]
            }]
        },
        "graph": {"schemaVersion": 1, "all": graph, "packages": {}, "reverse": {}},
        "actionPlans": {
            "schemaVersion": 1,
            "runner": {
                "kind": "devenv-task",
                "direct": {"argv": ["devenv-flake-tasks", "run"], "requiredEnvironment": ["DEVENV_TASK_FILE", "NIXSPACE_INDEX", "NIXSPACE_WORKSPACE_ROOT"]},
                "bootstrap": {"argv": ["nix", "develop", ".#default", "--command", "devenv-flake-tasks", "run"]}
            },
            "actions": {"build": {"all": [], "packages": {}}, "test": {"all": [], "packages": {}}}
        },
        "launchPlans": {
            "app/stack": {
                "launch": "app/stack",
                "parameters": {"port": port, "secret": secret},
                "requiredArtifacts": [], "requiredResources": [],
                "capabilities": {"provides": [], "requires": []},
                "instances": [{
                    "instanceId": "app/stack", "launch": "app/stack", "includedBy": null,
                    "parameterBindings": {"port": "port", "secret": "secret"},
                    "parameters": {"port": port, "secret": secret}
                }],
                "processes": [{
                    "id": "app:stack:server", "instance": "app/stack",
                    "launch": "app/stack", "processId": "server",
                    "executable": "app/program", "artifact": "app:default:program",
                    "executableArgv": ["program"], "argv": [], "environment": {},
                    "workingDirectory": null, "dependencies": {},
                    "endpoints": {"http": {"protocol": "http", "hostParameter": null, "portParameter": "port"}},
                    "readiness": {}, "restart": {}, "shutdown": {},
                    "required": true, "onExit": "stop", "onReadinessLoss": "stop"
                }]
            }
        }
    })
}

fn contract(
    schema_coordinate: &str,
    workspace_coordinate: &str,
    parameter_id: &str,
    environment: &str,
    parameter_type: &str,
    required: bool,
) -> Value {
    json!({
        "coordinate": schema_coordinate,
        "workspaceCoordinate": workspace_coordinate,
        "parameterId": parameter_id,
        "environment": environment,
        "type": parameter_type,
        "required": required,
        "default": if parameter_type == "port" { json!(8000) } else { Value::Null },
        "enumValues": [], "minimum": null, "maximum": null, "allowedRoots": [],
        "mustExist": false, "access": "read", "secret": parameter_type == "secret",
        "allocation": if parameter_type == "port" { "automatic" } else { "fixed" }, "allocationHost": "127.0.0.1",
        "allocationTransport": "tcp"
    })
}

fn execution_plan(manager: &Path) -> Value {
    let manager = manager.to_string_lossy();
    let socket = json!({"runtime": "sessionSocket"});
    let log = json!({"runtime": "sessionLog"});
    json!({
        "apiVersion": "nixspace/v1",
        "kind": "LaunchExecution",
        "interfaceVersion": 3,
        "stateRoot": "state/sessions",
        "launches": {
            "app/stack": {
                "coordinate": "app:stack",
                "workspaceLaunch": "app/stack",
                "parameters": {
                    "TEST_PORT": contract("app:stack", "app/stack", "port", "TEST_PORT", "port", false),
                    "TEST_SECRET": contract("app:stack", "app/stack", "secret", "TEST_SECRET", "secret", true)
                },
                "declaredPorts": [{
                    "launch": "app/stack", "process": "server", "endpoint": "http",
                    "protocol": "http", "transport": "tcp", "host": "127.0.0.1", "port": 8000,
                    "hostEnvironment": null, "portEnvironment": "TEST_PORT"
                }],
                "sessionEnvironment": {
                    "TEST_MAPPING": {"base": "session", "path": "mapping.json", "create": "parent"}
                },
                "processPolicies": {"server": {"required": true}},
                "runner": {
                    "kind": "devenv-process-compose",
                    "workingDirectory": ".",
                    "commands": {
                        "up": {"argv": [manager, "up", socket, log]},
                        "start": {"argv": [manager, "start", socket, log]},
                        "attach": {"argv": [manager, "attach", socket]},
                        "status": {"argv": [manager, "status", socket]},
                        "logs": {"argv": [manager, "logs", socket]},
                        "down": {"argv": [manager, "down", socket]}
                    }
                }
            }
        }
    })
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn session_port(fixture: &Fixture, id: &str) -> u16 {
    let record: Value =
        serde_json::from_slice(&fs::read(fixture.session(id).join("session.json")).unwrap())
            .unwrap();
    record["resolvedLaunch"]["processes"][0]["endpoints"]["http"]["port"]
        .as_u64()
        .and_then(|port| u16::try_from(port).ok())
        .unwrap()
}

#[test]
fn detached_launch_uses_exact_plan_and_keeps_secrets_out_of_metadata() {
    let fixture = Fixture::new();
    let output = fixture.run(&[
        "launch",
        "up",
        "app/stack",
        "--set",
        "port=9123",
        "--name",
        "demo",
        "--detach",
    ]);
    assert_success(&output);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "demo\n");
    let session = fixture.session("demo");
    let log = fs::read_to_string(&fixture.log).unwrap();
    assert!(log.contains("argv <start>"));
    assert!(log.contains("port=<9123> secret-set=<yes>"));
    assert!(!log.contains("top-secret-value"));
    assert!(log.contains(&format!("session=<{}>", session.display())));
    assert!(log.contains(&format!("mapping=<{}/mapping.json>", session.display())));
    assert!(!session.join("mapping.json").exists());
    let metadata = fs::read_to_string(session.join("session.json")).unwrap();
    assert!(!metadata.contains("top-secret-value"));
    assert!(metadata.contains("<redacted>"));
    assert_eq!(
        fs::metadata(session.join("session.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );

    let collision = fixture.run(&["launch", "up", "app/stack", "--name", "demo", "--detach"]);
    assert!(!collision.status.success());
    assert!(String::from_utf8_lossy(&collision.stderr).contains("already exists"));
}

#[test]
fn recorded_session_commands_work_and_successful_down_removes_owned_state() {
    let fixture = Fixture::new();
    assert_success(&fixture.run(&["launch", "up", "app/stack", "--name", "demo", "--detach"]));
    let sessions = fixture.run(&["launch", "sessions"]);
    assert_success(&sessions);
    assert_eq!(
        String::from_utf8_lossy(&sessions.stdout),
        "demo\tapp/stack\n"
    );
    for operation in ["status", "attach", "logs"] {
        assert_success(&fixture.run(&["launch", operation, "demo"]));
    }
    assert_success(&fixture.run(&["launch", "down", "demo"]));
    assert!(!fixture.session("demo").exists());
    let log = fs::read_to_string(&fixture.log).unwrap();
    for operation in ["status", "attach", "logs", "down"] {
        assert!(log.contains(&format!("argv <{operation}>")));
    }
}

#[test]
fn automatic_ports_isolate_two_sessions_and_down_proves_port_cleanup() {
    let fixture = Arc::new(Fixture::new());
    let barrier = Arc::new(Barrier::new(3));
    let handles = ["one", "two"].map(|id| {
        let fixture = Arc::clone(&fixture);
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            fixture.run(&["launch", "up", "app/stack", "--name", id, "--detach"])
        })
    });
    barrier.wait();
    for handle in handles {
        assert_success(&handle.join().unwrap());
    }
    let one = session_port(&fixture, "one");
    let two = session_port(&fixture, "two");
    assert_ne!(one, two);

    let occupied = TcpListener::bind(("127.0.0.1", one)).unwrap();
    let rejected = fixture.run(&["launch", "down", "one"]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("still occupied"));
    assert!(fixture.session("one").exists());
    drop(occupied);

    assert_success(&fixture.run(&["launch", "down", "one"]));
    assert_success(&fixture.run(&["launch", "down", "two"]));
    assert!(!fixture.session("one").exists());
    assert!(!fixture.session("two").exists());
}

#[test]
fn failed_detached_start_removes_pending_session_state() {
    let fixture = Fixture::new();
    let failed = fixture
        .command()
        .env("NIXSPACE_TEST_MANAGER_EXIT", "23")
        .args(["launch", "up", "app/stack", "--name", "failed", "--detach"])
        .output()
        .unwrap();
    assert_eq!(failed.status.code(), Some(23));
    assert!(!fixture.session("failed").exists());
}

#[test]
fn launch_rejects_missing_secrets_and_plan_traversal_before_starting_manager() {
    let fixture = Fixture::new();
    let missing = fixture
        .command()
        .env_remove("TEST_SECRET")
        .args(["launch", "up", "app/stack", "--detach"])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("requires secret environment"));
    assert!(!fixture.log.exists());

    let mut plan = execution_plan(&fixture.manager);
    plan["stateRoot"] = json!("../outside");
    fs::write(&fixture.plan, serde_json::to_vec(&plan).unwrap()).unwrap();
    let traversal = fixture.run(&["launch", "sessions"]);
    assert!(!traversal.status.success());
    assert!(String::from_utf8_lossy(&traversal.stderr).contains("safe workspace-relative"));
    assert!(!fixture.root.parent().unwrap().join("outside").exists());
}

#[test]
fn foreground_launch_propagates_exit_status() {
    let fixture = Fixture::new();
    let foreground = fixture
        .command()
        .env("NIXSPACE_TEST_MANAGER_EXIT", "23")
        .args(["launch", "up", "app/stack", "--name", "foreground"])
        .output()
        .unwrap();
    assert_eq!(foreground.status.code(), Some(23));
    assert!(fs::read_to_string(&fixture.log)
        .unwrap()
        .contains("argv <up>"));
}

#[test]
fn tcp_and_http_probes_are_one_shot_and_exact() {
    let tcp = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let tcp_port = tcp.local_addr().unwrap().port();
    let tcp_thread = thread::spawn(move || tcp.accept().unwrap());
    let output = Command::new(env!("CARGO_BIN_EXE_nixspace"))
        .args([
            "_probe",
            "tcp",
            "--host",
            "127.0.0.1",
            "--port",
            &tcp_port.to_string(),
        ])
        .output()
        .unwrap();
    assert_success(&output);
    tcp_thread.join().unwrap();

    let http = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let http_port = http.local_addr().unwrap().port();
    let http_thread = thread::spawn(move || {
        let (mut stream, _) = http.accept().unwrap();
        let mut request = [0_u8; 1024];
        let size = stream.read(&mut request).unwrap();
        assert!(String::from_utf8_lossy(&request[..size]).starts_with("GET /health HTTP/1.1\r\n"));
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
    });
    let output = Command::new(env!("CARGO_BIN_EXE_nixspace"))
        .args([
            "_probe",
            "http",
            "--host",
            "127.0.0.1",
            "--port",
            &http_port.to_string(),
            "--path",
            "/health",
            "--expect-status",
            "204",
        ])
        .output()
        .unwrap();
    assert_success(&output);
    http_thread.join().unwrap();
}
