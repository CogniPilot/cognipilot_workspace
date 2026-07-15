use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};

use clap::Args;
use fs4::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::action::Outcome;
use crate::{CliError, Result};

const SUPPORTED_API_VERSION: &str = "nixspace/v1";
const SUPPORTED_KIND: &str = "SourceWorkspace";
const SUPPORTED_INTERFACE_VERSION: u64 = 3;

#[derive(Debug, Args)]
pub(crate) struct SelectionArgs {
    /// Select the Nix-declared default, one exact package closure, or all repositories.
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
    transaction: TransactionPaths,
    repositories: BTreeMap<String, Repository>,
    plans: Selections,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TransactionPaths {
    mutation_lock: PathBuf,
    journal: PathBuf,
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
    rollback: RollbackCommand,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GitInspect {
    worktree: GitCommand,
    origin: GitCommand,
    branch: GitCommand,
    clean: GitCommand,
    head: GitCommand,
    target: GitCommand,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GitCommand {
    argv: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RollbackCommand {
    ref_update: RollbackCommandTemplate,
    worktree_restore: RollbackCommandTemplate,
    ref_restore: RollbackCommandTemplate,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RollbackCommandTemplate {
    argv: Vec<RollbackArgument>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum RollbackArgument {
    Literal { value: String },
    OldHead,
    ExpectedCurrent,
}

struct Loaded {
    plan: SourcePlan,
    workspace: PathBuf,
}

impl Loaded {
    fn mutation_lock(&self) -> PathBuf {
        self.workspace.join(&self.plan.transaction.mutation_lock)
    }

    fn transaction_journal(&self) -> PathBuf {
        self.workspace.join(&self.plan.transaction.journal)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateJournal {
    interface_version: u64,
    repositories: Vec<UpdateJournalRepository>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateJournalRepository {
    id: String,
    old_head: String,
    target_head: String,
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
    let _lock = SourceMutationLock::acquire(&loaded.mutation_lock())?;
    recover_interrupted_update(&loaded)?;
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
    let _lock = SourceMutationLock::acquire(&loaded.mutation_lock())?;
    recover_interrupted_update(&loaded)?;

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

    // Fetch and fast-forward checks can run arbitrary hooks and take time.
    // Revalidate every checkout while still holding the workspace mutation
    // lock immediately before capturing the rollback boundary.
    let mut revalidation_errors = Vec::new();
    for repository in &repositories {
        let path = repository_path(&loaded.workspace, repository);
        if let Err(error) = inspect_identity(&loaded.workspace, repository, &path, true) {
            revalidation_errors.push(error.0);
        }
    }
    if !revalidation_errors.is_empty() {
        return Err(CliError(format!(
            "source selection changed during update preflight; no checkout was advanced:\n{}",
            revalidation_errors.join("\n")
        )));
    }

    let heads = repositories
        .iter()
        .map(|repository| capture_head(&loaded.workspace, repository))
        .collect::<Result<Vec<_>>>()?;
    let targets = repositories
        .iter()
        .map(|repository| {
            capture_command(
                &loaded.workspace,
                repository,
                &repository.git.inspect.target,
                "fetched target head",
            )
        })
        .collect::<Result<Vec<_>>>()?;
    write_update_journal(
        &loaded.transaction_journal(),
        &repositories,
        &heads,
        &targets,
    )?;

    // Phase 4: execute the exact Nix-emitted ff-only commands. If any command
    // fails, restore every checkout that may have been touched to its captured
    // clean head before returning.
    for (index, repository) in repositories.iter().enumerate() {
        let result = run_streaming(
            &repository.git.fast_forward,
            &loaded.workspace,
            "fast-forward",
        );
        match result {
            Ok(status) if status.success() => {}
            Ok(status) => {
                let rollback_errors = rollback_heads(
                    &loaded.workspace,
                    &repositories[..=index],
                    &heads[..=index],
                    &targets[..=index],
                );
                if rollback_errors.is_empty() {
                    remove_update_journal(&loaded.transaction_journal())?;
                    return Ok(Outcome::Exit(status.code().unwrap_or(1)));
                }
                return Err(CliError(format!(
                    "source fast-forward failed with {status}; rollback was incomplete:\n{}",
                    rollback_errors.join("\n")
                )));
            }
            Err(error) => {
                let rollback_errors = rollback_heads(
                    &loaded.workspace,
                    &repositories[..=index],
                    &heads[..=index],
                    &targets[..=index],
                );
                if rollback_errors.is_empty() {
                    remove_update_journal(&loaded.transaction_journal())?;
                    return Err(CliError(format!(
                        "{}; every attempted checkout was restored",
                        error.0
                    )));
                }
                return Err(CliError(format!(
                    "{}; rollback was incomplete:\n{}",
                    error.0,
                    rollback_errors.join("\n")
                )));
            }
        }
    }

    let mut postflight_errors = Vec::new();
    for repository in &repositories {
        let path = repository_path(&loaded.workspace, repository);
        if let Err(error) = inspect_identity(&loaded.workspace, repository, &path, true) {
            postflight_errors.push(error.0);
        }
    }
    for ((repository, expected), actual) in repositories.iter().zip(&targets).zip(
        repositories
            .iter()
            .map(|repository| capture_head(&loaded.workspace, repository)),
    ) {
        match actual {
            Ok(actual) if &actual == expected => {}
            Ok(actual) => postflight_errors.push(format!(
                "repository `{}` ended at `{actual}` instead of fetched target `{expected}`",
                repository.id
            )),
            Err(error) => postflight_errors.push(error.0),
        }
    }
    if !postflight_errors.is_empty() {
        let rollback_errors = rollback_heads(&loaded.workspace, &repositories, &heads, &targets);
        let mut diagnostic = format!(
            "source update postflight failed; all checkouts were rolled back:\n{}",
            postflight_errors.join("\n")
        );
        if !rollback_errors.is_empty() {
            diagnostic.push_str("\nrollback was incomplete:\n");
            diagnostic.push_str(&rollback_errors.join("\n"));
        } else if let Err(error) = remove_update_journal(&loaded.transaction_journal()) {
            diagnostic.push_str("\nrollback completed but its journal could not be removed:\n");
            diagnostic.push_str(&error.0);
        }
        return Err(CliError(diagnostic));
    }
    remove_update_journal(&loaded.transaction_journal())?;
    Ok(Outcome::Success)
}

struct SourceMutationLock(File);

impl SourceMutationLock {
    fn acquire(path: &Path) -> Result<Self> {
        let parent = path.parent().expect("validated lock path has a parent");
        fs::create_dir_all(parent).map_err(|error| {
            CliError(format!(
                "cannot create source workspace lock directory {}: {error}",
                parent.display()
            ))
        })?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|error| {
                CliError(format!(
                    "cannot open source workspace lock {}: {error}",
                    path.display()
                ))
            })?;
        FileExt::lock(&file).map_err(|error| {
            CliError(format!(
                "cannot acquire source workspace lock {}: {error}",
                path.display()
            ))
        })?;
        Ok(Self(file))
    }
}

impl Drop for SourceMutationLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

fn capture_head(workspace: &Path, repository: &Repository) -> Result<String> {
    capture_command(
        workspace,
        repository,
        &repository.git.inspect.head,
        "rollback head",
    )
}

fn capture_command(
    workspace: &Path,
    repository: &Repository,
    command: &GitCommand,
    description: &str,
) -> Result<String> {
    let output = run_captured(command, workspace, "rollback-boundary inspection")?;
    if !output.status.success() {
        return Err(CliError(format!(
            "repository `{}` cannot capture its {description}: {}",
            repository.id,
            failure_detail(&output)
        )));
    }
    let head = output_line(&output, &repository.id, description)?;
    if !(40..=64).contains(&head.len()) || !head.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CliError(format!(
            "repository `{}` returned an invalid {description} `{head}`",
            repository.id
        )));
    }
    Ok(head.to_owned())
}

fn write_update_journal(
    path: &Path,
    repositories: &[&Repository],
    heads: &[String],
    targets: &[String],
) -> Result<()> {
    let journal = UpdateJournal {
        interface_version: 1,
        repositories: repositories
            .iter()
            .zip(heads)
            .zip(targets)
            .map(
                |((repository, old_head), target_head)| UpdateJournalRepository {
                    id: repository.id.clone(),
                    old_head: old_head.clone(),
                    target_head: target_head.clone(),
                },
            )
            .collect(),
    };
    let mut contents = serde_json::to_vec_pretty(&journal)
        .map_err(|error| CliError(format!("cannot encode source update journal: {error}")))?;
    contents.push(b'\n');
    let parent = path.parent().expect("validated journal path has a parent");
    fs::create_dir_all(parent).map_err(|error| {
        CliError(format!(
            "cannot create source transaction journal directory {}: {error}",
            parent.display()
        ))
    })?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(|error| {
        CliError(format!(
            "cannot create source update journal staging file {}: {error}",
            temporary.display()
        ))
    })?;
    use std::io::Write;
    file.write_all(&contents)
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            let _ = fs::remove_file(&temporary);
            CliError(format!("cannot persist source update journal: {error}"))
        })?;
    drop(file);
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        CliError(format!(
            "cannot publish source update journal {}: {error}",
            path.display()
        ))
    })?;
    sync_transaction_directory(parent)
}

fn remove_update_journal(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => sync_transaction_directory(path.parent().expect("journal path has parent")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CliError(format!(
            "cannot remove completed source update journal {}: {error}",
            path.display()
        ))),
    }
}

#[cfg(unix)]
fn sync_transaction_directory(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            CliError(format!(
                "cannot durably synchronize source transaction directory {}: {error}",
                path.display()
            ))
        })
}

#[cfg(not(unix))]
fn sync_transaction_directory(_path: &Path) -> Result<()> {
    Ok(())
}

fn recover_interrupted_update(loaded: &Loaded) -> Result<()> {
    let path = loaded.transaction_journal();
    let contents = match fs::read(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(CliError(format!(
                "cannot read interrupted source update journal {}: {error}",
                path.display()
            )))
        }
    };
    let journal: UpdateJournal = serde_json::from_slice(&contents).map_err(|error| {
        CliError(format!(
            "interrupted source update journal {} is unreadable: {error}",
            path.display()
        ))
    })?;
    if journal.interface_version != 1 || journal.repositories.is_empty() {
        return Err(CliError(
            "interrupted source update journal has an unsupported interface or no repositories"
                .into(),
        ));
    }
    let mut seen = BTreeSet::new();
    let mut repositories = Vec::with_capacity(journal.repositories.len());
    let mut observed = Vec::with_capacity(journal.repositories.len());
    for entry in &journal.repositories {
        if !seen.insert(entry.id.as_str())
            || !valid_object_id(&entry.old_head)
            || !valid_object_id(&entry.target_head)
        {
            return Err(CliError(
                "interrupted source update journal contains duplicate repositories or invalid object IDs"
                    .into(),
            ));
        }
        let repository = loaded.plan.repositories.get(&entry.id).ok_or_else(|| {
            CliError(format!(
                "interrupted source update references repository `{}` missing from the current Nix plan",
                entry.id
            ))
        })?;
        let repository_path = repository_path(&loaded.workspace, repository);
        inspect_identity(&loaded.workspace, repository, &repository_path, true)?;
        observed.push(capture_head(&loaded.workspace, repository)?);
        repositories.push(repository);
    }
    let all_old = observed
        .iter()
        .zip(&journal.repositories)
        .all(|(actual, entry)| actual == &entry.old_head);
    let all_target = observed
        .iter()
        .zip(&journal.repositories)
        .all(|(actual, entry)| actual == &entry.target_head);
    if all_old || all_target {
        return remove_update_journal(&path);
    }
    let recognized = observed
        .iter()
        .zip(&journal.repositories)
        .all(|(actual, entry)| actual == &entry.old_head || actual == &entry.target_head);
    if !recognized {
        return Err(CliError(
            "interrupted source update found a checkout at neither its old nor target head; refusing destructive recovery"
                .into(),
        ));
    }
    let old_heads = journal
        .repositories
        .iter()
        .map(|entry| entry.old_head.clone())
        .collect::<Vec<_>>();
    let target_heads = journal
        .repositories
        .iter()
        .map(|entry| entry.target_head.clone())
        .collect::<Vec<_>>();
    let errors = rollback_heads(&loaded.workspace, &repositories, &old_heads, &target_heads);
    if errors.is_empty() {
        remove_update_journal(&path)
    } else {
        Err(CliError(format!(
            "interrupted source update recovery was incomplete:\n{}",
            errors.join("\n")
        )))
    }
}

fn valid_object_id(value: &str) -> bool {
    (40..=64).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn rollback_heads(
    workspace: &Path,
    repositories: &[&Repository],
    old_heads: &[String],
    target_heads: &[String],
) -> Vec<String> {
    repositories
        .iter()
        .zip(old_heads)
        .zip(target_heads)
        .rev()
        .filter_map(|((repository, old_head), target_head)| {
            rollback_head(workspace, repository, old_head, target_head).err()
        })
        .map(|error| error.0)
        .collect()
}

fn rollback_head(
    workspace: &Path,
    repository: &Repository,
    old_head: &str,
    target_head: &str,
) -> Result<()> {
    let path = repository_path(workspace, repository);
    inspect_identity(workspace, repository, &path, true).map_err(|error| {
        CliError(format!(
            "repository `{}` changed while rollback was pending; refusing the conditional rollback and retaining the transaction journal for manual recovery: {}",
            repository.id, error.0
        ))
    })?;
    let current_head = capture_head(workspace, repository).map_err(|error| {
        CliError(format!(
            "repository `{}` head could not be revalidated immediately before rollback; refusing the conditional rollback and retaining the transaction journal for manual recovery: {}",
            repository.id, error.0
        ))
    })?;
    if current_head != old_head && current_head != target_head {
        return Err(CliError(format!(
            "repository `{}` is at `{current_head}`, neither transaction head `{old_head}` nor `{target_head}`; refusing destructive reset and retaining the transaction journal for manual recovery",
            repository.id
        )));
    }
    if current_head == old_head {
        return Ok(());
    }
    let ref_update =
        render_rollback_command(&repository.git.rollback.ref_update, old_head, target_head);
    let output = run_captured(&ref_update, workspace, "conditional rollback ref update").map_err(
        |error| {
            CliError(format!(
                "{}; the transaction journal is retained for manual recovery",
                error.0
            ))
        },
    )?;
    if !output.status.success() {
        return Err(CliError(format!(
            "repository `{}` moved away from expected transaction head `{target_head}` immediately before rollback; refusing to overwrite the concurrent ref update and retaining the transaction journal: {}",
            repository.id,
            failure_detail(&output)
        )));
    }

    let worktree_restore = render_rollback_command(
        &repository.git.rollback.worktree_restore,
        old_head,
        target_head,
    );
    let worktree_result = run_captured(
        &worktree_restore,
        workspace,
        "conditional rollback worktree restore",
    );
    let worktree_error = match worktree_result {
        Ok(output) if output.status.success() => None,
        Ok(output) => Some(failure_detail(&output)),
        Err(error) => Some(error.0),
    };
    if let Some(worktree_error) = worktree_error {
        let ref_restore =
            render_rollback_command(&repository.git.rollback.ref_restore, old_head, target_head);
        let ref_restore_detail = match run_captured(
            &ref_restore,
            workspace,
            "conditional rollback ref restoration",
        ) {
            Ok(output) if output.status.success() => {
                "the original ref was restored conditionally".to_owned()
            }
            Ok(output) => format!(
                "the original ref could not be restored because it moved concurrently: {}",
                failure_detail(&output)
            ),
            Err(error) => format!("the original ref could not be restored: {}", error.0),
        };
        return Err(CliError(format!(
            "repository `{}` worktree could not be restored safely after its conditional ref update; {ref_restore_detail}; the transaction journal is retained for manual recovery: {worktree_error}",
            repository.id
        )));
    }

    inspect_identity(workspace, repository, &path, true).map_err(|error| {
        CliError(format!(
            "repository `{}` diverged while its rollback worktree was being restored; the transaction journal is retained for manual recovery: {}",
            repository.id, error.0
        ))
    })?;
    let final_head = capture_head(workspace, repository)?;
    if final_head == old_head {
        Ok(())
    } else {
        Err(CliError(format!(
            "repository `{}` moved to `{final_head}` while rollback was completing instead of remaining at `{old_head}`; the concurrent ref update was not overwritten and the transaction journal is retained for manual recovery",
            repository.id
        )))
    }
}

fn render_rollback_command(
    template: &RollbackCommandTemplate,
    old_head: &str,
    expected_current: &str,
) -> GitCommand {
    GitCommand {
        argv: template
            .argv
            .iter()
            .map(|argument| match argument {
                RollbackArgument::Literal { value } => value.clone(),
                RollbackArgument::OldHead => old_head.to_owned(),
                RollbackArgument::ExpectedCurrent => expected_current.to_owned(),
            })
            .collect(),
    }
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
    for (name, path) in [
        ("mutationLock", &plan.transaction.mutation_lock),
        ("journal", &plan.transaction.journal),
    ] {
        if !safe_workspace_file(path) {
            return Err(CliError(format!(
                "source workspace transaction.{name} must be a nonempty workspace-relative file path without `.`, `..`, or platform prefixes"
            )));
        }
    }
    if plan.transaction.mutation_lock == plan.transaction.journal {
        return Err(CliError(
            "source workspace transaction lock and journal paths must be distinct".into(),
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
        for (name, command) in [
            ("refUpdate", &repository.git.rollback.ref_update),
            ("worktreeRestore", &repository.git.rollback.worktree_restore),
            ("refRestore", &repository.git.rollback.ref_restore),
        ] {
            validate_rollback_command(key, name, command)?;
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

fn validate_rollback_command(
    repository: &str,
    name: &str,
    command: &RollbackCommandTemplate,
) -> Result<()> {
    let old_head_parameters = command
        .argv
        .iter()
        .filter(|argument| matches!(argument, RollbackArgument::OldHead))
        .count();
    let expected_current_parameters = command
        .argv
        .iter()
        .filter(|argument| matches!(argument, RollbackArgument::ExpectedCurrent))
        .count();
    if command.argv.is_empty()
        || old_head_parameters != 1
        || expected_current_parameters != 1
        || command.argv.iter().any(|argument| {
            matches!(argument, RollbackArgument::Literal { value } if value.is_empty() || value.contains('\0'))
        })
        || !matches!(
            command.argv.first(),
            Some(RollbackArgument::Literal { .. })
        )
    {
        return Err(CliError(format!(
            "source repository `{repository}` rollback.{name} argv template must begin with a nonempty literal executable, contain no NUL, and declare exactly one typed old-head and expected-current parameter"
        )));
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

fn safe_workspace_file(path: &Path) -> bool {
    let rendered = path.to_string_lossy();
    !path.as_os_str().is_empty()
        && !rendered.contains('\\')
        && !rendered
            .split('/')
            .next()
            .is_some_and(|segment| segment.ends_with(':'))
        && path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

fn commands(repository: &Repository) -> [(&'static str, &GitCommand); 11] {
    [
        ("clone", &repository.git.clone),
        ("status", &repository.git.status),
        ("worktree inspect", &repository.git.inspect.worktree),
        ("origin inspect", &repository.git.inspect.origin),
        ("branch inspect", &repository.git.inspect.branch),
        ("clean inspect", &repository.git.inspect.clean),
        ("head inspect", &repository.git.inspect.head),
        ("target inspect", &repository.git.inspect.target),
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
