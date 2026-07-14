use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    plan: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let sequence = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "nixspace-west-cli-{}-{sequence}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src/telemetry")).unwrap();
        let manifest = root.join("west.yml");
        fs::write(&manifest, "manifest:\n  projects: []\n").unwrap();
        let plan = root.join("west-plan.json");
        let document = json!({
            "apiVersion": "nixspace/v1",
            "kind": "WestWorkspace",
            "interfaceVersion": 1,
            "product": {
                "id": "test-product",
                "interfaceVersion": 1
            },
            "workspaceRoot": ".",
            "workspace": {
                "id": "test-app",
                "source": {
                    "input": "test_source",
                    "root": ".",
                    "identity": {
                        "storePath": root.display().to_string(),
                        "narHash": null,
                        "rev": "1111111111111111111111111111111111111111"
                    }
                },
                "manifest": {
                    "resource": "test-app:west-manifest",
                    "relativePath": "west.yml",
                    "storePath": manifest.display().to_string(),
                    "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                },
                "contentKey": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
            },
            "cache": {
                "layoutVersion": 1,
                "namespace": "test-product",
                "root": {"base": "platform-cache", "path": "nixspace/test-product"},
                "nativePathCache": true,
                "narrowUpdate": true
            },
            "localView": {
                "root": {"base": "workspace", "path": ".devenv/state/nixspace/west/views"},
                "overrides": [
                    {
                        "project": "telemetry",
                        "source": "src/telemetry",
                        "required": false,
                        "zephyrModule": true
                    }
                ],
                "policyId": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            },
            "tools": {
                "west": std::env::current_exe().unwrap().display().to_string()
            }
        });
        fs::write(&plan, serde_json::to_vec_pretty(&document).unwrap()).unwrap();
        Self { root, plan }
    }

    fn command(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_nixspace"))
            .arg("--west-plan")
            .arg(&self.plan)
            .arg("west")
            .args(arguments)
            .current_dir(&self.root)
            .env("NIXSPACE_WORKSPACE_ROOT", &self.root)
            .env("XDG_CACHE_HOME", self.root.join("cache"))
            .output()
            .unwrap()
    }

    fn document(&self) -> Value {
        serde_json::from_slice(&fs::read(&self.plan).unwrap()).unwrap()
    }

    fn write_document(&self, document: &Value) {
        fs::write(&self.plan, serde_json::to_vec_pretty(document).unwrap()).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

#[test]
fn validate_consumes_the_versioned_nix_plan_without_a_project_index() {
    let fixture = Fixture::new();
    let output = fixture.command(&["validate", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let status: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["interfaceVersion"], 1);
    assert_eq!(status["product"], "test-product");
    assert_eq!(status["workspace"], "test-app");
    assert_eq!(status["ready"], false);
}

#[test]
fn paths_are_content_addressed_and_the_editable_view_is_not_src_west() {
    let fixture = Fixture::new();
    let locked = fixture.command(&["path", "--mode", "release"]);
    let local = fixture.command(&["path", "--mode", "local"]);
    assert!(locked.status.success(), "{}", stderr(&locked));
    assert!(local.status.success(), "{}", stderr(&local));

    let locked = PathBuf::from(stdout(&locked).trim());
    let local = PathBuf::from(stdout(&local).trim());
    assert!(locked.ends_with(Path::new(
        "workspaces/cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc/locked"
    )));
    assert!(local.starts_with(
        fixture
            .root
            .join(".devenv/state/nixspace/west/views/test-product")
    ));
    assert!(!local.ends_with(Path::new("src/.west")));
    assert!(!fixture.root.join("src/.west").exists());
}

#[test]
fn extra_modules_are_declared_by_nix_instead_of_discovered_by_rust() {
    let fixture = Fixture::new();
    let local = fixture.command(&["extra-modules", "--mode", "local"]);
    let release = fixture.command(&["extra-modules", "--mode", "release"]);
    assert!(local.status.success(), "{}", stderr(&local));
    assert!(release.status.success(), "{}", stderr(&release));
    assert_eq!(
        stdout(&local).trim(),
        fixture.root.join("src/telemetry").display().to_string()
    );
    assert_eq!(stdout(&release), "\n");
}

#[test]
fn unsupported_plan_versions_have_no_compatibility_path() {
    let fixture = Fixture::new();
    let mut document = fixture.document();
    document["interfaceVersion"] = 0.into();
    fixture.write_document(&document);

    let output = fixture.command(&["status"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("interface version 0 is unsupported"));
}

#[test]
fn west_transport_identity_has_no_compatibility_path() {
    let fixture = Fixture::new();
    let base = fixture.document();
    for (field, value, diagnostic) in [
        (
            "apiVersion",
            json!("example/v1"),
            "West plan API `example/v1` is unsupported",
        ),
        (
            "kind",
            json!("Workspace"),
            "West plan kind `Workspace` is unsupported",
        ),
    ] {
        let mut document = base.clone();
        document[field] = value;
        fixture.write_document(&document);

        let output = fixture.command(&["status"]);

        assert!(!output.status.success());
        assert!(stderr(&output).contains(diagnostic), "{}", stderr(&output));
    }
}
