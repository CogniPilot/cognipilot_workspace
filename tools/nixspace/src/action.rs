use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use clap::Args;
use fs4::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{ActionRunner, BootstrapEnvironmentValue, Index, ACTION_PLAN_INTERFACE_VERSION};
use crate::{canonical_package, write_json, CliError, Result};

const SUPPORTED_API_VERSION: &str = "nixspace/v1";
const SUPPORTED_TASK_KIND: &str = "ActionTask";
const SUPPORTED_TASK_INTERFACE_VERSION: u64 = 3;
const SUPPORTED_GENERATION_STORE_KIND: &str = "ActionGenerationStore";
const SUPPORTED_GENERATION_STORE_INTERFACE_VERSION: u64 = 2;
const SUPPORTED_GENERATION_INTERFACE_VERSION: u64 = 1;
const SUPPORTED_GENERATION_LAYOUT_KIND: &str = "ActionGenerationLayout";
const SUPPORTED_GENERATION_LAYOUT_INTERFACE_VERSION: u64 = 1;
const SUPPORTED_TASK_IDENTITY_KIND: &str = "ActionTaskIdentity";
const SUPPORTED_TASK_IDENTITY_INTERFACE_VERSION: u64 = 1;
const TASK_OUTPUT_ENVIRONMENT: &str = "DEVENV_TASK_OUTPUT_FILE";

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Args)]
pub(crate) struct ActionArgs {
    /// Select the exact Nix-emitted plan for this package or alias.
    #[arg(value_name = "PACKAGE")]
    package: Option<String>,

    /// Print the selected Nix-emitted invocation without executing it.
    #[arg(long)]
    plan: bool,

    /// Render a machine-readable plan (requires --plan).
    #[arg(long, requires = "plan")]
    json: bool,
}

impl ActionArgs {
    pub(crate) fn package(&self) -> Option<&str> {
        self.package.as_deref()
    }
}

#[derive(Debug, Args)]
#[group(required = true, multiple = false)]
pub(crate) struct RunTaskArgs {
    /// Decode the Nix-generated ActionTask document from this JSON string.
    #[arg(long, value_name = "JSON")]
    plan_json: Option<String>,

    /// Decode the Nix-generated ActionTask document from this file.
    #[arg(long, value_name = "PATH")]
    plan_file: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActionTask {
    api_version: String,
    kind: String,
    interface_version: u64,
    cwd: PathBuf,
    argv: Vec<ActionArgument>,
    environment: BTreeMap<String, String>,
    environment_paths: BTreeMap<String, PathBuf>,
    path_prefixes: Vec<PathBuf>,
    locks: Vec<PathBuf>,
    outputs: Vec<ActionOutput>,
    generation: ActionGenerationStore,
    result: Value,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ActionOutput {
    coordinate: String,
    path: PathBuf,
    kind: OutputKind,
    contract: OutputContract,
    proof: OutputProofCommand,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum OutputKind {
    File,
    Executable,
    Directory,
    Symlink,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct OutputContract {
    name: String,
    version: u64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OutputProofCommand {
    kind: String,
    argv_prefix: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OutputResult {
    coordinate: String,
    path: PathBuf,
    kind: OutputKind,
    contract: OutputContract,
    proof: OutputProof,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    symlink_target: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct OutputProof {
    kind: String,
    digest: String,
}

struct ValidatedOutput<'a> {
    declaration: &'a ActionOutput,
    path: PathBuf,
    mode: Option<u32>,
    symlink_target: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum ActionArgument {
    Literal(String),
    Path {
        path: PathBuf,
        prefix: String,
        suffix: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActionGenerationStore {
    api_version: String,
    kind: String,
    interface_version: u64,
    root: PathBuf,
    layout: ActionGenerationLayout,
    identity: ActionTaskIdentity,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActionGenerationLayout {
    api_version: String,
    kind: String,
    interface_version: u64,
    publication_lock: PathBuf,
    generations: PathBuf,
    pointer: PathBuf,
    manifest: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActionTaskIdentity {
    api_version: String,
    kind: String,
    interface_version: u64,
    declaration: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActionGenerationManifest<'a> {
    api_version: &'static str,
    kind: &'static str,
    interface_version: u64,
    generation: &'a str,
    identity: ActionGenerationIdentity<'a>,
    execution: ActionExecution,
    outputs: &'a [OutputResult],
    result: &'a Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActionGenerationIdentity<'a> {
    declared: &'a ActionTaskIdentity,
    cwd: &'a Path,
    argv: &'a [ActionArgument],
    environment: &'a BTreeMap<String, String>,
    environment_paths: &'a BTreeMap<String, PathBuf>,
    path_prefixes: &'a [PathBuf],
    locks: &'a [PathBuf],
    outputs: &'a [ActionOutput],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActionExecution {
    started_unix_millis: u64,
    finished_unix_millis: u64,
    duration_millis: u64,
    exit_status: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActionGenerationPointer<'a> {
    api_version: &'static str,
    kind: &'static str,
    interface_version: u64,
    generation: &'a str,
    manifest: &'a Path,
    identity: &'a ActionTaskIdentity,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SelectedActionPlan<'a> {
    api_version: &'static str,
    kind: &'static str,
    interface_version: u64,
    action: &'a str,
    package: Option<&'a str>,
    runner: &'a ActionRunner,
    runner_mode: &'static str,
    argv: &'a [String],
    tasks: &'a [String],
}

pub(crate) enum Outcome {
    Success,
    Exit(i32),
}

pub(crate) fn run_action(
    root: &Path,
    index: &Index,
    action: &str,
    arguments: ActionArgs,
) -> Result<Outcome> {
    validate_action_plans(index)?;
    let selection = index.action_plans.actions.get(action).ok_or_else(|| {
        CliError(format!(
            "action `{action}` is not declared by the cached workspace index"
        ))
    })?;
    let canonical = arguments
        .package
        .as_deref()
        .map(|package| canonical_package(index, package).map(|entry| entry.id.as_str()))
        .transpose()?;
    let tasks = if let Some(package) = canonical {
        selection.packages.get(package).ok_or_else(|| {
            CliError(format!(
                "action plan `{action}` has no emitted package entry for `{package}`"
            ))
        })?
    } else {
        &selection.all
    };
    if tasks.is_empty() {
        let scope = canonical
            .map(|package| format!(" for package `{package}`"))
            .unwrap_or_default();
        return Err(CliError(format!(
            "no `{action}` tasks are declared{scope} by the cached workspace index"
        )));
    }

    let (runner_mode, runner_argv) = selected_runner(&index.action_plans.runner);
    let selected = SelectedActionPlan {
        api_version: SUPPORTED_API_VERSION,
        kind: "ActionInvocation",
        interface_version: ACTION_PLAN_INTERFACE_VERSION,
        action,
        package: canonical,
        runner: &index.action_plans.runner,
        runner_mode,
        argv: runner_argv,
        tasks,
    };
    if arguments.plan {
        if arguments.json {
            write_json(&selected)?;
        } else {
            println!("Action: {}", selected.action);
            println!("Package: {}", selected.package.unwrap_or("all"));
            println!("Runner kind: {}", selected.runner.kind);
            println!("Runner mode: {}", selected.runner_mode);
            println!(
                "Argv: {}",
                serde_json::to_string(
                    &selected
                        .argv
                        .iter()
                        .chain(selected.tasks.iter())
                        .collect::<Vec<_>>()
                )
                .expect("action invocation is serializable")
            );
        }
        return Ok(Outcome::Success);
    }

    let executable = &selected.argv[0];
    let mut command = Command::new(executable);
    command
        .args(&selected.argv[1..])
        .args(selected.tasks)
        .current_dir(root);
    if selected.runner_mode == "bootstrap" {
        for (name, value) in &selected.runner.bootstrap.environment {
            match value {
                BootstrapEnvironmentValue::WorkspaceRoot => {
                    command.env(name, root);
                }
            }
        }
    }
    let status = command.status().map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            CliError(format!(
                "action runner `{executable}` is unavailable on PATH"
            ))
        } else {
            CliError(format!(
                "cannot start action runner `{executable}`: {error}"
            ))
        }
    })?;
    if status.success() {
        Ok(Outcome::Success)
    } else {
        Ok(Outcome::Exit(status.code().unwrap_or(1)))
    }
}

pub(crate) fn run_task(root: &Path, arguments: RunTaskArgs) -> Result<Outcome> {
    let (bytes, source) = match (arguments.plan_json, arguments.plan_file) {
        (Some(document), None) => (document.into_bytes(), "--plan-json".to_owned()),
        (None, Some(path)) => {
            let bytes = fs::read(&path).map_err(|error| {
                CliError(format!(
                    "cannot read Nix-generated ActionTask plan at {}: {error}",
                    path.display()
                ))
            })?;
            (bytes, path.display().to_string())
        }
        _ => {
            return Err(CliError(
                "exactly one of --plan-json or --plan-file is required".into(),
            ))
        }
    };
    let task: ActionTask = serde_json::from_slice(&bytes).map_err(|error| {
        CliError(format!(
            "Nix-generated ActionTask plan from {source} is unreadable: {error}"
        ))
    })?;
    validate_task(root, &task)?;

    let cwd = if task.cwd.is_absolute() {
        task.cwd.clone()
    } else {
        root.join(&task.cwd)
    };
    let generation_root = resolve_path(root, &task.generation.root);
    let mut lock_paths = task.locks.clone();
    lock_paths.push(generation_root.join(&task.generation.layout.publication_lock));
    let _locks = ActionLocks::acquire(root, &lock_paths)?;
    let argv: Vec<_> = task
        .argv
        .iter()
        .map(|argument| argument.materialize(root))
        .collect::<Result<_>>()?;
    let executable = &argv[0];
    let environment_paths: BTreeMap<_, _> = task
        .environment_paths
        .iter()
        .map(|(name, path)| {
            (
                name,
                if path.is_absolute() {
                    path.clone()
                } else {
                    root.join(path)
                },
            )
        })
        .collect();
    let task_path = prefixed_path(&task.path_prefixes)?;
    let mut command = Command::new(executable);
    command
        .args(&argv[1..])
        .current_dir(&cwd)
        .envs(&task.environment)
        .envs(&environment_paths);
    if let Some(path) = task_path {
        command.env("PATH", path);
    }
    let started_at = SystemTime::now();
    let started = Instant::now();
    let status = command.status().map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            CliError(format!(
                "action executable `{}` is unavailable in {}",
                executable.to_string_lossy(),
                cwd.display()
            ))
        } else {
            CliError(format!(
                "cannot start action executable `{}` in {}: {error}",
                executable.to_string_lossy(),
                cwd.display()
            ))
        }
    })?;
    if !status.success() {
        return Ok(Outcome::Exit(status.code().unwrap_or(1)));
    }

    let validated = validate_outputs(root, &task.outputs)?;
    let proofs = prove_outputs(root, validated)?;
    let finished_at = SystemTime::now();
    let execution = ActionExecution {
        started_unix_millis: unix_millis(started_at)?,
        finished_unix_millis: unix_millis(finished_at)?,
        duration_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        exit_status: 0,
    };

    let output = env::var_os(TASK_OUTPUT_ENVIRONMENT).ok_or_else(|| {
        CliError(format!(
            "devenv did not declare {TASK_OUTPUT_ENVIRONMENT} for the successful action task"
        ))
    })?;
    let output = PathBuf::from(output);
    validate_output_destination(&output)?;
    let mut result = task.result.clone();
    result
        .as_object_mut()
        .expect("validated ActionTask result object")
        .insert(
            "outputs".into(),
            serde_json::to_value(&proofs).map_err(|error| {
                CliError(format!("cannot serialize action output proofs: {error}"))
            })?,
        );
    let mut result_bytes = serde_json::to_vec(&result)
        .map_err(|error| CliError(format!("cannot serialize declared task result: {error}")))?;
    result_bytes.push(b'\n');
    let staged_generation =
        StagedGeneration::stage(&generation_root, &task, &proofs, &result, execution)?;
    atomic_write(&output, &result_bytes)?;
    staged_generation.publish()?;
    Ok(Outcome::Success)
}

fn validate_action_plans(index: &Index) -> Result<()> {
    if index.action_plans.schema_version != ACTION_PLAN_INTERFACE_VERSION {
        return Err(CliError(format!(
            "action-plan interface version {} is unsupported; this nixspace supports version {}",
            index.action_plans.schema_version, ACTION_PLAN_INTERFACE_VERSION
        )));
    }
    if index.action_plans.runner.kind != "devenv-task" {
        return Err(CliError(format!(
            "action-plan runner kind `{}` is unsupported; expected `devenv-task`",
            index.action_plans.runner.kind
        )));
    }
    for (mode, argv) in [
        ("direct", &index.action_plans.runner.direct.argv),
        ("bootstrap", &index.action_plans.runner.bootstrap.argv),
    ] {
        if argv.is_empty()
            || argv[0].is_empty()
            || argv.iter().any(|argument| argument.contains('\0'))
        {
            return Err(CliError(format!(
                "action-plan {mode} runner argv must start with a nonempty executable and contain no NUL bytes"
            )));
        }
    }
    let mut required = index
        .action_plans
        .runner
        .direct
        .required_environment
        .clone();
    required.sort();
    for (position, name) in required.iter().enumerate() {
        if name.is_empty()
            || name.contains('=')
            || name.contains('\0')
            || (position > 0 && name == &required[position - 1])
        {
            return Err(CliError(
                "action-plan direct runner requiredEnvironment must contain unique nonempty environment names without `=` or NUL"
                    .into(),
            ));
        }
    }
    for name in index.action_plans.runner.bootstrap.environment.keys() {
        let mut characters = name.chars();
        if characters
            .next()
            .is_none_or(|first| first != '_' && !first.is_ascii_alphabetic())
            || characters.any(|character| character != '_' && !character.is_ascii_alphanumeric())
        {
            return Err(CliError(
                "action-plan bootstrap environment names must be portable environment variable identifiers"
                    .into(),
            ));
        }
    }
    Ok(())
}

fn selected_runner(runner: &ActionRunner) -> (&'static str, &[String]) {
    if runner
        .direct
        .required_environment
        .iter()
        .all(|name| env::var_os(name).is_some())
    {
        ("direct", &runner.direct.argv)
    } else {
        ("bootstrap", &runner.bootstrap.argv)
    }
}

fn validate_task(root: &Path, task: &ActionTask) -> Result<()> {
    if task.api_version != SUPPORTED_API_VERSION {
        return Err(CliError(format!(
            "ActionTask API `{}` is unsupported; this nixspace requires `{SUPPORTED_API_VERSION}`",
            task.api_version
        )));
    }
    if task.kind != SUPPORTED_TASK_KIND {
        return Err(CliError(format!(
            "task kind `{}` is unsupported; this nixspace requires `{SUPPORTED_TASK_KIND}`",
            task.kind
        )));
    }
    if task.interface_version != SUPPORTED_TASK_INTERFACE_VERSION {
        return Err(CliError(format!(
            "ActionTask interface version {} is unsupported; this nixspace supports version {}",
            task.interface_version, SUPPORTED_TASK_INTERFACE_VERSION
        )));
    }
    if task.generation.api_version != SUPPORTED_API_VERSION {
        return Err(CliError(format!(
            "ActionGenerationStore API `{}` is unsupported; this nixspace requires `{SUPPORTED_API_VERSION}`",
            task.generation.api_version
        )));
    }
    if task.generation.kind != SUPPORTED_GENERATION_STORE_KIND {
        return Err(CliError(format!(
            "generation kind `{}` is unsupported; this nixspace requires `{SUPPORTED_GENERATION_STORE_KIND}`",
            task.generation.kind
        )));
    }
    if task.generation.interface_version != SUPPORTED_GENERATION_STORE_INTERFACE_VERSION {
        return Err(CliError(format!(
            "ActionGenerationStore interface version {} is unsupported; this nixspace supports version {}",
            task.generation.interface_version, SUPPORTED_GENERATION_STORE_INTERFACE_VERSION
        )));
    }
    validate_generation_layout(&task.generation.layout)?;
    let generation_root = task
        .generation
        .root
        .to_str()
        .ok_or_else(|| CliError("ActionGenerationStore root is not valid Unicode".into()))?;
    if generation_root.is_empty()
        || generation_root.contains('\0')
        || !portable_relative_path(&task.generation.root)
    {
        return Err(CliError(
            "ActionGenerationStore root must be a portable workspace-relative path with no empty, `.` or `..` components"
                .into(),
        ));
    }
    if task.generation.identity.api_version != SUPPORTED_API_VERSION {
        return Err(CliError(format!(
            "ActionTaskIdentity API `{}` is unsupported; this nixspace requires `{SUPPORTED_API_VERSION}`",
            task.generation.identity.api_version
        )));
    }
    if task.generation.identity.kind != SUPPORTED_TASK_IDENTITY_KIND {
        return Err(CliError(format!(
            "generation identity kind `{}` is unsupported; this nixspace requires `{SUPPORTED_TASK_IDENTITY_KIND}`",
            task.generation.identity.kind
        )));
    }
    if task.generation.identity.interface_version != SUPPORTED_TASK_IDENTITY_INTERFACE_VERSION {
        return Err(CliError(format!(
            "ActionTaskIdentity interface version {} is unsupported; this nixspace supports version {}",
            task.generation.identity.interface_version, SUPPORTED_TASK_IDENTITY_INTERFACE_VERSION
        )));
    }
    if task
        .generation
        .identity
        .declaration
        .as_object()
        .is_none_or(|identity| identity.is_empty())
    {
        return Err(CliError(
            "ActionTaskIdentity declaration must be a nonempty object emitted by Nix".into(),
        ));
    }
    if task.cwd.as_os_str().is_empty() {
        return Err(CliError("ActionTask cwd must not be empty".into()));
    }
    if task.argv.is_empty() {
        return Err(CliError(
            "ActionTask argv must declare a nonempty executable".into(),
        ));
    }
    for (index, argument) in task.argv.iter().enumerate() {
        argument.validate(index)?;
    }
    for (name, value) in &task.environment {
        if name.is_empty() || name.contains('=') || name.contains('\0') {
            return Err(CliError(format!(
                "ActionTask environment key `{name}` must be nonempty and contain neither `=` nor NUL"
            )));
        }
        if value.contains('\0') {
            return Err(CliError(format!(
                "ActionTask environment value for `{name}` contains NUL"
            )));
        }
    }
    for (name, value) in &task.environment_paths {
        if name.is_empty() || name.contains('=') || name.contains('\0') {
            return Err(CliError(format!(
                "ActionTask environmentPaths key `{name}` must be nonempty and contain neither `=` nor NUL"
            )));
        }
        if task.environment.contains_key(name) {
            return Err(CliError(format!(
                "ActionTask environment key `{name}` is declared as both a literal and a path"
            )));
        }
        let value = value.to_str().ok_or_else(|| {
            CliError(format!(
                "ActionTask environment path for `{name}` is not valid Unicode"
            ))
        })?;
        if value.is_empty() || value.contains('\0') {
            return Err(CliError(format!(
                "ActionTask environment path for `{name}` must be nonempty and contain no NUL"
            )));
        }
    }
    if !task.path_prefixes.is_empty() && task.environment.contains_key("PATH") {
        return Err(CliError(
            "ActionTask cannot combine pathPrefixes with a literal PATH environment value".into(),
        ));
    }
    let mut path_prefixes = std::collections::BTreeSet::new();
    for (index, path) in task.path_prefixes.iter().enumerate() {
        let value = path.to_str().ok_or_else(|| {
            CliError(format!(
                "ActionTask pathPrefixes entry at index {index} is not valid Unicode"
            ))
        })?;
        if !path.is_absolute() || value.contains('\0') {
            return Err(CliError(format!(
                "ActionTask pathPrefixes entry at index {index} must be an absolute path containing no NUL"
            )));
        }
        if !path_prefixes.insert(path) {
            return Err(CliError(format!(
                "ActionTask path prefix `{}` is declared more than once",
                path.display()
            )));
        }
    }
    let mut locks = task.locks.clone();
    locks.sort();
    for (index, path) in locks.iter().enumerate() {
        let value = path.to_str().ok_or_else(|| {
            CliError(format!(
                "ActionTask lock path at index {index} is not valid Unicode"
            ))
        })?;
        if value.is_empty() || value.contains('\0') {
            return Err(CliError(format!(
                "ActionTask lock path at index {index} must be nonempty and contain no NUL"
            )));
        }
        if index > 0 && path == &locks[index - 1] {
            return Err(CliError(format!(
                "ActionTask lock path `{}` is declared more than once",
                path.display()
            )));
        }
    }
    if !task.result.is_object() || task.result.get("outputs").is_some() {
        return Err(CliError(
            "ActionTask result must be an object without a predeclared `outputs` field".into(),
        ));
    }
    let mut coordinates = std::collections::BTreeSet::new();
    let mut paths = std::collections::BTreeSet::new();
    for output in &task.outputs {
        let rendered = output.path.to_str().ok_or_else(|| {
            CliError(format!(
                "ActionTask output `{}` path is not valid Unicode",
                output.coordinate
            ))
        })?;
        if output.coordinate.is_empty()
            || rendered.is_empty()
            || rendered.contains('\0')
            || output.contract.name.is_empty()
            || output.proof.kind != "nix-nar-sha256"
            || output.proof.argv_prefix.is_empty()
            || output.proof.argv_prefix[0].is_empty()
            || output
                .proof
                .argv_prefix
                .iter()
                .any(|argument| argument.contains('\0'))
        {
            return Err(CliError(format!(
                "ActionTask output `{}` has an invalid path, contract, or proof command",
                output.coordinate
            )));
        }
        if !coordinates.insert(&output.coordinate) {
            return Err(CliError(format!(
                "ActionTask output coordinate `{}` is declared more than once",
                output.coordinate
            )));
        }
        let resolved = if output.path.is_absolute() {
            output.path.clone()
        } else {
            root.join(&output.path)
        };
        let resolved_generation_root = resolve_path(root, &task.generation.root);
        if resolved.starts_with(&resolved_generation_root)
            || resolved_generation_root.starts_with(&resolved)
        {
            return Err(CliError(format!(
                "ActionTask output `{}` overlaps ActionGenerationStore root {}",
                output.coordinate,
                resolved_generation_root.display()
            )));
        }
        if !paths.insert(resolved) {
            return Err(CliError(format!(
                "ActionTask output path `{}` is declared more than once",
                output.path.display()
            )));
        }
    }
    Ok(())
}

fn validate_generation_layout(layout: &ActionGenerationLayout) -> Result<()> {
    if layout.api_version != SUPPORTED_API_VERSION {
        return Err(CliError(format!(
            "ActionGenerationLayout API `{}` is unsupported; this nixspace requires `{SUPPORTED_API_VERSION}`",
            layout.api_version
        )));
    }
    if layout.kind != SUPPORTED_GENERATION_LAYOUT_KIND {
        return Err(CliError(format!(
            "generation layout kind `{}` is unsupported; this nixspace requires `{SUPPORTED_GENERATION_LAYOUT_KIND}`",
            layout.kind
        )));
    }
    if layout.interface_version != SUPPORTED_GENERATION_LAYOUT_INTERFACE_VERSION {
        return Err(CliError(format!(
            "ActionGenerationLayout interface version {} is unsupported; this nixspace supports version {SUPPORTED_GENERATION_LAYOUT_INTERFACE_VERSION}",
            layout.interface_version
        )));
    }
    for (name, path) in [
        ("publicationLock", &layout.publication_lock),
        ("generations", &layout.generations),
        ("pointer", &layout.pointer),
        ("manifest", &layout.manifest),
    ] {
        if !portable_relative_path(path) {
            return Err(CliError(format!(
                "ActionGenerationLayout {name} must be a portable relative path with no empty, `.` or `..` components"
            )));
        }
    }
    for (left_name, left, right_name, right) in [
        (
            "publicationLock",
            &layout.publication_lock,
            "generations",
            &layout.generations,
        ),
        (
            "publicationLock",
            &layout.publication_lock,
            "pointer",
            &layout.pointer,
        ),
        (
            "generations",
            &layout.generations,
            "pointer",
            &layout.pointer,
        ),
    ] {
        if left.starts_with(right) || right.starts_with(left) {
            return Err(CliError(format!(
                "ActionGenerationLayout {left_name} and {right_name} paths must be distinct and non-overlapping"
            )));
        }
    }
    Ok(())
}

fn prefixed_path(prefixes: &[PathBuf]) -> Result<Option<OsString>> {
    if prefixes.is_empty() {
        return Ok(None);
    }
    let inherited = env::var_os("PATH");
    let mut paths = prefixes.to_vec();
    if let Some(inherited) = inherited.as_deref() {
        paths.extend(env::split_paths(inherited));
    }
    env::join_paths(paths).map(Some).map_err(|error| {
        CliError(format!(
            "cannot construct PATH from Nix-generated ActionTask pathPrefixes: {error}"
        ))
    })
}

fn resolve_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    }
}

fn portable_relative_path(path: &Path) -> bool {
    !path.is_absolute()
        && path.to_str().is_some_and(|path| {
            path.split('/').all(|segment| {
                !segment.is_empty()
                    && segment != "."
                    && segment != ".."
                    && segment.chars().all(|character| {
                        character.is_ascii_alphanumeric() || "._-".contains(character)
                    })
            })
        })
}

fn unix_millis(time: SystemTime) -> Result<u64> {
    let millis = time
        .duration_since(UNIX_EPOCH)
        .map_err(|error| CliError(format!("system clock precedes the Unix epoch: {error}")))?
        .as_millis();
    Ok(u64::try_from(millis).unwrap_or(u64::MAX))
}

fn validate_outputs<'a>(
    root: &Path,
    outputs: &'a [ActionOutput],
) -> Result<Vec<ValidatedOutput<'a>>> {
    outputs
        .iter()
        .map(|output| {
            let path = if output.path.is_absolute() {
                output.path.clone()
            } else {
                root.join(&output.path)
            };
            let link_metadata = fs::symlink_metadata(&path).map_err(|error| {
                CliError(format!(
                    "ActionTask output `{}` is unavailable at {}: {error}",
                    output.coordinate,
                    path.display()
                ))
            })?;
            let followed_metadata;
            let metadata = match output.kind {
                OutputKind::Symlink => &link_metadata,
                _ => {
                    followed_metadata = fs::metadata(&path).map_err(|error| {
                        CliError(format!(
                            "ActionTask output `{}` target is unavailable at {}: {error}",
                            output.coordinate,
                            path.display()
                        ))
                    })?;
                    &followed_metadata
                }
            };
            let valid_kind = match output.kind {
                OutputKind::File => metadata.is_file(),
                OutputKind::Executable => metadata.is_file() && executable(metadata),
                OutputKind::Directory => metadata.is_dir(),
                OutputKind::Symlink => metadata.file_type().is_symlink(),
            };
            if !valid_kind {
                return Err(CliError(format!(
                    "ActionTask output `{}` at {} does not match declared kind `{:?}`",
                    output.coordinate,
                    path.display(),
                    output.kind
                )));
            }
            let mode = output_mode(metadata, output.kind);
            let symlink_target = if matches!(output.kind, OutputKind::Symlink) {
                Some(fs::read_link(&path).map_err(|error| {
                    CliError(format!(
                        "cannot read ActionTask output `{}` symlink target at {}: {error}",
                        output.coordinate,
                        path.display()
                    ))
                })?)
            } else {
                None
            };
            Ok(ValidatedOutput {
                declaration: output,
                path,
                mode,
                symlink_target,
            })
        })
        .collect()
}

fn prove_outputs(root: &Path, outputs: Vec<ValidatedOutput<'_>>) -> Result<Vec<OutputResult>> {
    outputs
        .into_iter()
        .map(|validated| {
            let declaration = validated.declaration;
            let (program, arguments) = declaration
                .proof
                .argv_prefix
                .split_first()
                .expect("validated proof argv");
            let output = Command::new(program)
                .args(arguments)
                .arg(&validated.path)
                .current_dir(root)
                .output()
                .map_err(|error| {
                    CliError(format!(
                        "cannot start output proof command `{program}` for `{}`: {error}",
                        declaration.coordinate
                    ))
                })?;
            if !output.status.success() {
                return Err(CliError(format!(
                    "output proof command for `{}` failed with {}: {}",
                    declaration.coordinate,
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
            }
            let stdout = String::from_utf8(output.stdout).map_err(|error| {
                CliError(format!(
                    "output proof command for `{}` returned non-UTF-8 output: {error}",
                    declaration.coordinate
                ))
            })?;
            let lines = stdout
                .lines()
                .filter(|line| !line.trim().is_empty())
                .collect::<Vec<_>>();
            if lines.len() != 1 || lines[0].trim() != lines[0] {
                return Err(CliError(format!(
                    "output proof command for `{}` must return exactly one digest line",
                    declaration.coordinate
                )));
            }
            Ok(OutputResult {
                coordinate: declaration.coordinate.clone(),
                path: validated.path,
                kind: declaration.kind,
                contract: declaration.contract.clone(),
                proof: OutputProof {
                    kind: declaration.proof.kind.clone(),
                    digest: lines[0].into(),
                },
                mode: validated.mode,
                symlink_target: validated.symlink_target,
            })
        })
        .collect()
}

#[cfg(unix)]
fn executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn executable(metadata: &fs::Metadata) -> bool {
    metadata.is_file()
}

#[cfg(unix)]
fn output_mode(metadata: &fs::Metadata, kind: OutputKind) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    matches!(kind, OutputKind::File | OutputKind::Executable)
        .then(|| metadata.permissions().mode() & 0o7777)
}

#[cfg(not(unix))]
fn output_mode(_metadata: &fs::Metadata, _kind: OutputKind) -> Option<u32> {
    None
}

struct ActionLocks(Vec<File>);

impl ActionLocks {
    fn acquire(root: &Path, declared: &[PathBuf]) -> Result<Self> {
        let mut paths: Vec<_> = declared
            .iter()
            .map(|path| {
                if path.is_absolute() {
                    path.clone()
                } else {
                    root.join(path)
                }
            })
            .collect();
        paths.sort();
        paths.dedup();
        let mut files = Vec::with_capacity(paths.len());
        for path in paths {
            let parent = path.parent().ok_or_else(|| {
                CliError(format!(
                    "ActionTask lock path has no parent: {}",
                    path.display()
                ))
            })?;
            fs::create_dir_all(parent).map_err(|error| {
                CliError(format!(
                    "cannot create ActionTask lock directory {}: {error}",
                    parent.display()
                ))
            })?;
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)
                .map_err(|error| {
                    CliError(format!(
                        "cannot open ActionTask lock {}: {error}",
                        path.display()
                    ))
                })?;
            FileExt::lock(&file).map_err(|error| {
                CliError(format!(
                    "cannot acquire ActionTask lock {}: {error}",
                    path.display()
                ))
            })?;
            files.push(file);
        }
        Ok(Self(files))
    }
}

impl Drop for ActionLocks {
    fn drop(&mut self) {
        for file in self.0.iter().rev() {
            let _ = FileExt::unlock(file);
        }
    }
}

impl ActionArgument {
    fn validate(&self, index: usize) -> Result<()> {
        match self {
            Self::Literal(value) => {
                if value.contains('\0') || (index == 0 && value.is_empty()) {
                    return Err(CliError(format!(
                        "ActionTask argv item {index} must contain no NUL and the executable must be nonempty"
                    )));
                }
            }
            Self::Path {
                path,
                prefix,
                suffix,
            } => {
                let path = path.to_str().ok_or_else(|| {
                    CliError(format!(
                        "ActionTask argv path at index {index} is not valid Unicode"
                    ))
                })?;
                if path.is_empty()
                    || path.contains('\0')
                    || prefix.contains('\0')
                    || suffix.contains('\0')
                {
                    return Err(CliError(format!(
                        "ActionTask argv path at index {index} must be nonempty and its path, prefix, and suffix must contain no NUL"
                    )));
                }
            }
        }
        Ok(())
    }

    fn materialize(&self, root: &Path) -> Result<OsString> {
        match self {
            Self::Literal(value) => Ok(OsString::from(value)),
            Self::Path {
                path,
                prefix,
                suffix,
            } => {
                let path = if path.is_absolute() {
                    path.clone()
                } else {
                    root.join(path)
                };
                let mut value = OsString::from(prefix);
                value.push(path.as_os_str());
                value.push(suffix);
                Ok(value)
            }
        }
    }
}

struct StagedGeneration {
    generations: PathBuf,
    pointer: PathBuf,
    staging: Option<PathBuf>,
    installed: PathBuf,
    current: Vec<u8>,
}

impl StagedGeneration {
    fn stage(
        root: &Path,
        task: &ActionTask,
        outputs: &[OutputResult],
        result: &Value,
        execution: ActionExecution,
    ) -> Result<Self> {
        fs::create_dir_all(root).map_err(|error| {
            CliError(format!(
                "cannot create ActionGenerationStore root {}: {error}",
                root.display()
            ))
        })?;
        let layout = &task.generation.layout;
        let generations = root.join(&layout.generations);
        fs::create_dir_all(&generations).map_err(|error| {
            CliError(format!(
                "cannot create ActionGenerationStore generations directory {}: {error}",
                generations.display()
            ))
        })?;

        for _ in 0..128 {
            let sequence = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
            let generation = format!(
                "{}-{}-{sequence}",
                unix_millis(SystemTime::now())?,
                std::process::id()
            );
            let staging = root.join(format!(
                ".generation.{}.{}.tmp",
                std::process::id(),
                sequence
            ));
            match fs::create_dir(&staging) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(CliError(format!(
                        "cannot create staged ActionGeneration in {}: {error}",
                        root.display()
                    )))
                }
            }
            let mut guard = TemporaryDirectory(Some(staging.clone()));
            let installed = generations.join(&generation);
            match installed.try_exists() {
                Ok(false) => {}
                Ok(true) => continue,
                Err(error) => {
                    return Err(CliError(format!(
                        "cannot inspect ActionGeneration destination {}: {error}",
                        installed.display()
                    )))
                }
            }
            let relative_manifest = layout.generations.join(&generation).join(&layout.manifest);
            let manifest = ActionGenerationManifest {
                api_version: SUPPORTED_API_VERSION,
                kind: "ActionGeneration",
                interface_version: SUPPORTED_GENERATION_INTERFACE_VERSION,
                generation: &generation,
                identity: ActionGenerationIdentity {
                    declared: &task.generation.identity,
                    cwd: &task.cwd,
                    argv: &task.argv,
                    environment: &task.environment,
                    environment_paths: &task.environment_paths,
                    path_prefixes: &task.path_prefixes,
                    locks: &task.locks,
                    outputs: &task.outputs,
                },
                execution,
                outputs,
                result,
            };
            let mut manifest_bytes = serde_json::to_vec(&manifest).map_err(|error| {
                CliError(format!(
                    "cannot serialize ActionGeneration manifest: {error}"
                ))
            })?;
            manifest_bytes.push(b'\n');
            let staged_manifest = staging.join(&layout.manifest);
            fs::create_dir_all(
                staged_manifest
                    .parent()
                    .expect("validated generation manifest has a parent"),
            )
            .map_err(|error| {
                CliError(format!(
                    "cannot create ActionGeneration manifest directory: {error}"
                ))
            })?;
            write_new_synced(&staged_manifest, &manifest_bytes)?;
            sync_directory(&staging)?;

            let pointer = ActionGenerationPointer {
                api_version: SUPPORTED_API_VERSION,
                kind: "ActionGenerationPointer",
                interface_version: SUPPORTED_GENERATION_INTERFACE_VERSION,
                generation: &generation,
                manifest: &relative_manifest,
                identity: &task.generation.identity,
            };
            let mut current = serde_json::to_vec(&pointer).map_err(|error| {
                CliError(format!("cannot serialize ActionGenerationPointer: {error}"))
            })?;
            current.push(b'\n');
            guard.0 = None;
            return Ok(Self {
                generations,
                pointer: root.join(&layout.pointer),
                staging: Some(staging),
                installed,
                current,
            });
        }
        Err(CliError(format!(
            "cannot allocate a unique staged ActionGeneration in {}",
            root.display()
        )))
    }

    fn publish(mut self) -> Result<()> {
        let staging = self
            .staging
            .as_ref()
            .expect("staged ActionGeneration has a directory");
        fs::rename(staging, &self.installed).map_err(|error| {
            CliError(format!(
                "cannot install ActionGeneration at {}: {error}",
                self.installed.display()
            ))
        })?;
        self.staging = None;
        if let Err(error) = sync_directory(&self.generations) {
            let _ = fs::remove_dir_all(&self.installed);
            return Err(error);
        }
        if let Some(parent) = self.pointer.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                let _ = fs::remove_dir_all(&self.installed);
                let _ = sync_directory(&self.generations);
                return Err(CliError(format!(
                    "cannot create ActionGeneration pointer directory {}: {error}",
                    parent.display()
                )));
            }
        }
        if let Err(error) = atomic_write(&self.pointer, &self.current) {
            let _ = fs::remove_dir_all(&self.installed);
            let _ = sync_directory(&self.generations);
            return Err(error);
        }
        Ok(())
    }
}

impl Drop for StagedGeneration {
    fn drop(&mut self) {
        if let Some(path) = self.staging.take() {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn write_new_synced(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            CliError(format!(
                "cannot create staged ActionGeneration manifest {}: {error}",
                path.display()
            ))
        })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            CliError(format!(
                "cannot write staged ActionGeneration manifest {}: {error}",
                path.display()
            ))
        })
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            CliError(format!(
                "cannot synchronize ActionGeneration directory {}: {error}",
                path.display()
            ))
        })
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}

struct TemporaryDirectory(Option<PathBuf>);

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn validate_output_destination(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| {
            CliError(format!(
                "{TASK_OUTPUT_ENVIRONMENT} must name a file in an existing directory"
            ))
        })?;
    if path.file_name().is_none() {
        return Err(CliError(format!(
            "{TASK_OUTPUT_ENVIRONMENT} must name an output file"
        )));
    }
    if !parent.is_dir() {
        return Err(CliError(format!(
            "task output directory does not exist: {}",
            parent.display()
        )));
    }
    Ok(())
}

fn atomic_write(destination: &Path, bytes: &[u8]) -> Result<()> {
    let parent = destination
        .parent()
        .expect("validated output destination has a parent");
    let name = destination
        .file_name()
        .expect("validated output destination has a file name")
        .to_string_lossy();
    let (temporary, mut file) = create_temporary(parent, &name)?;
    let mut guard = TemporaryFile(Some(temporary.clone()));
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            CliError(format!(
                "cannot write temporary task result {}: {error}",
                temporary.display()
            ))
        })?;
    drop(file);
    fs::rename(&temporary, destination).map_err(|error| {
        CliError(format!(
            "cannot atomically publish task result at {}: {error}",
            destination.display()
        ))
    })?;
    guard.0 = None;
    Ok(())
}

fn create_temporary(parent: &Path, destination_name: &str) -> Result<(PathBuf, File)> {
    for _ in 0..128 {
        let sequence = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".{destination_name}.nixspace.{}.{}.tmp",
            std::process::id(),
            sequence
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(CliError(format!(
                    "cannot create temporary task result in {}: {error}",
                    parent.display()
                )))
            }
        }
    }
    Err(CliError(format!(
        "cannot allocate a unique temporary task result in {}",
        parent.display()
    )))
}

struct TemporaryFile(Option<PathBuf>);

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_file(path);
        }
    }
}
