use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    plan: PathBuf,
    attrs: PathBuf,
    output: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "nixspace-closure-materialization-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        Self {
            plan: root.join("plan.json"),
            attrs: root.join("structured-attrs.json"),
            output: root.join("result/materialized.json"),
            root,
        }
    }

    fn write(&self, plan: Value, attrs: Value) {
        fs::write(&self.plan, serde_json::to_vec(&plan).unwrap()).unwrap();
        fs::write(&self.attrs, serde_json::to_vec(&attrs).unwrap()).unwrap();
    }

    fn run(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_nixspace"))
            .args([
                "_materialize-closure",
                "--plan-file",
                self.plan.to_str().unwrap(),
                "--structured-attrs",
                self.attrs.to_str().unwrap(),
                "--output",
                self.output.to_str().unwrap(),
            ])
            .output()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn plan(interface_version: u64) -> Value {
    json!({
        "apiVersion": "nixspace/v1",
        "kind": "ClosureMaterialization",
        "interfaceVersion": interface_version,
        "closureAttribute": "releaseGraph",
        "closureRecordsPointer": "/provenance/closure",
        "proofBindings": [{
            "sourceOutputPointer": "/primary",
            "destinationDigestPointer": "/provenance/digest/sha256",
            "transform": "nix-nar-sha256-to-hex"
        }],
        "document": {
            "primary": {
                "drvPath": "/nix/store/primary.drv",
                "storePath": "/nix/store/primary"
            },
            "nested": [{
                "output": {
                    "drvPath": "/nix/store/dependency.drv",
                    "storePath": "/nix/store/dependency"
                }
            }],
            "input": {"storePath": "/nix/store/source-only"},
            "provenance": {"closure": null, "digest": {"sha256": null}}
        }
    })
}

fn attrs() -> Value {
    json!({
        "ignored": {"value": true},
        "releaseGraph": [
            {
                "path": "/nix/store/primary",
                "narHash": "sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=",
                "narSize": 20,
                "closureSize": 30,
                "valid": true,
                "references": ["/nix/store/dependency"]
            },
            {
                "path": "/nix/store/dependency",
                "narHash": "sha256-z4PhNX7vuL3xVChQ1m2AB9Yg5AULVxXcg/SpIdNs6c0=",
                "narSize": 10,
                "closureSize": 10,
                "valid": true,
                "references": []
            }
        ]
    })
}

#[test]
fn hidden_command_materializes_the_declared_generic_closure_atomically() {
    let fixture = Fixture::new();
    fixture.write(plan(2), attrs());

    let output = fixture.run();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&fs::read(&fixture.output).unwrap()).unwrap();
    assert_eq!(
        result["primary"]["narHash"],
        "sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU="
    );
    assert_eq!(result["primary"]["narSize"], 20);
    assert_eq!(
        result["nested"][0]["output"]["narHash"],
        "sha256-z4PhNX7vuL3xVChQ1m2AB9Yg5AULVxXcg/SpIdNs6c0="
    );
    assert_eq!(
        result["provenance"]["digest"]["sha256"],
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert!(result["input"].get("narHash").is_none());
    assert_eq!(
        result["provenance"]["closure"][0]["path"],
        "/nix/store/dependency"
    );
    assert!(fs::read_dir(fixture.output.parent().unwrap())
        .unwrap()
        .all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".tmp-")));
}

#[test]
fn hidden_command_rejects_other_versions_without_writing_or_fallback() {
    let fixture = Fixture::new();
    fixture.write(plan(1), attrs());

    let output = fixture.run();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("closure-materialization interface version 1 is unsupported"));
    assert!(!fixture.output.exists());
}
