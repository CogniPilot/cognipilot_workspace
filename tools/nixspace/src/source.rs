use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::action::Outcome;
use crate::{CliError, Result};

const SUPPORTED_API_VERSION: &str = "nixspace/v1";
const SUPPORTED_KIND: &str = "SourceWorkspace";
const SUPPORTED_INTERFACE_VERSION: u64 = 1;

#[derive(Debug, Args)]
pub(crate) struct SelectionArgs {
    /// Select the public default, one exact package closure, or all repositories.
    #[arg(value_name = "default|PACKAGE|all", default_value = "default")]
    selector: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourcePlan {
    api_version: String,
    kind: String,
    interface_version: u64,
    workspace_root: PathBuf,
    repositories: BTreeMap<String, Repository>,
    plans: Selections,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selections {
    default: Vec<String>,
    all: Vec<String>,
    packages: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Repository {
    id: String,
    packages: Vec<String>,
    path: PathBuf,
    source: Value,
    git: GitPlan,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GitPlan {
    url: String,
    branch: String,
    clone: GitCommand,
    status: GitCommand,
    inspect: GitInspect,
    fetch: GitCommand,
    fast_forward_check: GitCommand,
    fast_forward: GitCommand,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GitInspect {
    worktree: GitCommand,
    origin: GitCommand,
    branch: GitCommand,
    clean: GitCommand,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GitCommand {
    argv: Vec<String>,
}

struct Loaded {
    plan: SourcePlan,
    workspace: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceReadiness {
    pub(crate) selection: String,
    pub(crate) workspace: PathBuf,
    pub(crate) satisfied: bool,
    pub(crate) repositories: Vec<RepositoryReadiness>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RepositoryReadiness {
    id: String,
    path: PathBuf,
    present: bool,
    conflicts: Vec<String>,
    error: Option<String>,
    pub(crate) satisfied: bool,
}

pub(crate) fn readiness(
    root: &Path,
    explicit_plan: Option<PathBuf>,
    selector: &str,
) -> Result<SourceReadiness> {
    let loaded = load(root, explicit_plan)?;
    let selected = select(&loaded.plan, selector)?;
    let mut repositories = Vec::with_capacity(selected.len());
    for repository in selected {
        let path = repository_path(&loaded.workspace, repository);
        let present = path.is_dir();
        let (conflicts, error) = if present {
            match run_captured(
                &repository.git.inspect.clean,
                &loaded.workspace,
                "conflict status",
            ) {
                Ok(output) if output.status.success() => match conflict_paths(&output.stdout) {
                    Ok(conflicts) => (conflicts, None),
                    Err(error) => (Vec::new(), Some(error.0)),
                },
                Ok(output) => (
                    Vec::new(),
                    Some(format!(
                        "repository `{}` conflict status failed: {}",
                        repository.id,
                        failure_detail(&output)
                    )),
                ),
                Err(error) => (Vec::new(), Some(error.0)),
            }
        } else {
            (Vec::new(), None)
        };
        let satisfied = present && conflicts.is_empty() && error.is_none();
        repositories.push(RepositoryReadiness {
            id: repository.id.clone(),
            path,
            present,
            conflicts,
            error,
            satisfied,
        });
    }
    Ok(SourceReadiness {
        selection: selector.to_owned(),
        workspace: loaded.workspace,
        satisfied: repositories.iter().all(|repository| repository.satisfied),
        repositories,
    })
}

fn conflict_paths(output: &[u8]) -> Result<Vec<String>> {
    let status = std::str::from_utf8(output).map_err(|error| {
        CliError(format!(
            "Git conflict status returned non-UTF-8 output: {error}"
        ))
    })?;
    let mut conflicts = Vec::new();
    for line in status.lines() {
        let bytes = line.as_bytes();
        if bytes.len() < 3 || !bytes[0].is_ascii() || !bytes[1].is_ascii() || bytes[2] != b' ' {
            return Err(CliError(format!(
                "Git conflict status returned malformed porcelain line `{line}`"
            )));
        }
        let code = std::str::from_utf8(&bytes[..2]).expect("validated ASCII status code");
        if matches!(code, "DD" | "AU" | "UD" | "UA" | "DU" | "AA" | "UU") {
            conflicts.push(
                std::str::from_utf8(&bytes[3..])
                    .expect("suffix of validated UTF-8 status")
                    .to_owned(),
            );
        }
    }
    Ok(conflicts)
}

pub(crate) fn sync(
    root: &Path,
    explicit_plan: Option<PathBuf>,
    arguments: SelectionArgs,
) -> Result<Outcome> {
    let loaded = load(root, explicit_plan)?;
    let repositories = select(&loaded.plan, &arguments.selector)?;
    let mut missing = Vec::new();
    let mut preflight_errors = Vec::new();

    // Complete every predictable path/identity check before the first clone,
    // so one bad existing checkout cannot leave a partially synced selection.
    for repository in &repositories {
        let path = repository_path(&loaded.workspace, repository);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() => {
                if let Err(error) = inspect_identity(&loaded.workspace, repository, &path, false) {
                    preflight_errors.push(error.0);
                }
            }
            Ok(_) => {
                preflight_errors.push(format!(
                    "repository `{}` checkout path exists but is not a directory: {}",
                    repository.id,
                    path.display()
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                match preflight_missing_parent(&path, &repository.id) {
                    Ok(()) => missing.push(*repository),
                    Err(error) => preflight_errors.push(error.0),
                }
            }
            Err(error) => {
                preflight_errors.push(format!(
                    "cannot inspect checkout path for repository `{}` at {}: {error}",
                    repository.id,
                    path.display()
                ));
            }
        }
    }
    if !preflight_errors.is_empty() {
        return Err(CliError(preflight_errors.join("\n")));
    }

    for repository in &missing {
        let path = repository_path(&loaded.workspace, repository);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                CliError(format!(
                    "cannot create checkout parent for repository `{}` at {}: {error}",
                    repository.id,
                    parent.display()
                ))
            })?;
        }
    }
    for repository in missing {
        let status = run_streaming(&repository.git.clone, &loaded.workspace, "clone")?;
        if !status.success() {
            return Ok(Outcome::Exit(status.code().unwrap_or(1)));
        }
    }
    Ok(Outcome::Success)
}

pub(crate) fn status(
    root: &Path,
    explicit_plan: Option<PathBuf>,
    arguments: SelectionArgs,
) -> Result<Outcome> {
    let loaded = load(root, explicit_plan)?;
    let repositories = select(&loaded.plan, &arguments.selector)?;
    let mut failed = false;
    for repository in repositories {
        let path = repository_path(&loaded.workspace, repository);
        if !path.is_dir() {
            println!("{}\tmissing\t{}", repository.id, path.display());
            failed = true;
            continue;
        }
        println!("{}\t{}", repository.id, path.display());
        let status = run_streaming(&repository.git.status, &loaded.workspace, "status")?;
        if !status.success() {
            failed = true;
        }
    }
    if failed {
        Ok(Outcome::Exit(1))
    } else {
        Ok(Outcome::Success)
    }
}

pub(crate) fn update(
    root: &Path,
    explicit_plan: Option<PathBuf>,
    arguments: SelectionArgs,
) -> Result<Outcome> {
    let loaded = load(root, explicit_plan)?;
    let repositories = select(&loaded.plan, &arguments.selector)?;

    // Phase 1: nothing networked or mutable happens until every repository is
    // proven to be the exact clean worktree declared by Nix.
    let mut preflight_errors = Vec::new();
    for repository in &repositories {
        let path = repository_path(&loaded.workspace, repository);
        if let Err(error) = inspect_identity(&loaded.workspace, repository, &path, true) {
            preflight_errors.push(error.0);
        }
    }
    if !preflight_errors.is_empty() {
        return Err(CliError(preflight_errors.join("\n")));
    }

    // Phase 2: fetching all repositories may change remote-tracking refs, but
    // never a checkout. A failure therefore cannot partially merge sources.
    let mut fetch_failure = None;
    for repository in &repositories {
        let status = run_streaming(&repository.git.fetch, &loaded.workspace, "fetch")?;
        if !status.success() && fetch_failure.is_none() {
            fetch_failure = Some(status.code().unwrap_or(1));
        }
    }
    if let Some(code) = fetch_failure {
        return Ok(Outcome::Exit(code));
    }

    // Phase 3: prove the complete selection before the first worktree merge.
    let mut non_fast_forward = Vec::new();
    for repository in &repositories {
        let output = run_captured(
            &repository.git.fast_forward_check,
            &loaded.workspace,
            "fast-forward check",
        )?;
        if !output.status.success() {
            non_fast_forward.push(format!(
                "repository `{}` cannot fast-forward `{}` to its fetched origin: {}",
                repository.id,
                repository.git.branch,
                failure_detail(&output)
            ));
        }
    }
    if !non_fast_forward.is_empty() {
        return Err(CliError(non_fast_forward.join("\n")));
    }

    // Phase 4: execute the exact Nix-emitted ff-only commands.
    for repository in repositories {
        let status = run_streaming(
            &repository.git.fast_forward,
            &loaded.workspace,
            "fast-forward",
        )?;
        if !status.success() {
            return Ok(Outcome::Exit(status.code().unwrap_or(1)));
        }
    }
    Ok(Outcome::Success)
}

fn load(root: &Path, explicit_plan: Option<PathBuf>) -> Result<Loaded> {
    let path = explicit_plan
        .or_else(|| env::var_os("NIXSPACE_SOURCE_PLAN").map(PathBuf::from))
        .map(|path| resolve(root, path))
        .ok_or_else(|| {
            CliError(
                "source workspace requires an explicit Nix-generated plan; pass --source-plan or set NIXSPACE_SOURCE_PLAN"
                    .into(),
            )
        })?;
    let bytes = fs::read(&path).map_err(|error| {
        CliError(format!(
            "source workspace plan is unavailable at {}: {error}; set --source-plan or NIXSPACE_SOURCE_PLAN",
            path.display()
        ))
    })?;
    let plan: SourcePlan = serde_json::from_slice(&bytes).map_err(|error| {
        CliError(format!(
            "Nix-generated source workspace plan at {} is unreadable: {error}",
            path.display()
        ))
    })?;
    validate(&plan)?;
    let workspace = resolve(root, plan.workspace_root.clone());
    validate_repository_paths(&plan, &workspace)?;
    Ok(Loaded { plan, workspace })
}

fn validate(plan: &SourcePlan) -> Result<()> {
    if plan.api_version != SUPPORTED_API_VERSION {
        return Err(CliError(format!(
            "source workspace API `{}` is unsupported; this nixspace requires `{SUPPORTED_API_VERSION}`",
            plan.api_version
        )));
    }
    if plan.kind != SUPPORTED_KIND {
        return Err(CliError(format!(
            "source workspace kind `{}` is unsupported; this nixspace requires `{SUPPORTED_KIND}`",
            plan.kind
        )));
    }
    if plan.interface_version != SUPPORTED_INTERFACE_VERSION {
        return Err(CliError(format!(
            "source workspace interface version {} is unsupported; this nixspace supports version {}",
            plan.interface_version, SUPPORTED_INTERFACE_VERSION
        )));
    }
    if plan.workspace_root.as_os_str().is_empty() {
        return Err(CliError(
            "source workspace workspaceRoot must not be empty".into(),
        ));
    }
    for (key, repository) in &plan.repositories {
        if key != &repository.id
            || repository.id.is_empty()
            || repository.path.as_os_str().is_empty()
        {
            return Err(CliError(format!(
                "source repository key `{key}` must equal its nonempty id and declare a nonempty path"
            )));
        }
        if repository.git.url.is_empty() || repository.git.branch.is_empty() {
            return Err(CliError(format!(
                "source repository `{key}` must declare a nonempty Git URL and branch"
            )));
        }
        if repository.packages.iter().any(|package| package.is_empty()) {
            return Err(CliError(format!(
                "source repository `{key}` declares an empty package identity"
            )));
        }
        let _ = &repository.source;
        for (name, command) in commands(repository) {
            if command.argv.is_empty()
                || command.argv[0].is_empty()
                || command.argv.iter().any(|argument| argument.contains('\0'))
            {
                return Err(CliError(format!(
                    "source repository `{key}` Git {name} argv must begin with a nonempty executable and contain no NUL"
                )));
            }
        }
    }
    validate_selection("default", &plan.plans.default, &plan.repositories)?;
    validate_selection("all", &plan.plans.all, &plan.repositories)?;
    for (package, selection) in &plan.plans.packages {
        if package.is_empty() {
            return Err(CliError(
                "source workspace package selection name must not be empty".into(),
            ));
        }
        validate_selection(package, selection, &plan.repositories)?;
    }
    Ok(())
}

fn validate_repository_paths(plan: &SourcePlan, workspace: &Path) -> Result<()> {
    for repository in plan.repositories.values() {
        if repository.path.is_absolute() {
            continue;
        }
        let rendered = repository.path.to_string_lossy();
        let safe = !rendered.contains('\\')
            && !rendered
                .split('/')
                .next()
                .is_some_and(|segment| segment.ends_with(':'))
            && repository
                .path
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)));
        let resolved = workspace.join(&repository.path);
        if !safe || !resolved.starts_with(workspace) {
            return Err(CliError(format!(
                "source repository `{}` relative path must remain lexically within workspaceRoot without `.`, `..`, alternate separators, or platform prefixes",
                repository.id
            )));
        }
    }
    Ok(())
}

fn commands(repository: &Repository) -> [(&'static str, &GitCommand); 9] {
    [
        ("clone", &repository.git.clone),
        ("status", &repository.git.status),
        ("worktree inspect", &repository.git.inspect.worktree),
        ("origin inspect", &repository.git.inspect.origin),
        ("branch inspect", &repository.git.inspect.branch),
        ("clean inspect", &repository.git.inspect.clean),
        ("fetch", &repository.git.fetch),
        ("fast-forward check", &repository.git.fast_forward_check),
        ("fast-forward", &repository.git.fast_forward),
    ]
}

fn validate_selection(
    name: &str,
    selection: &[String],
    repositories: &BTreeMap<String, Repository>,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for repository in selection {
        if !repositories.contains_key(repository) {
            return Err(CliError(format!(
                "source selection `{name}` references missing repository `{repository}`"
            )));
        }
        if !seen.insert(repository) {
            return Err(CliError(format!(
                "source selection `{name}` repeats repository `{repository}`"
            )));
        }
    }
    Ok(())
}

fn select<'a>(plan: &'a SourcePlan, selector: &str) -> Result<Vec<&'a Repository>> {
    let ids = match selector {
        "default" => &plan.plans.default,
        "all" => &plan.plans.all,
        _ => plan.plans.packages.get(selector).ok_or_else(|| {
            CliError(format!(
                "package `{selector}` has no source selection in the Nix-generated plan"
            ))
        })?,
    };
    Ok(ids
        .iter()
        .map(|id| {
            plan.repositories
                .get(id)
                .expect("validated source selection")
        })
        .collect())
}

fn repository_path(workspace: &Path, repository: &Repository) -> PathBuf {
    resolve(workspace, repository.path.clone())
}

fn resolve(base: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

fn preflight_missing_parent(path: &Path, repository: &str) -> Result<()> {
    let mut parent = path.parent();
    while let Some(candidate) = parent {
        if candidate.exists() {
            if !candidate.is_dir() {
                return Err(CliError(format!(
                    "repository `{repository}` checkout parent is not a directory: {}",
                    candidate.display()
                )));
            }
            return Ok(());
        }
        parent = candidate.parent();
    }
    Err(CliError(format!(
        "repository `{repository}` checkout path has no existing directory ancestor: {}",
        path.display()
    )))
}

fn inspect_identity(
    workspace: &Path,
    repository: &Repository,
    expected_path: &Path,
    require_clean: bool,
) -> Result<()> {
    if !expected_path.is_dir() {
        return Err(CliError(format!(
            "repository `{}` is missing at {}",
            repository.id,
            expected_path.display()
        )));
    }
    let mut errors = Vec::new();
    let worktree_check = (|| -> Result<()> {
        let worktree = successful_output(
            &repository.git.inspect.worktree,
            workspace,
            &repository.id,
            "worktree",
        )?;
        let reported = output_line(&worktree, &repository.id, "worktree")?;
        let expected = fs::canonicalize(expected_path).map_err(|error| {
            CliError(format!(
                "cannot canonicalize repository `{}` checkout {}: {error}",
                repository.id,
                expected_path.display()
            ))
        })?;
        let reported =
            fs::canonicalize(resolve(workspace, PathBuf::from(reported))).map_err(|error| {
                CliError(format!(
                    "repository `{}` reported an invalid worktree path: {error}",
                    repository.id
                ))
            })?;
        if reported != expected {
            return Err(CliError(format!(
                "repository `{}` worktree mismatch: expected {}, got {}",
                repository.id,
                expected.display(),
                reported.display()
            )));
        }
        Ok(())
    })();
    if let Err(error) = worktree_check {
        errors.push(error.0);
    }

    let origin_check = (|| -> Result<()> {
        let origin = successful_output(
            &repository.git.inspect.origin,
            workspace,
            &repository.id,
            "origin",
        )?;
        let origin = output_line(&origin, &repository.id, "origin")?;
        if origin != repository.git.url {
            return Err(CliError(format!(
                "repository `{}` origin mismatch: expected `{}`, got `{origin}`",
                repository.id, repository.git.url
            )));
        }
        Ok(())
    })();
    if let Err(error) = origin_check {
        errors.push(error.0);
    }

    let branch_check = (|| -> Result<()> {
        let branch = successful_output(
            &repository.git.inspect.branch,
            workspace,
            &repository.id,
            "branch",
        )?;
        let branch = output_line(&branch, &repository.id, "branch")?;
        if branch != repository.git.branch {
            return Err(CliError(format!(
                "repository `{}` branch mismatch: expected `{}`, got `{branch}`",
                repository.id, repository.git.branch
            )));
        }
        Ok(())
    })();
    if let Err(error) = branch_check {
        errors.push(error.0);
    }

    if require_clean {
        let clean_check = successful_output(
            &repository.git.inspect.clean,
            workspace,
            &repository.id,
            "clean-tree",
        )
        .and_then(|clean| {
            if clean.stdout.is_empty() {
                Ok(())
            } else {
                Err(CliError(format!(
                    "repository `{}` has local changes; update requires a clean tree",
                    repository.id
                )))
            }
        });
        if let Err(error) = clean_check {
            errors.push(error.0);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(CliError(errors.join("\n")))
    }
}

fn successful_output(
    command: &GitCommand,
    workspace: &Path,
    repository: &str,
    inspection: &str,
) -> Result<Output> {
    let output = run_captured(command, workspace, inspection)?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(CliError(format!(
            "repository `{repository}` failed {inspection} inspection: {}",
            failure_detail(&output)
        )))
    }
}

fn output_line<'a>(output: &'a Output, repository: &str, field: &str) -> Result<&'a str> {
    let value = std::str::from_utf8(&output.stdout).map_err(|error| {
        CliError(format!(
            "repository `{repository}` {field} inspection returned non-UTF-8 output: {error}"
        ))
    })?;
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() || value.contains('\n') || value.contains('\r') {
        return Err(CliError(format!(
            "repository `{repository}` {field} inspection must return exactly one nonempty line"
        )));
    }
    Ok(value)
}

fn run_captured(command: &GitCommand, cwd: &Path, purpose: &str) -> Result<Output> {
    let executable = &command.argv[0];
    Command::new(executable)
        .args(&command.argv[1..])
        .current_dir(cwd)
        .output()
        .map_err(|error| command_start_error(executable, purpose, error))
}

fn run_streaming(command: &GitCommand, cwd: &Path, purpose: &str) -> Result<ExitStatus> {
    let executable = &command.argv[0];
    Command::new(executable)
        .args(&command.argv[1..])
        .current_dir(cwd)
        .status()
        .map_err(|error| command_start_error(executable, purpose, error))
}

fn command_start_error(executable: &str, purpose: &str, error: io::Error) -> CliError {
    if error.kind() == io::ErrorKind::NotFound {
        CliError(format!(
            "source {purpose} executable `{executable}` is unavailable on PATH"
        ))
    } else {
        CliError(format!(
            "cannot start source {purpose} executable `{executable}`: {error}"
        ))
    }
}

fn failure_detail(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.trim().is_empty() {
        format!("status {}", output.status)
    } else {
        stderr.trim().to_owned()
    }
}
