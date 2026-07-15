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
	  "rev-parse --verify HEAD")
	    if [ -e "$path/.concurrent-head" ]; then
	      printf 'fedcba9876543210fedcba9876543210fedcba98\n'
	    elif [ -e "$path/.head-target" ]; then
	      printf '89abcdef0123456789abcdef0123456789abcdef\n'
	    else
      printf '0123456789abcdef0123456789abcdef01234567\n'
    fi
    ;;
  "rev-parse --verify origin/"*)
    printf '89abcdef0123456789abcdef0123456789abcdef\n'
    ;;
  "remote get-url origin")
    cat "$path/.origin"
    ;;
  "symbolic-ref --quiet --short HEAD")
    cat "$path/.branch"
    ;;
  "status --porcelain=v1 --untracked-files=normal")
    if [ -e "$path/.dirty" ]; then
      cat "$path/.dirty"
    elif [ "${NIXSPACE_TEST_DIRTY_AFTER_CLEAN:-}" = "$path" ] && [ -e "$path/.merged" ]; then
      : > "$path/.dirty-after-clean-check"
    fi
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
	    if [ "${NIXSPACE_TEST_MERGE_FAIL:-}" = "$path" ]; then
	      : > "$path/.merged"
	      : > "$path/.head-target"
      if [ -n "${NIXSPACE_TEST_CONCURRENT_DIRTY:-}" ]; then
        printf ' M concurrently-edited.txt\n' > "$NIXSPACE_TEST_CONCURRENT_DIRTY/.dirty"
      fi
      exit 31
	    fi
	    : > "$path/.merged"
	    : > "$path/.head-target"
	    ;;
	  "update-ref HEAD 0123456789abcdef0123456789abcdef01234567 89abcdef0123456789abcdef0123456789abcdef")
	    if [ "${NIXSPACE_TEST_CONCURRENT_REF:-}" = "$path" ]; then
	      : > "$path/.concurrent-head"
	      rm -f "$path/.head-target"
	      exit 33
	    fi
	    [ -e "$path/.head-target" ] || exit 34
	    rm -f "$path/.head-target"
	    ;;
	  "read-tree -m -u 89abcdef0123456789abcdef0123456789abcdef 0123456789abcdef0123456789abcdef01234567")
	    [ ! -e "$path/.dirty-after-clean-check" ] || exit 32
	    [ ! -e "$path/.dirty" ] || exit 32
	    rm -f "$path/.merged"
	    ;;
	  "update-ref HEAD 89abcdef0123456789abcdef0123456789abcdef 0123456789abcdef0123456789abcdef01234567")
	    [ ! -e "$path/.head-target" ] || exit 35
	    [ ! -e "$path/.concurrent-head" ] || exit 36
	    : > "$path/.head-target"
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
            .env_remove("NIXSPACE_TEST_NON_FF")
            .env_remove("NIXSPACE_TEST_MERGE_FAIL")
            .env_remove("NIXSPACE_TEST_CONCURRENT_DIRTY")
            .env_remove("NIXSPACE_TEST_DIRTY_AFTER_CLEAN")
            .env_remove("NIXSPACE_TEST_CONCURRENT_REF");
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

    fn journal(&self) -> PathBuf {
        self.root.join("state/source-update.json")
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
            "interfaceVersion": 3,
        "workspaceRoot": ".",
        "transaction": {
            "mutationLock": "state/source-mutation.lock",
            "journal": "state/source-update.json"
        },
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
            "clean": {"argv": ["git", "-C", path, "status", "--porcelain=v1", "--untracked-files=normal"]},
            "head": {"argv": ["git", "-C", path, "rev-parse", "--verify", "HEAD"]},
            "target": {"argv": ["git", "-C", path, "rev-parse", "--verify", format!("origin/{branch}")]}
        },
        "fetch": {"argv": ["git", "-C", path, "fetch", "--prune", "origin", branch]},
        "fastForwardCheck": {"argv": ["git", "-C", path, "merge-base", "--is-ancestor", "HEAD", format!("origin/{branch}")]},
        "fastForward": {"argv": ["git", "-C", path, "merge", "--ff-only", format!("origin/{branch}")]},
            "rollback": {
                "refUpdate": {"argv": rollback_argv(&path, "update-ref", true)},
                "worktreeRestore": {"argv": rollback_argv(&path, "read-tree", true)},
                "refRestore": {"argv": rollback_argv(&path, "update-ref", false)}
            }
        }
    })
}

fn rollback_argv(path: &str, operation: &str, forward: bool) -> Value {
    let mut argv = vec![
        json!({"kind": "literal", "value": "git"}),
        json!({"kind": "literal", "value": "-C"}),
        json!({"kind": "literal", "value": path}),
    ];
    match operation {
        "update-ref" => {
            argv.extend([
                json!({"kind": "literal", "value": "update-ref"}),
                json!({"kind": "literal", "value": "HEAD"}),
            ]);
            if forward {
                argv.extend([
                    json!({"kind": "old-head"}),
                    json!({"kind": "expected-current"}),
                ]);
            } else {
                argv.extend([
                    json!({"kind": "expected-current"}),
                    json!({"kind": "old-head"}),
                ]);
            }
        }
        "read-tree" => {
            argv.extend([
                json!({"kind": "literal", "value": "read-tree"}),
                json!({"kind": "literal", "value": "-m"}),
                json!({"kind": "literal", "value": "-u"}),
                json!({"kind": "expected-current"}),
                json!({"kind": "old-head"}),
            ]);
        }
        _ => unreachable!(),
    }
    Value::Array(argv)
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
fn update_rolls_back_every_attempted_checkout_when_a_later_merge_fails() {
    let fixture = Fixture::new();
    for id in ["alpha", "beta", "gamma"] {
        fixture.create_checkout(id);
    }
    let output = fixture
        .command("update")
        .arg("application")
        .env("NIXSPACE_TEST_MERGE_FAIL", "checkouts/beta")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(31));
    for id in ["alpha", "beta", "gamma"] {
        assert!(
            !fixture.checkout(id).join(".merged").exists(),
            "repository {id} remained partially advanced"
        );
    }
    let log = fixture.log();
    assert_eq!(log.matches(" update-ref HEAD ").count(), 2);
    assert_eq!(log.matches(" read-tree -m -u ").count(), 2);
    assert!(log.contains("checkouts/beta update-ref HEAD"));
    assert!(log.contains("checkouts/alpha update-ref HEAD"));
    assert!(!log.contains("git -C checkouts/gamma merge --ff-only"));
    assert!(!fixture.journal().exists());
}

#[test]
fn rollback_refuses_to_destroy_a_concurrent_tracked_edit_and_retains_journal() {
    let fixture = Fixture::new();
    for id in ["alpha", "beta", "gamma"] {
        fixture.create_checkout(id);
    }
    let output = fixture
        .command("update")
        .arg("application")
        .env("NIXSPACE_TEST_MERGE_FAIL", "checkouts/beta")
        .env("NIXSPACE_TEST_CONCURRENT_DIRTY", "checkouts/alpha")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("refusing the conditional rollback"),
        "{stderr}"
    );
    assert!(stderr.contains("manual recovery"), "{stderr}");
    assert!(fixture.checkout("alpha").join(".merged").exists());
    assert!(fixture.checkout("alpha").join(".dirty").exists());
    assert!(!fixture.checkout("beta").join(".merged").exists());
    assert!(fixture.journal().is_file());
    let log = fixture.log();
    assert!(log.contains("checkouts/beta update-ref HEAD"));
    assert!(!log.contains("checkouts/alpha update-ref HEAD"));
}

#[test]
fn nix_declared_conditional_rollback_preserves_an_edit_after_the_clean_check() {
    let fixture = Fixture::new();
    for id in ["alpha", "beta", "gamma"] {
        fixture.create_checkout(id);
    }
    let output = fixture
        .command("update")
        .arg("application")
        .env("NIXSPACE_TEST_MERGE_FAIL", "checkouts/beta")
        .env("NIXSPACE_TEST_DIRTY_AFTER_CLEAN", "checkouts/alpha")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(fixture.checkout("alpha").join(".merged").exists());
    assert!(fixture
        .checkout("alpha")
        .join(".dirty-after-clean-check")
        .exists());
    assert!(fixture.journal().is_file());
    let log = fixture.log();
    assert!(log.contains("checkouts/alpha read-tree -m -u"));
    assert!(log.contains(
        "checkouts/alpha update-ref HEAD 89abcdef0123456789abcdef0123456789abcdef 0123456789abcdef0123456789abcdef01234567"
    ));
}

#[test]
fn rollback_cas_refuses_a_concurrent_clean_commit_and_retains_the_journal() {
    let fixture = Fixture::new();
    for id in ["alpha", "beta", "gamma"] {
        fixture.create_checkout(id);
    }
    let output = fixture
        .command("update")
        .arg("application")
        .env("NIXSPACE_TEST_MERGE_FAIL", "checkouts/beta")
        .env("NIXSPACE_TEST_CONCURRENT_REF", "checkouts/alpha")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("moved away from expected transaction head"),
        "{stderr}"
    );
    assert!(stderr.contains("concurrent ref update"), "{stderr}");
    assert!(fixture.checkout("alpha").join(".concurrent-head").is_file());
    assert!(fixture.checkout("alpha").join(".merged").is_file());
    assert!(fixture.journal().is_file());
    assert!(!fixture.log().contains("checkouts/alpha read-tree -m -u"));
}

#[test]
fn a_durable_journal_recovers_an_interrupted_partial_update_before_sync() {
    let fixture = Fixture::new();
    for id in ["alpha", "beta", "gamma"] {
        fixture.create_checkout(id);
    }
    fs::write(fixture.checkout("alpha").join(".merged"), "").unwrap();
    fs::write(fixture.checkout("alpha").join(".head-target"), "").unwrap();
    fs::create_dir_all(fixture.journal().parent().unwrap()).unwrap();
    fs::write(
        fixture.journal(),
        serde_json::to_vec(&json!({
            "interfaceVersion": 1,
            "repositories": [
                {
                    "id": "alpha",
                    "oldHead": "0123456789abcdef0123456789abcdef01234567",
                    "targetHead": "89abcdef0123456789abcdef0123456789abcdef"
                },
                {
                    "id": "beta",
                    "oldHead": "0123456789abcdef0123456789abcdef01234567",
                    "targetHead": "89abcdef0123456789abcdef0123456789abcdef"
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    let output = fixture.run("sync", &[]);
    assert_success(&output);
    assert!(!fixture.checkout("alpha").join(".merged").exists());
    assert!(!fixture.checkout("beta").join(".merged").exists());
    assert!(!fixture.journal().exists());
    let log = fixture.log();
    let recovery = log.find(" update-ref HEAD ").unwrap();
    let sync_inspection = log.rfind(" remote get-url origin").unwrap();
    assert!(recovery < sync_inspection);
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

    plan["interfaceVersion"] = json!(3);
    plan["plans"]["all"] = json!(["alpha", "alpha"]);
    fs::write(&fixture.plan, serde_json::to_vec(&plan).unwrap()).unwrap();
    let duplicate = fixture.run("sync", &[]);
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr)
        .contains("selection `all` repeats repository `alpha`"));
    assert!(!fixture.log.exists());
}

#[test]
fn source_plan_requires_typed_rollback_old_head_and_expected_current_parameters() {
    let fixture = Fixture::new();
    let mut plan = source_plan();
    plan["repositories"]["alpha"]["git"]["rollback"]["refUpdate"]["argv"][5] =
        json!({"kind": "literal", "value": "not-an-old-head"});
    fs::write(&fixture.plan, serde_json::to_vec(&plan).unwrap()).unwrap();
    let output = fixture.run("sync", &["alpha"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("exactly one typed old-head and expected-current parameter"));
    assert!(!fixture.log.exists());
}

#[test]
fn transaction_paths_are_distinct_portable_workspace_files() {
    let fixture = Fixture::new();
    for invalid in [
        ".",
        "../source.lock",
        "C:/source.lock",
        "state\\source.lock",
    ] {
        let mut plan = source_plan();
        plan["transaction"]["mutationLock"] = json!(invalid);
        fs::write(&fixture.plan, serde_json::to_vec(&plan).unwrap()).unwrap();
        let output = fixture.run("sync", &["alpha"]);
        assert!(
            !output.status.success(),
            "transaction path `{invalid}` was accepted"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("workspace-relative file path"));
    }

    let mut plan = source_plan();
    plan["transaction"]["mutationLock"] = plan["transaction"]["journal"].clone();
    fs::write(&fixture.plan, serde_json::to_vec(&plan).unwrap()).unwrap();
    let output = fixture.run("sync", &["alpha"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("must be distinct"));
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
