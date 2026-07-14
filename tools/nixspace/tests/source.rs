#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    bin: PathBuf,
    plan: PathBuf,
    log: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("nixspace-source-{}-{sequence}", std::process::id()));
        let bin = root.join("bin");
        fs::create_dir_all(root.join("checkouts")).unwrap();
        fs::create_dir(&bin).unwrap();
        let plan = root.join("source-plan.json");
        let log = root.join("git.log");
        fs::write(&plan, serde_json::to_vec(&source_plan()).unwrap()).unwrap();
        write_executable(
            &bin.join("git"),
            r#"#!/bin/sh
printf 'git' >> "$NIXSPACE_TEST_GIT_LOG"
printf ' %s' "$@" >> "$NIXSPACE_TEST_GIT_LOG"
printf '\n' >> "$NIXSPACE_TEST_GIT_LOG"

if [ "$1" = clone ]; then
  branch="$5"
  url="$7"
  path="$8"
  mkdir -p "$path"
  printf '%s\n' "$url" > "$path/.origin"
  printf '%s\n' "$branch" > "$path/.branch"
  printf 'cloned %s\n' "$path"
  exit 0
fi

if [ "$1" != -C ]; then
  printf 'unexpected argv: %s\n' "$*" >&2
  exit 96
fi
path="$2"
shift 2
case "$*" in
  "rev-parse --show-toplevel")
    [ -d "$path" ] || exit 91
    (cd "$path" && pwd -P)
    ;;
  "remote get-url origin")
    cat "$path/.origin"
    ;;
  "symbolic-ref --quiet --short HEAD")
    cat "$path/.branch"
    ;;
  "status --porcelain=v1 --untracked-files=normal")
    [ ! -e "$path/.dirty" ] || cat "$path/.dirty"
    ;;
  "status --short --branch")
    printf '## %s...origin/%s\n' "$(cat "$path/.branch")" "$(cat "$path/.branch")"
    ;;
  "fetch --prune origin "*)
    if [ "${NIXSPACE_TEST_FETCH_FAIL:-}" = "$path" ]; then
      exit 29
    fi
    : > "$path/.fetched"
    ;;
  "merge-base --is-ancestor HEAD origin/"*)
    if [ "${NIXSPACE_TEST_NON_FF:-}" = "$path" ]; then
      exit 1
    fi
    ;;
  "merge --ff-only origin/"*)
    : > "$path/.merged"
    ;;
  *)
    printf 'unexpected argv after -C %s: %s\n' "$path" "$*" >&2
    exit 95
    ;;
esac
"#,
        );
        Self {
            root,
            bin,
            plan,
            log,
        }
    }

    fn command(&self, name: &str) -> Command {
        let mut paths = vec![self.bin.clone()];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let mut command = Command::new(env!("CARGO_BIN_EXE_nixspace"));
        command
            .arg("--workspace-root")
            .arg(&self.root)
            .arg("--source-plan")
            .arg(&self.plan)
            .arg(name)
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("NIXSPACE_TEST_GIT_LOG", &self.log)
            .env_remove("NIXSPACE_TEST_FETCH_FAIL")
            .env_remove("NIXSPACE_TEST_NON_FF");
        command
    }

    fn run(&self, name: &str, arguments: &[&str]) -> Output {
        self.command(name).args(arguments).output().unwrap()
    }

    fn checkout(&self, id: &str) -> PathBuf {
        self.root.join("checkouts").join(id)
    }

    fn create_checkout(&self, id: &str) {
        let path = self.checkout(id);
        fs::create_dir_all(&path).unwrap();
        fs::write(
            path.join(".origin"),
            format!("https://example.test/{id}.git\n"),
        )
        .unwrap();
        let branch = if id == "gamma" { "develop" } else { "main" };
        fs::write(path.join(".branch"), format!("{branch}\n")).unwrap();
    }

    fn log(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn clear_log(&self) {
        let _ = fs::remove_file(&self.log);
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

fn source_plan() -> Value {
    json!({
        "apiVersion": "nixspace/v1",
        "kind": "SourceWorkspace",
        "interfaceVersion": 1,
        "workspaceRoot": ".",
        "repositories": {
            "alpha": repository("alpha", "main"),
            "beta": repository("beta", "main"),
            "gamma": repository("gamma", "develop")
        },
        "plans": {
            "default": ["alpha", "beta"],
            "all": ["alpha", "beta", "gamma"],
            "packages": {
                "alpha": ["alpha"],
                "application": ["alpha", "beta", "gamma"]
            }
        }
    })
}

fn repository(id: &str, branch: &str) -> Value {
    let path = format!("checkouts/{id}");
    let url = format!("https://example.test/{id}.git");
    json!({
        "id": id,
        "packages": [id],
        "path": path,
        "source": {
            "input": format!("{id}-source"),
            "roots": ["."],
            "locked": {"type": "github", "rev": "0000000000000000000000000000000000000000", "narHash": "sha256-example"}
        },
        "git": {
            "url": url,
            "branch": branch,
            "clone": {"argv": ["git", "clone", "--origin", "origin", "--branch", branch, "--", url, path]},
            "status": {"argv": ["git", "-C", path, "status", "--short", "--branch"]},
            "inspect": {
                "worktree": {"argv": ["git", "-C", path, "rev-parse", "--show-toplevel"]},
                "origin": {"argv": ["git", "-C", path, "remote", "get-url", "origin"]},
                "branch": {"argv": ["git", "-C", path, "symbolic-ref", "--quiet", "--short", "HEAD"]},
                "clean": {"argv": ["git", "-C", path, "status", "--porcelain=v1", "--untracked-files=normal"]}
            },
            "fetch": {"argv": ["git", "-C", path, "fetch", "--prune", "origin", branch]},
            "fastForwardCheck": {"argv": ["git", "-C", path, "merge-base", "--is-ancestor", "HEAD", format!("origin/{branch}")]},
            "fastForward": {"argv": ["git", "-C", path, "merge", "--ff-only", format!("origin/{branch}")]}
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
fn sync_preflights_every_selected_path_before_cloning_only_missing_repositories() {
    let fixture = Fixture::new();
    fixture.create_checkout("alpha");
    fixture.create_checkout("gamma");
    fs::write(
        fixture.checkout("gamma").join(".origin"),
        "https://wrong.test/repo.git\n",
    )
    .unwrap();

    let output = fixture.run("sync", &["all"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("origin mismatch"));
    assert!(!fixture.checkout("beta").exists());
    assert!(!fixture.log().contains("git clone"));
    assert!(fixture
        .log()
        .contains("git -C checkouts/gamma remote get-url origin"));

    fs::write(
        fixture.checkout("gamma").join(".origin"),
        "https://example.test/gamma.git\n",
    )
    .unwrap();
    fixture.clear_log();
    let output = fixture.run("sync", &[]);
    assert_success(&output);
    assert!(fixture.checkout("beta").is_dir());
    assert_eq!(
        fixture
            .log()
            .lines()
            .filter(|line| line.starts_with("git clone"))
            .collect::<Vec<_>>(),
        ["git clone --origin origin --branch main -- https://example.test/beta.git checkouts/beta"]
    );
}

#[test]
fn status_streams_every_existing_repository_and_reports_missing_paths() {
    let fixture = Fixture::new();
    fixture.create_checkout("alpha");
    fixture.create_checkout("gamma");
    let output = fixture.run("status", &["all"]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("alpha\t"));
    assert!(stdout.contains("## main...origin/main"));
    assert!(stdout.contains("beta\tmissing\t"));
    assert!(stdout.contains("gamma\t"));
    assert!(stdout.contains("## develop...origin/develop"));
    let log = fixture.log();
    assert!(log.contains("git -C checkouts/alpha status --short --branch"));
    assert!(!log.contains("git -C checkouts/beta status --short --branch"));
    assert!(log.contains("git -C checkouts/gamma status --short --branch"));
}

#[test]
fn update_preflights_all_repositories_before_any_fetch() {
    let fixture = Fixture::new();
    for id in ["alpha", "beta", "gamma"] {
        fixture.create_checkout(id);
    }
    fs::write(fixture.checkout("gamma").join(".dirty"), " M changed.txt\n").unwrap();
    let output = fixture.run("update", &["application"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("local changes"));
    assert!(!fixture.log().contains(" fetch "));
    assert!(!fixture.checkout("alpha").join(".merged").exists());
}

#[test]
fn update_fetches_all_then_proves_all_fast_forwardable_before_any_merge() {
    let fixture = Fixture::new();
    for id in ["alpha", "beta", "gamma"] {
        fixture.create_checkout(id);
    }
    let output = fixture
        .command("update")
        .arg("application")
        .env("NIXSPACE_TEST_NON_FF", "checkouts/gamma")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot fast-forward"));
    let log = fixture.log();
    let last_fetch = log.rfind(" fetch ").unwrap();
    let first_check = log.find(" merge-base ").unwrap();
    assert!(last_fetch < first_check);
    assert_eq!(log.matches(" merge-base ").count(), 3);
    assert!(!log.contains(" merge --ff-only "));

    fixture.clear_log();
    let output = fixture.run("update", &["application"]);
    assert_success(&output);
    let log = fixture.log();
    let last_fetch = log.rfind(" fetch ").unwrap();
    let first_check = log.find(" merge-base ").unwrap();
    let last_check = log.rfind(" merge-base ").unwrap();
    let first_merge = log.find(" merge --ff-only ").unwrap();
    assert!(last_fetch < first_check);
    assert!(last_check < first_merge);
    assert_eq!(log.matches(" merge --ff-only ").count(), 3);
    for id in ["alpha", "beta", "gamma"] {
        assert!(fixture.checkout(id).join(".merged").exists());
    }
}

#[test]
fn update_fetch_failure_propagates_without_checks_or_merges() {
    let fixture = Fixture::new();
    for id in ["alpha", "beta", "gamma"] {
        fixture.create_checkout(id);
    }
    let output = fixture
        .command("update")
        .arg("application")
        .env("NIXSPACE_TEST_FETCH_FAIL", "checkouts/beta")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(29));
    let log = fixture.log();
    assert_eq!(log.matches(" fetch ").count(), 3);
    assert!(!log.contains("merge-base"));
    assert!(!log.contains("merge --ff-only"));
}

#[test]
fn exact_package_selection_and_environment_plan_override_are_honored() {
    let fixture = Fixture::new();
    fixture.create_checkout("alpha");
    let mut paths = vec![fixture.bin.clone()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let output = Command::new(env!("CARGO_BIN_EXE_nixspace"))
        .arg("--workspace-root")
        .arg(&fixture.root)
        .args(["status", "alpha"])
        .env("NIXSPACE_SOURCE_PLAN", &fixture.plan)
        .env("PATH", std::env::join_paths(paths).unwrap())
        .env("NIXSPACE_TEST_GIT_LOG", &fixture.log)
        .output()
        .unwrap();
    assert_success(&output);
    assert!(fixture.log().contains("checkouts/alpha"));
    assert!(!fixture.log().contains("checkouts/beta"));
    assert!(!fixture.log().contains("checkouts/gamma"));

    let missing = fixture.run("sync", &["unknown"]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr)
        .contains("package `unknown` has no source selection"));
}

#[test]
fn invalid_plan_version_and_duplicate_selection_have_no_fallback() {
    let fixture = Fixture::new();
    let mut plan = source_plan();
    plan["interfaceVersion"] = json!(2);
    fs::write(&fixture.plan, serde_json::to_vec(&plan).unwrap()).unwrap();
    let version = fixture.run("sync", &[]);
    assert!(!version.status.success());
    assert!(String::from_utf8_lossy(&version.stderr).contains("version 2 is unsupported"));

    plan["interfaceVersion"] = json!(1);
    plan["plans"]["all"] = json!(["alpha", "alpha"]);
    fs::write(&fixture.plan, serde_json::to_vec(&plan).unwrap()).unwrap();
    let duplicate = fixture.run("sync", &[]);
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr)
        .contains("selection `all` repeats repository `alpha`"));
    assert!(!fixture.log.exists());
}

#[test]
fn relative_repository_paths_cannot_traverse_or_use_portable_prefix_tricks() {
    let fixture = Fixture::new();
    for invalid in [
        "../outside",
        "checkouts/../outside",
        ".",
        "C:/outside",
        "checkouts\\outside",
    ] {
        let mut plan = source_plan();
        plan["repositories"]["alpha"]["path"] = json!(invalid);
        fs::write(&fixture.plan, serde_json::to_vec(&plan).unwrap()).unwrap();
        let output = fixture.run("sync", &["alpha"]);
        assert!(
            !output.status.success(),
            "relative repository path `{invalid}` was accepted"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("lexically within workspaceRoot"));
    }

    let absolute = fixture.root.join("absolute-alpha");
    let mut plan = source_plan();
    plan["repositories"]["alpha"]["path"] = json!(absolute);
    fs::write(&fixture.plan, serde_json::to_vec(&plan).unwrap()).unwrap();
    let output = fixture.run("status", &["alpha"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains(&absolute.display().to_string()));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("lexically within workspaceRoot"));
}
