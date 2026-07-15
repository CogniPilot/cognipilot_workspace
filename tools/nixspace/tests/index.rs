#![cfg(unix)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};

static NEXT_WORKSPACE: AtomicU64 = AtomicU64::new(0);

fn workspace_index(interface_version: u64) -> Vec<u8> {
    let document = json!({
        "apiVersion": "nixspace/v1",
        "kind": "Workspace",
        "interfaceVersion": interface_version,
        "catalog": {
            "packages": [],
            "targets": [],
            "artifacts": [],
            "resources": [],
            "executables": [],
            "launches": []
        },
        "graph": {
            "schemaVersion": 1,
            "all": {"nodes": [], "edges": []},
            "packages": {},
            "reverse": {}
        },
        "launchPlans": {},
        "actionPlans": {
            "schemaVersion": 2,
            "runner": {
                "kind": "devenv-task",
                "direct": {"argv": ["devenv-flake-tasks", "run"], "requiredEnvironment": ["DEVENV_TASK_FILE", "NIXSPACE_INDEX", "NIXSPACE_WORKSPACE_ROOT"]},
                "bootstrap": {
                    "argv": ["nix", "develop", ".#default", "--command", "devenv-flake-tasks", "run"],
                    "environment": {"WORKSPACE_ROOT": "workspace-root"}
                }
            },
            "actions": {
                "build": {"all": [], "packages": {}},
                "test": {"all": [], "packages": {}}
            }
        }
    });
    let mut bytes = serde_json::to_vec_pretty(&document).expect("fixture serializes");
    bytes.push(b'\n');
    bytes
}

struct Workspace {
    root: PathBuf,
    output: PathBuf,
    index: PathBuf,
    environment: BTreeMap<OsString, OsString>,
}

impl Workspace {
    fn new() -> Self {
        let sequence = NEXT_WORKSPACE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "nixspace-index-test-{}-{sequence}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("workspace directory is writable");
        fs::write(root.join("flake.nix"), "{ outputs = _: {}; }\n")
            .expect("fixture flake is writable");
        fs::write(
            root.join("flake.lock"),
            serde_json::to_vec(&json!({
                "version": 7,
                "root": "root",
                "nodes": {"root": {"inputs": {}}}
            }))
            .expect("fixture lock serializes"),
        )
        .expect("fixture lock is writable");

        let output = root.join("result");
        let generated = output.join("share/nixspace");
        fs::create_dir_all(&generated).expect("generated output directory is writable");
        fs::write(generated.join("index.json"), workspace_index(2))
            .expect("generated index is writable");

        let bin = root.join("bin");
        fs::create_dir(&bin).expect("fake binary directory is writable");
        write_executable(
            &bin.join("nix"),
            r#"#!/bin/sh
printf 'cwd=%s\n' "$PWD" >>"$NIX_LOG"
for argument do
  printf 'arg=%s\n' "$argument" >>"$NIX_LOG"
done
if [ "${NIX_FAIL:-0}" = 1 ]; then
  printf 'synthetic nix failure\n' >&2
  exit 23
fi
printf '%s\n' "$NIX_OUTPUTS"
"#,
        );

        let original_path = std::env::var_os("PATH").unwrap_or_default();
        let mut path = OsString::from(bin.as_os_str());
        path.push(":");
        path.push(original_path);
        let mut environment = BTreeMap::new();
        environment.insert(
            OsString::from("NIXSPACE_WORKSPACE_ROOT"),
            root.as_os_str().to_owned(),
        );
        environment.insert(OsString::from("PATH"), path);
        environment.insert(
            OsString::from("NIX_LOG"),
            root.join("nix.log").into_os_string(),
        );
        environment.insert(OsString::from("NIX_OUTPUTS"), output.as_os_str().to_owned());
        environment.insert(
            OsString::from("NIXSPACE_INDEX"),
            root.join(".nixspace/index.json").into_os_string(),
        );
        environment.insert(
            OsString::from("NIXSPACE_INDEX_INSTALLABLE"),
            OsString::from(".#nixspace-index"),
        );
        environment.insert(
            OsString::from("NIXSPACE_INDEX_FILE"),
            OsString::from("share/nixspace/index.json"),
        );

        Self {
            index: root.join(".nixspace/index.json"),
            root,
            output,
            environment,
        }
    }

    fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_nixspace"));
        command
            .current_dir(&self.root)
            .envs(&self.environment)
            .args(arguments);
        command
    }

    fn run(&self, arguments: &[&str]) -> Output {
        self.command(arguments).output().expect("nixspace starts")
    }

    fn set(&mut self, name: &str, value: impl Into<OsString>) {
        self.environment.insert(OsString::from(name), value.into());
    }

    fn generated_index(&self) -> PathBuf {
        self.output.join("share/nixspace/index.json")
    }

    fn nix_log(&self) -> PathBuf {
        self.root.join("nix.log")
    }

    fn assert_default_invocation(&self) {
        assert_eq!(
            fs::read_to_string(self.nix_log()).expect("nix log is readable"),
            format!(
                "cwd={}\narg=build\narg=--no-link\narg=--print-out-paths\narg=--\narg=.#nixspace-index\n",
                self.root.display()
            )
        );
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).expect("fake executable is writable");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .expect("fake executable permissions are writable");
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
fn path_and_status_are_filesystem_only() {
    let workspace = Workspace::new();

    let path = workspace.run(&["index", "path"]);
    let missing = workspace.run(&["index", "status"]);
    fs::create_dir_all(workspace.index.parent().expect("index has a parent"))
        .expect("state directory is writable");
    fs::write(&workspace.index, "status does not parse this\n").expect("cached index is writable");
    let ready = workspace.run(&["index", "status"]);

    assert_success(&path);
    assert_eq!(
        String::from_utf8_lossy(&path.stdout),
        format!("{}\n", workspace.index.display())
    );
    assert!(!missing.status.success());
    assert_eq!(
        String::from_utf8_lossy(&missing.stdout),
        format!("missing: {}\n", workspace.index.display())
    );
    assert_success(&ready);
    assert_eq!(
        String::from_utf8_lossy(&ready.stdout),
        format!("ready: {}\n", workspace.index.display())
    );
    assert!(!workspace.nix_log().exists());
}

#[test]
fn completion_exposes_only_the_three_index_operations() {
    let workspace = Workspace::new();
    let output = workspace.run(&["_complete", "index", ""]);

    assert_success(&output);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "refresh\nstatus\npath\n"
    );
    assert!(!workspace.nix_log().exists());
}

#[test]
fn refresh_configuration_is_explicit_and_rejects_legacy_split_coordinates() {
    let workspace = Workspace::new();

    let missing_installable = workspace
        .command(&["index", "refresh"])
        .env_remove("NIXSPACE_INDEX_INSTALLABLE")
        .output()
        .expect("nixspace starts");
    let empty_installable = workspace
        .command(&["index", "refresh"])
        .env("NIXSPACE_INDEX_INSTALLABLE", "")
        .output()
        .expect("nixspace starts");
    let missing_index_file = workspace
        .command(&["index", "refresh"])
        .env_remove("NIXSPACE_INDEX_FILE")
        .output()
        .expect("nixspace starts");
    let unsafe_index_file = workspace.run(&["index", "refresh", "--index-file", "../index.json"]);

    for output in [
        missing_installable,
        empty_installable,
        missing_index_file,
        unsafe_index_file,
    ] {
        assert!(!output.status.success());
    }
    for legacy in ["--flake", "--index-installable"] {
        let output = workspace.run(&["index", "refresh", legacy, "legacy-value"]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("unexpected argument"));
    }
    assert!(!workspace.nix_log().exists());
    assert!(!workspace.index.exists());
}

#[test]
fn refresh_passes_the_complete_installable_to_nix_without_source_inspection() {
    let mut workspace = Workspace::new();
    let git_marker = workspace.root.join("git-was-invoked");
    write_executable(
        &workspace.root.join("bin/git"),
        "#!/bin/sh\nprintf 'called\\n' >\"$GIT_MARKER\"\nexit 97\n",
    );
    workspace.set("GIT_MARKER", git_marker.as_os_str().to_owned());
    let installable = "git+https://example.invalid/product?rev=abc123&dir=workspace#custom-index";

    let output = workspace.run(&["index", "refresh", "--installable", installable]);

    assert_success(&output);
    assert!(!git_marker.exists());
    assert_eq!(
        fs::read_to_string(workspace.nix_log()).expect("nix log is readable"),
        format!(
            "cwd={}\narg=build\narg=--no-link\narg=--print-out-paths\narg=--\narg={installable}\n",
            workspace.root.display()
        )
    );
}

#[test]
fn refresh_builds_exactly_once_validates_and_atomically_installs_exact_bytes() {
    let workspace = Workspace::new();
    let expected = fs::read(workspace.generated_index()).expect("generated index is readable");

    let output = workspace.run(&["index", "refresh"]);

    assert_success(&output);
    workspace.assert_default_invocation();
    assert_eq!(
        fs::read(&workspace.index).expect("cached index is readable"),
        expected
    );
    assert_eq!(
        fs::metadata(&workspace.index)
            .expect("cached index metadata is readable")
            .mode()
            & 0o777,
        0o644
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("refreshed: {}\n", workspace.index.display())
    );
    let temporary_files: Vec<_> = fs::read_dir(
        workspace
            .index
            .parent()
            .expect("cached index has a state directory"),
    )
    .expect("state directory is readable")
    .filter_map(std::result::Result::ok)
    .filter(|entry| {
        entry
            .file_name()
            .to_string_lossy()
            .starts_with(".nixspace-index.")
    })
    .collect();
    assert!(temporary_files.is_empty());
}

#[test]
fn remote_flake_output_file_and_cache_are_explicitly_configurable() {
    let workspace = Workspace::new();
    let generated = workspace.output.join("share/example/workspace.json");
    fs::create_dir_all(generated.parent().expect("generated file has a parent"))
        .expect("custom generated directory is writable");
    fs::copy(workspace.generated_index(), &generated).expect("custom generated index is writable");
    let cache = workspace.root.join("state/custom-index.json");

    let output = workspace
        .command(&[
            "--index",
            cache.to_str().expect("temporary paths are UTF-8"),
            "index",
            "refresh",
            "--installable",
            "github:example/workspace/0123456789abcdef#workspace-index",
            "--index-file",
            "share/example/workspace.json",
        ])
        .output()
        .expect("nixspace starts");

    assert_success(&output);
    assert_eq!(
        fs::read_to_string(workspace.nix_log()).expect("nix log is readable"),
        format!(
            "cwd={}\narg=build\narg=--no-link\narg=--print-out-paths\narg=--\narg=github:example/workspace/0123456789abcdef#workspace-index\n",
            workspace.root.display()
        )
    );
    assert_eq!(
        fs::read(cache).expect("custom cached index is readable"),
        fs::read(generated).expect("custom generated index is readable")
    );
}

#[test]
fn invalid_built_index_never_replaces_the_existing_cache() {
    let workspace = Workspace::new();
    fs::create_dir_all(workspace.index.parent().expect("index has a parent"))
        .expect("state directory is writable");
    fs::write(&workspace.index, "existing\n").expect("existing cache is writable");
    fs::write(workspace.generated_index(), "{}\n").expect("generated index is writable");

    let output = workspace.run(&["index", "refresh"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("validation failed"));
    assert_eq!(
        fs::read_to_string(&workspace.index).expect("existing cache is readable"),
        "existing\n"
    );
    workspace.assert_default_invocation();
}

#[test]
fn unsupported_interface_never_replaces_the_existing_cache() {
    let workspace = Workspace::new();
    fs::create_dir_all(workspace.index.parent().expect("index has a parent"))
        .expect("state directory is writable");
    fs::write(&workspace.index, "existing\n").expect("existing cache is writable");
    fs::write(workspace.generated_index(), workspace_index(1))
        .expect("generated index is writable");

    let output = workspace.run(&["index", "refresh"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("interface version 1 is unsupported"));
    assert_eq!(
        fs::read_to_string(&workspace.index).expect("existing cache is readable"),
        "existing\n"
    );
}

#[test]
fn transport_identity_has_no_compatibility_fallback() {
    let workspace = Workspace::new();
    let base: Value = serde_json::from_slice(&workspace_index(2)).expect("fixture index is JSON");
    for (field, value, diagnostic) in [
        (
            "apiVersion",
            json!("example/v1"),
            "workspace API `example/v1` is unsupported",
        ),
        (
            "kind",
            json!("Project"),
            "workspace kind `Project` is unsupported",
        ),
    ] {
        let mut document = base.clone();
        document[field] = value;
        fs::write(
            workspace.generated_index(),
            serde_json::to_vec(&document).expect("invalid fixture serializes"),
        )
        .expect("generated index is writable");

        let output = workspace.run(&["index", "refresh"]);

        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(diagnostic),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!workspace.index.exists());
    }
}

#[test]
fn failed_build_never_replaces_the_existing_cache() {
    let mut workspace = Workspace::new();
    fs::create_dir_all(workspace.index.parent().expect("index has a parent"))
        .expect("state directory is writable");
    fs::write(&workspace.index, "existing\n").expect("existing cache is writable");
    workspace.set("NIX_FAIL", "1");

    let output = workspace.run(&["index", "refresh"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("build failed"));
    assert_eq!(
        fs::read_to_string(&workspace.index).expect("existing cache is readable"),
        "existing\n"
    );
    workspace.assert_default_invocation();
}

#[test]
fn multiple_output_paths_are_rejected_without_replacing_the_cache() {
    let mut workspace = Workspace::new();
    fs::create_dir_all(workspace.index.parent().expect("index has a parent"))
        .expect("state directory is writable");
    fs::write(&workspace.index, "existing\n").expect("existing cache is writable");
    workspace.set(
        "NIX_OUTPUTS",
        format!("{}\n/nix/store/unexpected", workspace.output.display()),
    );

    let output = workspace.run(&["index", "refresh"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("returned 2 output paths"));
    assert_eq!(
        fs::read_to_string(&workspace.index).expect("existing cache is readable"),
        "existing\n"
    );
}

#[test]
fn index_cache_location_honors_explicit_override() {
    let workspace = Workspace::new();
    let override_path = workspace.root.join("query-only.json");
    let output = workspace
        .command(&[
            "--index",
            override_path.to_str().expect("temporary paths are UTF-8"),
            "index",
            "path",
        ])
        .output()
        .expect("nixspace starts");

    assert_success(&output);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("{}\n", override_path.display())
    );
}

#[test]
fn fixture_is_a_valid_v2_document() {
    let parsed: Value = serde_json::from_slice(&workspace_index(2)).expect("fixture is JSON");
    assert_eq!(parsed["apiVersion"], "nixspace/v1");
    assert_eq!(parsed["kind"], "Workspace");
    assert_eq!(parsed["interfaceVersion"], 2);
}
