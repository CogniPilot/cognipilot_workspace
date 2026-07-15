#![cfg(unix)]

use std::fs;
use std::net::TcpListener;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

const BEGIN: &str = "# BEGIN NIXSPACE MANAGED SETTINGS";
const END: &str = "# END NIXSPACE MANAGED SETTINGS";

struct Fixture {
    root: PathBuf,
    home: PathBuf,
    bin: PathBuf,
    plan: PathBuf,
    config: PathBuf,
    active: PathBuf,
    command_log: PathBuf,
    tool_state: PathBuf,
    nixos_marker: PathBuf,
    local_closure: PathBuf,
    remote_closure: PathBuf,
    secondary_remote_closure: PathBuf,
    cache_stdin: PathBuf,
    secondary_cache_stdin: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("nixspace-host-{}-{sequence}", std::process::id()));
        let home = root.join("home");
        let bin = root.join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir(&bin).unwrap();
        let plan = root.join("host-plan.json");
        let config = root.join("nix.conf");
        let active = root.join("active-config.json");
        let command_log = root.join("commands.log");
        let tool_state = root.join("tool-state");
        let nixos_marker = root.join("NIXOS");
        let local_closure = root.join("local-closure.json");
        let remote_closure = root.join("remote-closure.json");
        let secondary_remote_closure = root.join("secondary-remote-closure.json");
        let cache_stdin = root.join("cache-stdin.txt");
        let secondary_cache_stdin = root.join("secondary-cache-stdin.txt");
        fs::write(&plan, serde_json::to_vec(&host_plan()).unwrap()).unwrap();
        fs::write(&active, serde_json::to_vec(&active_config(true)).unwrap()).unwrap();
        write_executable(
            &bin.join("nix"),
            r#"#!/bin/sh
printf 'nix' >> "$NIXSPACE_TEST_COMMAND_LOG"
printf ' %s' "$@" >> "$NIXSPACE_TEST_COMMAND_LOG"
printf '\n' >> "$NIXSPACE_TEST_COMMAND_LOG"
case "$*" in
  "--version")
    printf 'nix (Nix) %s\n' "${NIXSPACE_TEST_NIX_VERSION:-2.34.7}"
    ;;
  "config show --json")
    cat "$NIXSPACE_TEST_ACTIVE_CONFIG"
    ;;
  "profile add --accept-flake-config example:formatter")
    if [ "${NIXSPACE_TEST_INSTALL_EXIT:-0}" -ne 0 ]; then
      exit "$NIXSPACE_TEST_INSTALL_EXIT"
    fi
    : > "$NIXSPACE_TEST_TOOL_STATE"
    ;;
  "path-info --json --recursive --size "*)
    cat "$NIXSPACE_TEST_LOCAL_CLOSURE"
    ;;
  "path-info --json --size --store https://cache.example.test --stdin")
    cat > "$NIXSPACE_TEST_CACHE_STDIN"
    cat "$NIXSPACE_TEST_REMOTE_CLOSURE"
    ;;
  "path-info --json --size --store https://cache-secondary.example.test --stdin")
    cat > "$NIXSPACE_TEST_SECONDARY_CACHE_STDIN"
    cat "$NIXSPACE_TEST_SECONDARY_REMOTE_CLOSURE"
    ;;
  *)
    printf 'unexpected nix argv: %s\n' "$*" >&2
    exit 97
    ;;
esac
"#,
        );
        write_executable(
            &bin.join("formatter"),
            r#"#!/bin/sh
printf 'formatter' >> "$NIXSPACE_TEST_COMMAND_LOG"
printf ' %s' "$@" >> "$NIXSPACE_TEST_COMMAND_LOG"
printf '\n' >> "$NIXSPACE_TEST_COMMAND_LOG"
if [ -e "$NIXSPACE_TEST_TOOL_STATE" ]; then
  printf 'formatter %s\n' "${NIXSPACE_TEST_TOOL_VERSION:-3.2.1 (test-system)}"
else
  printf 'formatter 1.0.0 (test-system)\n'
fi
"#,
        );
        write_executable(
            &bin.join("sudo"),
            r#"#!/bin/sh
printf 'sudo' >> "$NIXSPACE_TEST_COMMAND_LOG"
printf ' %s' "$@" >> "$NIXSPACE_TEST_COMMAND_LOG"
printf '\n' >> "$NIXSPACE_TEST_COMMAND_LOG"
exec "$@"
"#,
        );
        write_executable(
            &bin.join("systemctl"),
            r#"#!/bin/sh
printf 'systemctl' >> "$NIXSPACE_TEST_COMMAND_LOG"
printf ' %s' "$@" >> "$NIXSPACE_TEST_COMMAND_LOG"
printf '\n' >> "$NIXSPACE_TEST_COMMAND_LOG"
"#,
        );
        write_executable(
            &bin.join("launchctl"),
            r#"#!/bin/sh
printf 'launchctl' >> "$NIXSPACE_TEST_COMMAND_LOG"
printf ' %s' "$@" >> "$NIXSPACE_TEST_COMMAND_LOG"
printf '\n' >> "$NIXSPACE_TEST_COMMAND_LOG"
"#,
        );
        Self {
            root,
            home,
            bin,
            plan,
            config,
            active,
            command_log,
            tool_state,
            nixos_marker,
            local_closure,
            remote_closure,
            secondary_remote_closure,
            cache_stdin,
            secondary_cache_stdin,
        }
    }

    fn command(&self, command_name: &str) -> Command {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![self.bin.clone()];
        paths.extend(std::env::split_paths(&path));
        let mut command = Command::new(env!("CARGO_BIN_EXE_nixspace"));
        command
            .arg("--workspace-root")
            .arg(&self.root)
            .arg("--host-plan")
            .arg(&self.plan)
            .arg("--nix-config")
            .arg(&self.config)
            .arg(command_name)
            .env("HOME", &self.home)
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("NIXSPACE_TEST_COMMAND_LOG", &self.command_log)
            .env("NIXSPACE_TEST_ACTIVE_CONFIG", &self.active)
            .env("NIXSPACE_TEST_TOOL_STATE", &self.tool_state)
            .env("NIXSPACE_TEST_LOCAL_CLOSURE", &self.local_closure)
            .env("NIXSPACE_TEST_REMOTE_CLOSURE", &self.remote_closure)
            .env(
                "NIXSPACE_TEST_SECONDARY_REMOTE_CLOSURE",
                &self.secondary_remote_closure,
            )
            .env("NIXSPACE_TEST_CACHE_STDIN", &self.cache_stdin)
            .env(
                "NIXSPACE_TEST_SECONDARY_CACHE_STDIN",
                &self.secondary_cache_stdin,
            )
            .env("NIXSPACE_NIXOS_MARKER", &self.nixos_marker)
            .env("NIXSPACE_NIX_MODE", "single-user")
            .env_remove("NIXSPACE_TEST_INSTALL_EXIT")
            .env_remove("NIXSPACE_TEST_NIX_VERSION")
            .env_remove("NIXSPACE_TEST_TOOL_VERSION");
        command
    }

    fn run(&self, command_name: &str, arguments: &[&str]) -> Output {
        self.command(command_name)
            .args(arguments)
            .output()
            .expect("nixspace starts")
    }

    fn log(&self) -> String {
        fs::read_to_string(&self.command_log).unwrap_or_default()
    }

    fn write_plan(&self, plan: &Value) {
        fs::write(&self.plan, serde_json::to_vec(plan).unwrap()).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn host_plan() -> Value {
    json!({
        "apiVersion": "nixspace/v1",
        "kind": "Host",
        "interfaceVersion": 4,
        "nix": {
            "minimumVersion": "2.18",
            "settings": {
                "accept-flake-config": true,
                "experimental-features": ["nix-command", "flakes"],
                "extra-substituters": ["https://cache.example.test"],
                "extra-trusted-public-keys": ["cache.example.test-1:AAAAAAAA="],
                "trusted-users": ["*"]
            }
        },
        "readiness": {
            "storage": {"path": ".", "minimumAvailableBytes": 0},
            "daemon": {"when": "never", "probeArgv": []},
            "requiredDocuments": [],
            "sourceSelection": "default",
            "launch": {
                "allowActiveSessions": true,
                "requireManagerSocket": true,
                "requireAvailableDeclaredPorts": true
            },
            "cache": {
                "storeDirectory": "/nix/store",
                "roots": [],
                "stores": [{
                    "name": "public",
                    "uri": "https://cache.example.test"
                }],
                "coverageMode": "union"
            }
        },
        "tools": {
            "formatter": {
                "executable": "formatter",
                "expectedVersion": "3.2.1",
                "versionArgv": ["formatter", "version"],
                "installArgv": [
                    "nix", "profile", "add", "--accept-flake-config", "example:formatter"
                ]
            }
        }
    })
}

#[test]
fn credential_bearing_nix_settings_are_rejected_before_host_commands_run() {
    let fixture = Fixture::new();
    for name in [
        "access-tokens",
        "netrc-file",
        "secret-key-files",
        "registry-credential",
        "mytokenized-setting",
        "PASSWORD_HELPER",
    ] {
        let mut plan = host_plan();
        plan["nix"]["settings"][name] = json!("must-never-enter-managed-configuration");
        fixture.write_plan(&plan);
        let output = fixture.run("setup", &["--check"]);
        assert!(!output.status.success(), "setting `{name}` was accepted");
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("must not be embedded in a Nix-generated host plan"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!fixture.config.exists());
        assert!(!fixture.command_log.exists());
    }
}

#[test]
fn substituter_settings_reject_credentials_and_non_public_uris() {
    let fixture = Fixture::new();
    for value in [
        "https://user:must-never-leak@cache.example.test",
        "https://cache.example.test?token=secret",
        "https://cache.example.test#private",
        "http://cache.example.test",
        "https://cache.example.test other",
        "https://cache.example.test\nhttps://other.example.test",
    ] {
        let mut plan = host_plan();
        plan["nix"]["settings"]["extra-substituters"] = json!([value]);
        fixture.write_plan(&plan);
        let output = fixture.run("setup", &["--check"]);
        assert!(
            !output.status.success(),
            "substituter `{value}` was accepted"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("credential-free HTTPS caches"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains(value));
        assert!(!String::from_utf8_lossy(&output.stderr).contains(value));
        assert!(!fixture.config.exists());
        assert!(!fixture.command_log.exists());
    }
}

#[test]
fn cache_stores_use_the_deduplicated_union_of_both_substituter_settings() {
    let fixture = Fixture::new();
    fs::write(&fixture.tool_state, "current").unwrap();
    let mut plan = host_plan();
    plan["nix"]["settings"]["substituters"] = json!([
        "https://cache-primary.example.test",
        "https://cache-primary.example.test"
    ]);
    plan["nix"]["settings"]["extra-substituters"] = json!([
        "https://cache-secondary.example.test",
        "https://cache-secondary.example.test"
    ]);
    plan["readiness"]["cache"]["stores"][0]["uri"] = json!("https://cache-primary.example.test");
    fixture.write_plan(&plan);
    let mut active = active_config(true);
    active["substituters"]["value"] = json!([
        "https://cache.nixos.org/",
        "https://cache-primary.example.test",
        "https://cache-secondary.example.test"
    ]);
    fs::write(&fixture.active, serde_json::to_vec(&active).unwrap()).unwrap();

    let output = fixture.run("setup", &[]);
    assert_success(&output);
    let configured = fs::read_to_string(&fixture.config).unwrap();
    assert!(configured.contains("substituters = https://cache-primary.example.test"));
    assert!(configured.contains("extra-substituters = https://cache-secondary.example.test"));

    let output = fixture.run("setup", &["--check"]);
    assert_success(&output);
}

fn active_config(compliant: bool) -> Value {
    json!({
        "accept-flake-config": {"value": compliant},
        "experimental-features": {
            "value": if compliant { json!(["flakes", "fetch-tree", "nix-command"]) } else { json!([]) }
        },
        "substituters": {
            "value": if compliant { json!(["https://cache.nixos.org/", "https://cache.example.test"]) } else { json!(["https://cache.nixos.org/"]) }
        },
        "trusted-public-keys": {
            "value": if compliant { json!(["cache.nixos.org-1:default", "cache.example.test-1:AAAAAAAA="]) } else { json!(["cache.nixos.org-1:default"]) }
        },
        "trusted-users": {"value": if compliant { json!(["root", "*"]) } else { json!(["root"]) }}
    })
}

fn source_plan() -> Value {
    let git = |argv: Vec<&str>| json!({"argv": argv});
    json!({
        "apiVersion": "nixspace/v1",
        "kind": "SourceWorkspace",
            "interfaceVersion": 3,
        "workspaceRoot": ".",
        "transaction": {"mutationLock": "state/source.lock", "journal": "state/source.json"},
        "repositories": {
            "alpha": {
                "id": "alpha",
                "packages": ["alpha"],
                "path": "checkouts/alpha",
                "source": {},
                "git": {
                    "url": "https://example.test/alpha.git",
                    "branch": "main",
                    "clone": git(vec!["git", "clone"]),
                    "status": git(vec!["git", "status"]),
                    "inspect": {
                        "worktree": git(vec!["git", "worktree"]),
                        "origin": git(vec!["git", "origin"]),
                        "branch": git(vec!["git", "branch"]),
                        "clean": git(vec!["git", "conflicts"]),
                        "head": git(vec!["git", "head"]),
                        "target": git(vec!["git", "target"])
                    },
                    "fetch": git(vec!["git", "fetch"]),
                    "fastForwardCheck": git(vec!["git", "ff-check"]),
                    "fastForward": git(vec!["git", "ff"]),
                        "rollback": {
                            "refUpdate": {"argv": [
                                {"kind": "literal", "value": "git"},
                                {"kind": "old-head"},
                                {"kind": "expected-current"}
                            ]},
                            "worktreeRestore": {"argv": [
                                {"kind": "literal", "value": "git"},
                                {"kind": "expected-current"},
                                {"kind": "old-head"}
                            ]},
                            "refRestore": {"argv": [
                                {"kind": "literal", "value": "git"},
                                {"kind": "expected-current"},
                                {"kind": "old-head"}
                            ]}
                        }
                }
            }
        },
        "plans": {"default": ["alpha"], "all": ["alpha"], "packages": {"alpha": ["alpha"]}}
    })
}

fn launch_plan(port: u16) -> Value {
    let command = json!({"argv": ["manager"]});
    json!({
        "apiVersion": "nixspace/v1",
        "kind": "LaunchExecution",
        "interfaceVersion": 4,
        "stateRoot": "state/sessions",
        "sessionLayout": {
            "metadata": "session.json",
            "managerSocket": "process-compose.sock",
            "managerLog": "processes.log",
            "portAllocationLock": ".port-allocation.lock"
        },
        "launches": {
            "app/stack": {
                "coordinate": "app:stack",
                "workspaceLaunch": "app/stack",
                "parameters": {},
                "declaredPorts": [{
                    "launch": "app/stack",
                    "process": "server",
                    "endpoint": "http",
                    "protocol": "http",
                    "transport": "tcp",
                    "host": "127.0.0.1",
                    "port": port,
                    "hostEnvironment": null,
                    "portEnvironment": "STATIC_PORT"
                }],
                "sessionEnvironment": {},
                "processPolicies": {},
                "runner": {
                    "kind": "devenv-process-compose",
                    "workingDirectory": ".",
                    "sessionRootEnvironment": "NIXSPACE_SESSION_DIR",
                    "commands": {
                        "up": command, "start": command, "attach": command,
                        "status": command, "logs": command, "down": command
                    }
                }
            }
        }
    })
}

fn session_record(root: &Path, port: u16) -> Value {
    let socket = root.join("state/sessions/demo/process-compose.sock");
    json!({
        "apiVersion": "nixspace/v1",
        "kind": "LaunchSession",
        "interfaceVersion": 4,
        "id": "demo",
        "coordinate": "app/stack",
        "schemaCoordinate": "app:stack",
        "createdUnixMs": 1,
        "workingDirectory": root,
        "socket": socket,
        "log": root.join("state/sessions/demo/processes.log"),
        "commands": {"attach": ["manager"], "status": ["manager"], "logs": ["manager"], "down": ["manager"]},
        "parameterEnvironments": [],
        "processPolicies": {},
        "resolvedLaunch": {
            "processes": [{
                "id": "server",
                "endpoints": {"http": {"protocol": "http", "host": "127.0.0.1", "port": port}}
            }]
        }
    })
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
fn doctor_is_read_only_uses_exact_commands_and_reports_root_equivalent_trust() {
    let fixture = Fixture::new();
    fs::write(&fixture.tool_state, "current").unwrap();
    let output = fixture.run("doctor", &["--json"]);
    assert_success(&output);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["kind"], "HostDiagnostics");
    assert_eq!(report["interfaceVersion"], 4);
    assert_eq!(report["compliant"], true);
    assert_eq!(report["nix"]["version"], "2.34.7");
    assert_eq!(report["nix"]["settings"][2]["actualName"], "substituters");
    assert_eq!(report["tools"]["formatter"]["version"], "3.2.1");
    assert!(report["securityWarnings"][0]
        .as_str()
        .unwrap()
        .contains("root-equivalent"));
    assert_eq!(
        fixture.log(),
        "nix --version\nnix config show --json\nformatter version\n"
    );
    assert!(!fixture.config.exists());
}

#[test]
fn doctor_accepts_semver_build_metadata_from_a_pinned_tool() {
    let fixture = Fixture::new();
    fs::write(&fixture.tool_state, "current").unwrap();
    let output = fixture
        .command("doctor")
        .arg("--json")
        .env("NIXSPACE_TEST_TOOL_VERSION", "3.2.1+407080fe (test-system)")
        .output()
        .unwrap();
    assert_success(&output);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["tools"]["formatter"]["version"], "3.2.1");
    assert_eq!(report["tools"]["formatter"]["satisfied"], true);
}

#[test]
fn doctor_reports_exact_cache_coverage_without_claiming_transport_bytes() {
    let fixture = Fixture::new();
    fs::write(&fixture.tool_state, "current").unwrap();
    let store = fixture.root.join("fake-store");
    let store_root = store.join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-public-root");
    fs::create_dir(&store).unwrap();
    fs::create_dir(&store_root).unwrap();
    symlink(&store_root, fixture.root.join("result-public-cache-root")).unwrap();
    fs::write(
        &fixture.local_closure,
        serde_json::to_vec(&json!({
            "version": 2,
            "storeDir": "/nix/store",
            "info": {
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-one": {"narSize": 100},
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-two": {"narSize": 250}
            }
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        &fixture.remote_closure,
        serde_json::to_vec(&json!({
            "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-one": {
                "narSize": 100,
                "downloadSize": 61
            },
            "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-two": null
        }))
        .unwrap(),
    )
    .unwrap();
    let mut plan = host_plan();
    plan["readiness"]["cache"] = json!({
        "storeDirectory": store,
        "roots": [{"name": "public-cache-root", "path": "result-public-cache-root"}],
        "stores": [{"name": "public", "uri": "https://cache.example.test"}],
        "coverageMode": "union"
    });
    fixture.write_plan(&plan);

    let output = fixture
        .command("doctor")
        .arg("--json")
        .env("CACHIX_AUTH_TOKEN", "must-never-appear")
        .output()
        .unwrap();
    assert_success(&output);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["readiness"]["cache"]["coverageMode"], "union");
    let root = &report["readiness"]["cache"]["roots"][0];
    assert_eq!(root["observed"], true);
    assert_eq!(root["pathCount"], 2);
    assert_eq!(root["narBytes"], 350);
    assert_eq!(root["union"]["observedStoreCount"], 1);
    assert_eq!(root["union"]["hitPathCount"], 1);
    assert_eq!(root["union"]["missPathCount"], 1);
    assert_eq!(root["union"]["hitNarBytes"], 100);
    assert_eq!(root["union"]["missNarBytes"], 250);
    assert_eq!(root["union"]["cacheDownloadBytes"], 61);
    assert!(root["union"]["transferredBytes"].is_null());
    let store = &root["stores"][0];
    assert_eq!(store["observed"], true);
    assert_eq!(store["hitPathCount"], 1);
    assert_eq!(store["missPathCount"], 1);
    assert_eq!(store["hitNarBytes"], 100);
    assert_eq!(store["missNarBytes"], 250);
    assert_eq!(store["cacheDownloadBytes"], 61);
    assert!(store["transferredBytes"].is_null());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("must-never-appear"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("must-never-appear"));
    assert_eq!(
        fs::read_to_string(&fixture.cache_stdin).unwrap(),
        "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-one\n/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-two\n"
    );
    let log = fixture.log();
    assert!(log.contains("path-info --json --recursive --size"));
    assert!(log.contains("path-info --json --size --store https://cache.example.test --stdin"));

    let text = fixture.run("doctor", &[]);
    assert_success(&text);
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(text.contains("1 hit paths / 1 miss paths"));
    assert!(text.contains("100 hit NAR bytes / 250 miss NAR bytes"));
    assert!(text.contains("61 cache download bytes"));
    assert!(text.contains("transferred bytes unavailable"));
}

#[test]
fn cache_coverage_isolated_command_retains_json_summary_and_enforces_completeness() {
    let fixture = Fixture::new();
    let store = fixture.root.join("fake-store");
    let store_root = store.join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-public-root");
    fs::create_dir(&store).unwrap();
    fs::create_dir(&store_root).unwrap();
    symlink(&store_root, fixture.root.join("result-public-cache-root")).unwrap();
    fs::write(
        &fixture.local_closure,
        serde_json::to_vec(&json!({
            "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-one": {"narSize": 100},
            "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-two": {"narSize": 250}
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        &fixture.remote_closure,
        serde_json::to_vec(&json!({
            "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-one": {
                "narSize": 100,
                "downloadSize": 61
            },
            "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-two": {
                "narSize": 250,
                "downloadSize": 89
            }
        }))
        .unwrap(),
    )
    .unwrap();
    let mut plan = host_plan();
    plan["readiness"]["cache"] = json!({
        "storeDirectory": store,
        "roots": [{"name": "public-cache-root", "path": "result-public-cache-root"}],
        "stores": [{"name": "public", "uri": "https://cache.example.test"}],
        "coverageMode": "union"
    });
    fixture.write_plan(&plan);
    let report_path = fixture.root.join("reports/cache-coverage.json");
    let summary_path = fixture.root.join("step-summary.md");
    fs::write(&summary_path, "existing summary\n").unwrap();

    let output = fixture
        .command("cache")
        .args([
            "coverage",
            "--json",
            "--require-complete",
            "--output",
            report_path.to_str().unwrap(),
            "--summary",
            summary_path.to_str().unwrap(),
        ])
        .env("CACHIX_AUTH_TOKEN", "must-never-appear")
        .env_remove("HOME")
        .output()
        .unwrap();
    assert_success(&output);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let retained: Value = serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
    assert_eq!(retained, report);
    assert_eq!(report["kind"], "CacheCoverage");
    assert_eq!(report["interfaceVersion"], 2);
    assert_eq!(report["complete"], true);
    assert_eq!(report["cache"]["coverageMode"], "union");
    assert_eq!(
        report["cache"]["roots"][0]["union"]["observedStoreCount"],
        1
    );
    assert_eq!(report["cache"]["roots"][0]["union"]["hitPathCount"], 2);
    assert_eq!(report["cache"]["roots"][0]["union"]["missPathCount"], 0);
    assert_eq!(report["cache"]["roots"][0]["union"]["hitNarBytes"], 350);
    assert_eq!(report["cache"]["roots"][0]["union"]["missNarBytes"], 0);
    assert_eq!(
        report["cache"]["roots"][0]["union"]["cacheDownloadBytes"],
        150
    );
    assert!(report["cache"]["roots"][0]["union"]["transferredBytes"].is_null());
    assert_eq!(
        report["cache"]["roots"][0]["stores"][0]["cacheDownloadBytes"],
        150
    );
    assert!(report["cache"]["roots"][0]["stores"][0]["transferredBytes"].is_null());
    let summary = fs::read_to_string(&summary_path).unwrap();
    assert!(summary.starts_with("existing summary\n"));
    assert!(summary.contains(
        "| public-cache-root | union (1 observed) | 2 | 0 | 350 | 0 | 150 | unavailable |"
    ));
    assert!(
        summary.contains("| public-cache-root | public | 2 | 0 | 350 | 0 | 150 | unavailable |")
    );
    assert!(summary.contains("Coverage result: **complete**"));
    assert!(summary.contains(
        "sums each covered path once, using the least `downloadSize` advertised by any observed store"
    ));
    assert!(!summary.contains("must-never-appear"));
    let log = fixture.log();
    assert!(log.contains("path-info --json --recursive --size"));
    assert!(log.contains("path-info --json --size --store"));
    assert!(!log.contains("nix --version"));
    assert!(!log.contains("config show"));
    assert!(!log.contains("formatter"));

    fs::write(
        &fixture.remote_closure,
        serde_json::to_vec(&json!({
            "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-one": {
                "narSize": 100,
                "downloadSize": 61
            },
            "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-two": null
        }))
        .unwrap(),
    )
    .unwrap();
    let incomplete_path = fixture.root.join("reports/incomplete.json");
    let incomplete = fixture
        .command("cache")
        .args([
            "coverage",
            "--json",
            "--require-complete",
            "--output",
            incomplete_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(incomplete.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&incomplete.stdout).unwrap();
    assert_eq!(report["complete"], false);
    assert_eq!(report["cache"]["roots"][0]["union"]["hitPathCount"], 1);
    assert_eq!(report["cache"]["roots"][0]["union"]["missPathCount"], 1);
    assert_eq!(report["cache"]["roots"][0]["union"]["hitNarBytes"], 100);
    assert_eq!(report["cache"]["roots"][0]["union"]["missNarBytes"], 250);
    assert_eq!(report["cache"]["roots"][0]["stores"][0]["missPathCount"], 1);
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&incomplete_path).unwrap()).unwrap(),
        report
    );
}

#[test]
fn cache_coverage_unions_paths_across_stores_without_double_counting() {
    let fixture = Fixture::new();
    let store = fixture.root.join("fake-store");
    let store_root = store.join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-public-root");
    fs::create_dir(&store).unwrap();
    fs::create_dir(&store_root).unwrap();
    symlink(&store_root, fixture.root.join("result-public-cache-root")).unwrap();
    fs::write(
        &fixture.local_closure,
        serde_json::to_vec(&json!({
            "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-one": {"narSize": 100},
            "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-two": {"narSize": 250},
            "/nix/store/cccccccccccccccccccccccccccccccc-three": {"narSize": 400}
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        &fixture.remote_closure,
        serde_json::to_vec(&json!({
            "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-one": {
                "narSize": 100,
                "downloadSize": 70
            },
            "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-two": {
                "narSize": 250,
                "downloadSize": 90
            },
            "/nix/store/cccccccccccccccccccccccccccccccc-three": null
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        &fixture.secondary_remote_closure,
        serde_json::to_vec(&json!({
            "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-one": {
                "narSize": 100,
                "downloadSize": 61
            },
            "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-two": null,
            "/nix/store/cccccccccccccccccccccccccccccccc-three": {
                "narSize": 400,
                "downloadSize": 110
            }
        }))
        .unwrap(),
    )
    .unwrap();
    let mut plan = host_plan();
    plan["nix"]["settings"]["extra-substituters"] = json!([
        "https://cache.example.test",
        "https://cache-secondary.example.test"
    ]);
    plan["readiness"]["cache"] = json!({
        "storeDirectory": store,
        "roots": [{"name": "public-cache-root", "path": "result-public-cache-root"}],
        "stores": [
            {"name": "primary", "uri": "https://cache.example.test"},
            {"name": "secondary", "uri": "https://cache-secondary.example.test"}
        ],
        "coverageMode": "union"
    });
    fixture.write_plan(&plan);

    let output = fixture.run("cache", &["coverage", "--json", "--require-complete"]);
    assert_success(&output);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["interfaceVersion"], 2);
    assert_eq!(report["complete"], true);
    let root = &report["cache"]["roots"][0];
    assert_eq!(root["pathCount"], 3);
    assert_eq!(root["narBytes"], 750);
    assert_eq!(root["union"]["observedStoreCount"], 2);
    assert_eq!(root["union"]["hitPathCount"], 3);
    assert_eq!(root["union"]["missPathCount"], 0);
    assert_eq!(root["union"]["hitNarBytes"], 750);
    assert_eq!(root["union"]["missNarBytes"], 0);
    // The shared first path contributes the least advertised size (61), once.
    assert_eq!(root["union"]["cacheDownloadBytes"], 261);
    assert!(root["union"]["transferredBytes"].is_null());
    assert_eq!(root["stores"][0]["hitPathCount"], 2);
    assert_eq!(root["stores"][0]["missPathCount"], 1);
    assert_eq!(root["stores"][0]["cacheDownloadBytes"], 160);
    assert_eq!(root["stores"][1]["hitPathCount"], 2);
    assert_eq!(root["stores"][1]["missPathCount"], 1);
    assert_eq!(root["stores"][1]["cacheDownloadBytes"], 171);
    assert_eq!(
        fs::read_to_string(&fixture.cache_stdin).unwrap(),
        fs::read_to_string(&fixture.secondary_cache_stdin).unwrap()
    );
}

#[test]
fn doctor_skips_absent_cache_roots_without_evaluation_or_network() {
    let fixture = Fixture::new();
    fs::write(&fixture.tool_state, "current").unwrap();
    let mut plan = host_plan();
    plan["readiness"]["cache"] = json!({
        "storeDirectory": "/nix/store",
        "roots": [{"name": "public-cache-root", "path": "result-public-cache-root"}],
        "stores": [{"name": "public", "uri": "https://cache.example.test"}],
        "coverageMode": "union"
    });
    fixture.write_plan(&plan);

    let output = fixture.run("doctor", &["--json"]);
    assert_success(&output);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let root = &report["readiness"]["cache"]["roots"][0];
    assert_eq!(root["observed"], false);
    assert!(root["error"].as_str().unwrap().contains("not realized"));
    assert!(!fixture.log().contains("path-info"));
}

#[test]
fn host_v4_rejects_token_bearing_cache_uris_without_echoing_them() {
    let fixture = Fixture::new();
    let mut plan = host_plan();
    plan["readiness"]["cache"] = json!({
        "storeDirectory": "/nix/store",
        "roots": [],
        "stores": [{
            "name": "private",
            "uri": "https://sensitive-token@cache.example.test?auth=sensitive-token"
        }],
        "coverageMode": "union"
    });
    fixture.write_plan(&plan);

    let output = fixture.run("doctor", &["--json"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("credential-free public HTTPS URLs"));
    assert!(!stderr.contains("sensitive-token"));
    assert_eq!(fixture.log(), "");
}

#[test]
fn host_v4_has_no_v3_compatibility_fallback() {
    let fixture = Fixture::new();
    let mut plan = host_plan();
    plan["interfaceVersion"] = json!(3);
    fixture.write_plan(&plan);

    let output = fixture.run("doctor", &["--json"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("interface version 3 is unsupported"));
    assert!(stderr.contains("supports version 4"));
    assert_eq!(fixture.log(), "");
}

#[test]
fn host_v4_requires_union_cache_coverage_mode_without_fallback() {
    let fixture = Fixture::new();
    let mut plan = host_plan();
    plan["readiness"]["cache"]
        .as_object_mut()
        .unwrap()
        .remove("coverageMode")
        .unwrap();
    fixture.write_plan(&plan);

    let missing = fixture.run("doctor", &["--json"]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("missing field `coverageMode`"));
    assert_eq!(fixture.log(), "");

    plan["readiness"]["cache"]["coverageMode"] = json!("per-store");
    fixture.write_plan(&plan);
    let unsupported = fixture.run("doctor", &["--json"]);
    assert!(!unsupported.status.success());
    let stderr = String::from_utf8_lossy(&unsupported.stderr);
    assert!(stderr.contains("unknown variant `per-store`"));
    assert!(stderr.contains("expected `union`"));
    assert_eq!(fixture.log(), "");

    plan["readiness"]["cache"]["coverageMode"] = json!("union");
    plan["readiness"]["cache"]["stores"] = json!([]);
    fixture.write_plan(&plan);
    let no_stores = fixture.run("doctor", &["--json"]);
    assert!(!no_stores.status.success());
    assert!(String::from_utf8_lossy(&no_stores.stderr)
        .contains("must declare at least one union store"));
    assert_eq!(fixture.log(), "");
}

#[test]
fn host_v4_rejects_missing_nix_normalized_tools() {
    let fixture = Fixture::new();
    let mut plan = host_plan();
    plan.as_object_mut().unwrap().remove("tools").unwrap();
    fixture.write_plan(&plan);

    let output = fixture.run("doctor", &["--json"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing field `tools`"));
    assert_eq!(fixture.log(), "");
}

#[test]
fn doctor_returns_nonzero_for_old_nix_missing_settings_and_wrong_tool() {
    let fixture = Fixture::new();
    fs::write(
        &fixture.active,
        serde_json::to_vec(&active_config(false)).unwrap(),
    )
    .unwrap();
    let output = fixture
        .command("doctor")
        .arg("--json")
        .env("NIXSPACE_TEST_NIX_VERSION", "2.17.9")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["compliant"], false);
    assert_eq!(report["nix"]["versionSatisfied"], false);
    assert_eq!(report["tools"]["formatter"]["satisfied"], false);
    assert!(report["nix"]["settings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|setting| setting["satisfied"] == false));
}

#[test]
fn setup_preserves_other_lines_and_backup_installs_tool_then_becomes_idempotent() {
    let fixture = Fixture::new();
    let original = "keep-this = exactly\n# an unmanaged comment\n";
    fs::write(&fixture.config, original).unwrap();
    let output = fixture.run("setup", &[]);
    assert_success(&output);
    let configured = fs::read_to_string(&fixture.config).unwrap();
    assert!(configured.starts_with(original));
    assert!(configured.contains(BEGIN));
    assert!(configured.contains("extra-substituters = https://cache.example.test"));
    assert!(configured.contains("trusted-users = *"));
    assert!(configured.ends_with(&format!("{END}\n")));
    assert_eq!(
        fs::read_to_string(format!("{}.pre-nixspace", fixture.config.display())).unwrap(),
        original
    );
    assert!(fixture.tool_state.exists());
    assert!(fixture
        .log()
        .contains("nix profile add --accept-flake-config example:formatter\n"));
    assert!(!fixture.log().contains("sudo "));
    assert!(!fixture.log().contains("systemctl "));

    let first_log = fixture.log();
    let output = fixture.run("setup", &[]);
    assert_success(&output);
    assert_eq!(fs::read_to_string(&fixture.config).unwrap(), configured);
    let added_log = &fixture.log()[first_log.len()..];
    assert!(!added_log.contains("profile add"));
    assert!(!added_log.contains("systemctl"));
}

#[test]
fn setup_check_verifies_without_mutating_installing_or_restarting() {
    let fixture = Fixture::new();
    fs::write(&fixture.config, "unmanaged = true\n").unwrap();
    let output = fixture.run("setup", &["--check"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        fs::read_to_string(&fixture.config).unwrap(),
        "unmanaged = true\n"
    );
    assert!(!fixture.tool_state.exists());
    let log = fixture.log();
    assert!(!log.contains("profile add"));
    assert!(!log.contains("systemctl"));
    assert!(!log.contains("sudo"));
}

#[test]
fn daemon_setup_restarts_exactly_once_and_only_after_a_mutation() {
    let fixture = Fixture::new();
    fs::write(&fixture.tool_state, "current").unwrap();
    let mut command = fixture.command("setup");
    let output = command.args(["--nix-mode", "daemon"]).output().unwrap();
    assert_success(&output);
    let log = fixture.log();
    assert!(log.contains("sudo systemctl restart nix-daemon.service\n"));
    assert!(log.contains("systemctl restart nix-daemon.service\n"));
    let restart = log.find("sudo systemctl restart").unwrap();
    assert!(restart < log.find("formatter version").unwrap());

    let first_log = log;
    let output = fixture
        .command("setup")
        .args(["--nix-mode", "daemon"])
        .output()
        .unwrap();
    assert_success(&output);
    assert!(!fixture.log()[first_log.len()..].contains("systemctl"));
}

#[test]
fn setup_refuses_symlink_and_nixos_declarative_configuration() {
    let fixture = Fixture::new();
    let declarative = fixture.root.join("declarative.conf");
    fs::write(&declarative, "managed elsewhere\n").unwrap();
    symlink(&declarative, &fixture.config).unwrap();
    let check = fixture.run("setup", &["--check"]);
    assert_eq!(check.status.code(), Some(1));
    assert!(!String::from_utf8_lossy(&check.stderr).contains("refusing to update"));
    assert!(fixture.log().contains("nix config show --json"));
    fs::remove_file(&fixture.command_log).unwrap();

    let output = fixture.run("setup", &[]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("symlink"));
    assert_eq!(
        fs::read_to_string(&declarative).unwrap(),
        "managed elsewhere\n"
    );

    fs::remove_file(&fixture.config).unwrap();
    fs::write(&fixture.nixos_marker, "").unwrap();
    let output = fixture
        .command("setup")
        .args(["--nix-mode", "daemon"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("NixOS marker"));
    assert!(!fixture.config.exists());
}

#[test]
fn setup_check_accepts_a_compliant_declaratively_managed_host() {
    let fixture = Fixture::new();
    fs::write(&fixture.tool_state, "current").unwrap();
    let declarative = fixture.root.join("declarative.conf");
    fs::write(&declarative, "managed elsewhere\n").unwrap();
    symlink(&declarative, &fixture.config).unwrap();

    let output = fixture.run("setup", &["--check"]);

    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("Host: compliant"));
    assert_eq!(
        fs::read_to_string(&declarative).unwrap(),
        "managed elsewhere\n"
    );
    assert!(!fixture.log().contains("profile add"));
    assert!(!fixture.log().contains("systemctl"));
}

#[test]
fn failed_generated_tool_installer_propagates_status_without_claiming_success() {
    let fixture = Fixture::new();
    let output = fixture
        .command("setup")
        .env("NIXSPACE_TEST_INSTALL_EXIT", "27")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(27));
    assert!(!fixture.tool_state.exists());
    assert!(fixture
        .log()
        .contains("nix profile add --accept-flake-config example:formatter"));
}

#[test]
fn environment_can_select_the_plan_and_config_paths_explicitly() {
    let fixture = Fixture::new();
    fs::write(&fixture.tool_state, "current").unwrap();
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![fixture.bin.clone()];
    paths.extend(std::env::split_paths(&path));
    let output = Command::new(env!("CARGO_BIN_EXE_nixspace"))
        .arg("--workspace-root")
        .arg(&fixture.root)
        .arg("doctor")
        .arg("--json")
        .env("HOME", &fixture.home)
        .env("PATH", std::env::join_paths(paths).unwrap())
        .env("NIXSPACE_HOST_PLAN", &fixture.plan)
        .env("NIXSPACE_NIX_CONFIG", &fixture.config)
        .env("NIXSPACE_NIX_MODE", "single-user")
        .env("NIXSPACE_TEST_COMMAND_LOG", &fixture.command_log)
        .env("NIXSPACE_TEST_ACTIVE_CONFIG", &fixture.active)
        .env("NIXSPACE_TEST_TOOL_STATE", &fixture.tool_state)
        .env("NIXSPACE_NIXOS_MARKER", &fixture.nixos_marker)
        .output()
        .unwrap();
    assert_success(&output);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["plan"], fixture.plan.display().to_string());
    assert_eq!(
        report["configuration"]["path"],
        fixture.config.display().to_string()
    );
}

#[test]
fn doctor_applies_nix_generated_storage_and_daemon_expectations() {
    let fixture = Fixture::new();
    fs::write(&fixture.tool_state, "current").unwrap();
    write_executable(&fixture.bin.join("daemon-probe"), "#!/bin/sh\nexit 0\n");
    let mut plan = host_plan();
    plan["readiness"]["storage"]["minimumAvailableBytes"] = json!(u64::MAX);
    plan["readiness"]["daemon"] = json!({
        "when": "always",
        "probeArgv": ["daemon-probe", "ping"]
    });
    fixture.write_plan(&plan);

    let output = fixture.run("doctor", &["--json"]);
    assert_eq!(output.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["readiness"]["storage"]["satisfied"], false);
    assert_eq!(report["readiness"]["daemon"]["checked"], true);
    assert_eq!(report["readiness"]["daemon"]["satisfied"], true);
    assert!(report["readiness"]["storage"]["availableBytes"].is_u64());

    plan["readiness"]["storage"]["minimumAvailableBytes"] = json!(0);
    fixture.write_plan(&plan);
    write_executable(&fixture.bin.join("daemon-probe"), "#!/bin/sh\nexit 23\n");
    let output = fixture.run("doctor", &["--json"]);
    assert_eq!(output.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["readiness"]["storage"]["satisfied"], true);
    assert_eq!(report["readiness"]["daemon"]["satisfied"], false);
}

#[test]
fn doctor_reports_missing_selected_sources_and_exact_git_conflicts() {
    let fixture = Fixture::new();
    fs::write(&fixture.tool_state, "current").unwrap();
    let source = fixture.root.join("source-plan.json");
    fs::write(&source, serde_json::to_vec(&source_plan()).unwrap()).unwrap();
    let mut plan = host_plan();
    plan["readiness"]["requiredDocuments"] = json!(["source"]);
    fixture.write_plan(&plan);
    write_executable(
        &fixture.bin.join("git"),
        "#!/bin/sh\n[ \"$1\" = conflicts ] || exit 97\n[ ! -e checkouts/alpha/.conflicts ] || cat checkouts/alpha/.conflicts\n",
    );

    let output = fixture
        .command("doctor")
        .args(["--source-plan", source.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["readiness"]["documents"]["source"]["ready"], true);
    assert_eq!(
        report["readiness"]["source"]["repositories"][0]["present"],
        false
    );

    fs::create_dir_all(fixture.root.join("checkouts/alpha")).unwrap();
    fs::write(
        fixture.root.join("checkouts/alpha/.conflicts"),
        "UU src/main.rs\n M unrelated.txt\n",
    )
    .unwrap();
    let output = fixture
        .command("doctor")
        .args(["--source-plan", source.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report["readiness"]["source"]["repositories"][0]["conflicts"],
        json!(["src/main.rs"])
    );

    fs::remove_file(fixture.root.join("checkouts/alpha/.conflicts")).unwrap();
    let output = fixture
        .command("doctor")
        .args(["--source-plan", source.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert_success(&output);
}

#[test]
fn doctor_distinguishes_unclaimed_ports_from_nixspace_sessions() {
    let fixture = Fixture::new();
    fs::write(&fixture.tool_state, "current").unwrap();
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let launch = fixture.root.join("launch-plan.json");
    fs::write(&launch, serde_json::to_vec(&launch_plan(port)).unwrap()).unwrap();
    let mut plan = host_plan();
    plan["readiness"]["requiredDocuments"] = json!(["launch"]);
    fixture.write_plan(&plan);

    let output = fixture
        .command("doctor")
        .args(["--launch-plan", launch.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report["readiness"]["launch"]["ports"][0]["available"],
        false
    );
    assert_eq!(
        report["readiness"]["launch"]["ports"][0]["claimedBySessions"],
        json!([])
    );

    let session = fixture.root.join("state/sessions/demo");
    fs::create_dir_all(&session).unwrap();
    fs::write(session.join("process-compose.sock"), "socket marker").unwrap();
    fs::write(
        session.join("session.json"),
        serde_json::to_vec(&session_record(&fixture.root, port)).unwrap(),
    )
    .unwrap();
    let output = fixture
        .command("doctor")
        .args(["--launch-plan", launch.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert_success(&output);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["readiness"]["launch"]["sessions"][0]["id"], "demo");
    assert_eq!(
        report["readiness"]["launch"]["ports"][0]["claimedBySessions"],
        json!(["demo"])
    );
}

#[test]
fn doctor_requires_configured_generated_documents_to_be_offline_ready() {
    let fixture = Fixture::new();
    fs::write(&fixture.tool_state, "current").unwrap();
    let mut plan = host_plan();
    plan["readiness"]["requiredDocuments"] = json!(["index"]);
    fixture.write_plan(&plan);
    let missing = fixture.root.join("missing-index.json");
    let output = fixture
        .command("doctor")
        .args(["--index", missing.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["readiness"]["documents"]["index"]["ready"], false);
    assert!(report["readiness"]["documents"]["index"]["error"]
        .as_str()
        .unwrap()
        .contains("unavailable"));
}

#[test]
fn doctor_validates_the_configured_workspace_resolution_document() {
    let fixture = Fixture::new();
    fs::write(&fixture.tool_state, "current").unwrap();
    let resolution = fixture.root.join("resolution.json");
    let document = json!({
        "apiVersion": "nixspace/v1",
        "kind": "WorkspaceResolution",
        "interfaceVersion": 2,
        "roots": {"workspace": ".", "taskState": ".state"},
        "packagePlans": {},
        "packages": {},
        "artifacts": {},
        "resources": {},
        "executables": {},
        "actionBindings": {}
    });
    fs::write(&resolution, serde_json::to_vec(&document).unwrap()).unwrap();
    let mut plan = host_plan();
    plan["readiness"]["requiredDocuments"] = json!(["resolution"]);
    fixture.write_plan(&plan);

    let ready = fixture
        .command("doctor")
        .args(["--resolution-plan", resolution.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert_success(&ready);
    let report: Value = serde_json::from_slice(&ready.stdout).unwrap();
    assert_eq!(
        report["readiness"]["documents"]["resolution"]["ready"],
        true
    );

    let mut wrong = document;
    wrong["kind"] = json!("WorkspaceResolutionTemplate");
    fs::write(&resolution, serde_json::to_vec(&wrong).unwrap()).unwrap();
    let rejected = fixture
        .command("doctor")
        .args(["--resolution-plan", resolution.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert_eq!(rejected.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&rejected.stdout).unwrap();
    assert_eq!(
        report["readiness"]["documents"]["resolution"]["ready"],
        false
    );
}
