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
            "schemaVersion": 1,
            "runner": {
                "kind": "devenv-task",
                "direct": {"argv": ["devenv-flake-tasks", "run"], "requiredEnvironment": ["DEVENV_TASK_FILE", "NIXSPACE_INDEX", "NIXSPACE_WORKSPACE_ROOT"]},
                "bootstrap": {"argv": ["nix", "develop", ".#default", "--command", "devenv-flake-tasks", "run"]}
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
        fs::create_dir(root.join(".git")).expect("fake Git directory is writable");
        fs::write(root.join(".git/index"), "fake-index\n").expect("fake Git index is writable");

        let output = root.join("result");
        let generated = output.join("share/nixspace");
        fs::create_dir_all(&generated).expect("generated output directory is writable");
        fs::write(generated.join("index.json"), workspace_index(2))
            .expect("generated index is writable");

        let bin = root.join("bin");
        fs::create_dir(&bin).expect("fake binary directory is writable");
        write_executable(
            &bin.join("git"),
            r#"#!/bin/sh
printf '%s\n' "$*" >>"$GIT_LOG"
case "$1" in
  clone)
    source="$6"
    destination="$7"
    mkdir -p "$destination"
    cp "$source/flake.nix" "$destination/flake.nix"
    cp "$source/flake.lock" "$destination/flake.lock"
    ;;
  status)
    if [ -n "${GIT_DIRTY_DIRECTORY:-}" ] && [ "$PWD" = "$GIT_DIRTY_DIRECTORY" ]; then
      printf '%s\n' "${GIT_DIRTY_STATUS:-?? local-input/flake.nix}"
    elif [ -n "${GIT_STATUS:-}" ]; then
      printf '%s\n' "$GIT_STATUS"
    fi
    ;;
  rev-parse)
    if [ "$2" = "--git-path" ] && [ "$3" = index ]; then
      printf '%s\n' .git/index
    else
      printf '%s\n' "${GIT_TOPLEVEL:-$PWD}"
    fi
    ;;
  add)
    ;;
  write-tree)
    printf '%s\n' 1111111111111111111111111111111111111111
    ;;
  commit-tree)
    printf '%s\n' 2222222222222222222222222222222222222222
    ;;
  update-ref)
    ;;
  ls-tree)
    [ "${GIT_TRACKED:-1}" = 1 ] || exit 0
    for path do :; done
    case "$path" in
      *flake.nix|*flake.lock)
        printf '100644 blob 3333333333333333333333333333333333333333\t%s\0' "$path"
        ;;
      *)
        printf '040000 tree 4444444444444444444444444444444444444444\t%s\0' "$path"
        ;;
    esac
    ;;
  cat-file)
    case "$3" in
      *:flake.nix) cat "$GIT_DIR/flake.nix" ;;
      *:flake.lock) cat "$GIT_DIR/flake.lock" ;;
      *) printf 'unexpected fake cat-file object: %s\n' "$3" >&2; exit 92 ;;
    esac
    ;;
  *)
    printf 'unexpected fake git command: %s\n' "$*" >&2
    exit 91
    ;;
esac
"#,
        );
        write_executable(
            &bin.join("nix"),
            r#"#!/bin/sh
printf '%s\n' "$*" >>"$NIX_LOG"
if [ -n "${NIX_MUTATE_LOCK:-}" ]; then
  printf '%s\n' '{"mutated":true}' >"$NIX_MUTATE_LOCK"
fi
if [ -n "${NIX_SNAPSHOT_CAPTURE:-}" ]; then
  for installable do :; done
  location="${installable#git+file://}"
  repository="${location%%\?rev=*}"
  revision="${location#*\?rev=}"
  revision="${revision%%#*}"
  advertised="$(git ls-remote "$repository" refs/heads/nixspace-index-snapshot | awk '{print $1}')"
  [ "$advertised" = "$revision" ] || exit 25
  git --git-dir="$repository" show "$revision:flake.lock" >"$NIX_SNAPSHOT_CAPTURE" || exit 24
fi
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
            OsString::from("GIT_LOG"),
            root.join("git.log").into_os_string(),
        );
        environment.insert(
            OsString::from("NIX_LOG"),
            root.join("nix.log").into_os_string(),
        );
        environment.insert(OsString::from("NIX_OUTPUTS"), output.as_os_str().to_owned());
        environment.insert(
            OsString::from("NIXSPACE_INDEX"),
            root.join(".nixspace/index.json").into_os_string(),
        );
        environment.insert(OsString::from("NIXSPACE_FLAKE"), OsString::from("."));
        environment.insert(
            OsString::from("NIXSPACE_INDEX_INSTALLABLE"),
            OsString::from("nixspace-index"),
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

    fn git_log(&self) -> PathBuf {
        self.root.join("git.log")
    }

    fn nix_log(&self) -> PathBuf {
        self.root.join("nix.log")
    }

    fn assert_local_installable(&self, invocation: &str) {
        assert!(
            invocation.contains("git+file://") && invocation.contains("/nixspace-index-snapshot."),
            "{invocation}"
        );
        assert!(
            invocation.contains(
                "/snapshot.git?rev=2222222222222222222222222222222222222222#nixspace-index"
            ),
            "{invocation}"
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

fn real_git(root: &Path, arguments: &[&str]) -> Output {
    Command::new("git")
        .current_dir(root)
        .args(arguments)
        .output()
        .expect("real Git starts")
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
    assert!(!workspace.git_log().exists());
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
    assert!(!workspace.git_log().exists());
    assert!(!workspace.nix_log().exists());
}

#[test]
fn refresh_refuses_an_untracked_root_before_nix() {
    let mut workspace = Workspace::new();
    workspace.set("GIT_TRACKED", "0");

    let output = workspace.run(&["index", "refresh"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("refusing index refresh"));
    assert!(!workspace.nix_log().exists());
    assert!(!workspace.index.exists());
}

#[test]
fn refresh_filters_ignored_and_untracked_files_from_in_tree_path_inputs() {
    let mut workspace = Workspace::new();
    let local_input = workspace.root.join("local-input");
    fs::create_dir(&local_input).expect("local input directory is writable");
    fs::write(local_input.join("flake.nix"), "{ outputs = _: {}; }\n")
        .expect("local input flake is writable");
    fs::write(
        workspace.root.join("flake.lock"),
        serde_json::to_vec(&json!({
            "version": 7,
            "root": "root",
            "nodes": {
                "root": {"inputs": {"local": "local"}},
                "local": {
                    "locked": {"type": "path", "path": "./local-input"},
                    "original": {"type": "path", "path": "./local-input"}
                }
            }
        }))
        .expect("fixture lock serializes"),
    )
    .expect("fixture lock is writable");
    workspace.set("GIT_DIRTY_DIRECTORY", local_input.as_os_str().to_owned());

    let output = workspace.run(&["index", "refresh"]);

    assert_success(&output);
    let invocation = fs::read_to_string(workspace.nix_log()).unwrap();
    workspace.assert_local_installable(&invocation);
    assert!(!invocation.contains("path:"));
    assert!(!fs::read_to_string(workspace.git_log())
        .unwrap()
        .contains("status"));
}

#[test]
fn refresh_refuses_path_inputs_outside_the_filtered_worktree() {
    let workspace = Workspace::new();
    let external = workspace.root.with_extension("external-input");
    fs::create_dir(&external).unwrap();
    fs::write(external.join("flake.nix"), "{ outputs = _: {}; }\n").unwrap();
    fs::write(
        workspace.root.join("flake.lock"),
        serde_json::to_vec(&json!({
            "version": 7,
            "root": "root",
            "nodes": {
                "root": {"inputs": {"local": "local"}},
                "local": {
                    "locked": {"type": "path", "path": external},
                    "original": {"type": "path", "path": external}
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let output = workspace.run(&["index", "refresh"]);
    let _ = fs::remove_dir_all(&external);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("absolute and cannot be bound to the immutable snapshot"));
    assert!(!workspace.nix_log().exists());
}

#[test]
fn refresh_builds_exactly_once_validates_and_atomically_installs_exact_bytes() {
    let workspace = Workspace::new();
    let expected = fs::read(workspace.generated_index()).expect("generated index is readable");

    let output = workspace.run(&["index", "refresh"]);

    assert_success(&output);
    workspace.assert_local_installable(
        &fs::read_to_string(workspace.nix_log()).expect("nix log is readable"),
    );
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
fn refresh_builds_from_an_immutable_snapshot_without_mutating_git_state() {
    let mut workspace = Workspace::new();
    fs::remove_file(workspace.root.join("bin/git")).unwrap();
    fs::remove_dir_all(workspace.root.join(".git")).unwrap();
    assert!(real_git(&workspace.root, &["init", "--quiet"])
        .status
        .success());
    assert!(
        real_git(&workspace.root, &["add", "flake.nix", "flake.lock"])
            .status
            .success()
    );
    assert!(real_git(
        &workspace.root,
        &[
            "-c",
            "user.name=Nixspace Test",
            "-c",
            "user.email=nixspace@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ]
    )
    .status
    .success());
    let head_before = real_git(&workspace.root, &["rev-parse", "HEAD"]).stdout;
    let lock_before = fs::read(workspace.root.join("flake.lock")).unwrap();
    workspace.set(
        "NIX_MUTATE_LOCK",
        workspace.root.join("flake.lock").into_os_string(),
    );
    let captured_snapshot = workspace.root.join("captured-snapshot-lock");
    workspace.set(
        "NIX_SNAPSHOT_CAPTURE",
        captured_snapshot.clone().into_os_string(),
    );

    let output = workspace.run(&["index", "refresh"]);
    assert_success(&output);
    let invocation = fs::read_to_string(workspace.nix_log()).unwrap();
    let revision = invocation
        .split("?rev=")
        .nth(1)
        .and_then(|suffix| suffix.split('#').next())
        .expect("immutable revision is in the installable");
    assert_eq!(revision.len(), 40);
    assert!(invocation.contains("/snapshot.git?rev="));
    assert_eq!(fs::read(captured_snapshot).unwrap(), lock_before);
    assert_ne!(
        fs::read(workspace.root.join("flake.lock")).unwrap(),
        lock_before
    );
    assert_eq!(
        real_git(&workspace.root, &["rev-parse", "HEAD"]).stdout,
        head_before
    );
    assert!(real_git(&workspace.root, &["diff", "--cached", "--quiet"])
        .status
        .success());
    assert!(!real_git(&workspace.root, &["cat-file", "-e", revision])
        .status
        .success());
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
            "--flake",
            "github:example/workspace/0123456789abcdef",
            "--index-installable",
            "workspace-index",
            "--index-file",
            "share/example/workspace.json",
        ])
        .output()
        .expect("nixspace starts");

    assert_success(&output);
    assert_eq!(
        fs::read_to_string(workspace.nix_log()).expect("nix log is readable"),
        "build --no-link --print-out-paths github:example/workspace/0123456789abcdef#workspace-index\n"
    );
    assert!(!workspace.git_log().exists());
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
    workspace.assert_local_installable(
        &fs::read_to_string(workspace.nix_log()).expect("nix log is readable"),
    );
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
    workspace.assert_local_installable(
        &fs::read_to_string(workspace.nix_log()).expect("nix log is readable"),
    );
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
