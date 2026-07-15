use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Subcommand, ValueEnum};
use fs4::FileExt;
use serde::{Deserialize, Serialize};

use super::{CliError, Result};

const WEST_PLAN_INTERFACE_VERSION: u64 = 3;
const WEST_CACHE_LAYOUT_VERSION: u64 = 2;
const STORE_TOOL_INTERFACE_VERSION: u64 = 2;
const PROJECT_PATH_ENVIRONMENT_INTERFACE_VERSION: u64 = 1;
const WEST_API_VERSION: &str = "nixspace/v1";
const WEST_KIND: &str = "WestWorkspace";
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Subcommand)]
pub(crate) enum WestCommand {
    /// Check that the Nix-generated plan is supported and operationally usable.
    Validate {
        #[arg(long)]
        json: bool,
    },
    /// Materialize the immutable checkout and its product-local editable view.
    Ensure {
        #[arg(long)]
        json: bool,
    },
    /// Refresh the immutable checkout with native West and rebuild its local view.
    Sync {
        #[arg(long)]
        json: bool,
    },
    /// Remove old plan-owned generations according to the Nix-declared retention policy.
    Gc {
        #[arg(long)]
        json: bool,
    },
    /// Report content-addressed paths without changing the workspace.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Print a content-addressed workspace path.
    Path {
        #[arg(long, value_enum)]
        mode: WestMode,
    },
    /// Print Nix-declared editable Zephyr module paths separated by semicolons.
    ExtraModules {
        #[arg(long, value_enum)]
        mode: WestMode,
    },
    /// Ensure the editable view, then invoke native West inside it.
    Exec {
        #[arg(required = true, allow_hyphen_values = true, trailing_var_arg = true)]
        arguments: Vec<String>,
    },
    /// Ensure the editable view, then invoke one exact external argv inside it.
    Run {
        /// Safe local-view-relative working directory for the external command.
        #[arg(long, value_name = "RELATIVE", default_value = ".")]
        cwd: PathBuf,
        #[arg(required = true, allow_hyphen_values = true, trailing_var_arg = true)]
        argv: Vec<String>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum WestMode {
    Local,
    Release,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WestPlan {
    api_version: String,
    kind: String,
    interface_version: u64,
    product: Product,
    workspace_root: String,
    workspace: Workspace,
    cache: CachePolicy,
    local_view: LocalViewPolicy,
    tools: Tools,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Product {
    id: String,
    interface_version: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Workspace {
    id: String,
    source: SourceBinding,
    manifest: ManifestBinding,
    content_key: String,
}

#[derive(Debug, Deserialize)]
struct SourceBinding {
    input: String,
    root: String,
    identity: SourceIdentity,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceIdentity {
    store_path: String,
    nar_hash: Option<String>,
    rev: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestBinding {
    resource: String,
    relative_path: String,
    store_path: PathBuf,
    sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachePolicy {
    layout_version: u64,
    namespace: String,
    retained_generations: usize,
    root: RootPolicy,
    native_path_cache: bool,
    narrow_update: bool,
    paths: CachePaths,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CachePaths {
    generations: PathBuf,
    generation_gc_root: PathBuf,
    current: PathBuf,
    publication_lock: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalViewPolicy {
    root: RootPolicy,
    overrides: Vec<LocalOverride>,
    policy_id: String,
    paths: LocalViewPaths,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocalViewPaths {
    generations: PathBuf,
    execution_lock: PathBuf,
}

#[derive(Debug, Deserialize)]
struct RootPolicy {
    base: RootBase,
    path: PathBuf,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum RootBase {
    PlatformCache,
    Workspace,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalOverride {
    project: String,
    source: PathBuf,
    required: bool,
    zephyr_module: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Tools {
    west: PathBuf,
    store: StoreTools,
    project_path_environment: ProjectPathEnvironment,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectPathEnvironment {
    interface_version: u64,
    count_variable: String,
    key_variable_prefix: String,
    value_variable_prefix: String,
    key: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoreTools {
    interface_version: u64,
    seal: ToolCommand,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolCommand {
    argv: Vec<ToolArgument>,
    output: ToolOutput,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolArgument {
    literal: Option<String>,
    parameter: Option<ToolParameter>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
enum ToolParameter {
    Source,
    GcRoot,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum ToolOutput {
    StorePath,
}

#[derive(Debug)]
struct WestPaths {
    generations: PathBuf,
    generation_gc_root: PathBuf,
    local_generations: PathBuf,
    current: PathBuf,
    lock: PathBuf,
    view_lock: PathBuf,
}

#[derive(Clone, Debug)]
struct ActiveWorkspace {
    generation: String,
    locked: PathBuf,
    local: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WestStatus<'a> {
    interface_version: u64,
    product: &'a str,
    workspace: &'a str,
    manifest_resource: &'a str,
    content_key: &'a str,
    generation: Option<String>,
    locked: PathBuf,
    local: PathBuf,
    path_cache_seed: Option<PathBuf>,
    ready: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WestGcReport<'a> {
    interface_version: u64,
    product: &'a str,
    workspace: &'a str,
    current_generation: String,
    retained_generations: usize,
    kept_generations: Vec<String>,
    removed_generations: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LockedMarker {
    interface_version: u64,
    product: String,
    workspace: String,
    content_key: String,
    manifest_resource: String,
    manifest_sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalMarker {
    interface_version: u64,
    product: String,
    workspace: String,
    content_key: String,
    manifest_sha256: String,
    policy_id: String,
    overrides: BTreeMap<String, String>,
    projects: BTreeMap<String, LocalProjectMarker>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalProjectMarker {
    path: String,
    source: String,
}

#[derive(Clone, Debug)]
struct LocalProject {
    name: String,
    relative: PathBuf,
    source: PathBuf,
}

#[derive(Clone, Debug)]
struct LocalProjection {
    projects: Vec<LocalProject>,
    overrides: BTreeMap<String, String>,
    markers: BTreeMap<String, LocalProjectMarker>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurrentGeneration {
    interface_version: u64,
    generation: String,
}

pub(crate) fn run(root: &Path, explicit_plan: Option<PathBuf>, command: WestCommand) -> Result<()> {
    let plan_path = west_plan_path(explicit_plan)?;
    let plan = load_plan(&plan_path)?;
    validate_plan(&plan, &plan_path)?;
    let workspace_root = resolve_workspace_root(root, &plan.workspace_root)?;
    let paths = west_paths(&workspace_root, &plan)?;

    match command {
        WestCommand::Validate { json } => {
            let status = status_document(&workspace_root, &plan, &paths);
            if json {
                super::write_json(&status)
            } else {
                println!(
                    "West plan valid: {} / {} ({})",
                    plan.product.id, plan.workspace.id, plan.workspace.content_key
                );
                Ok(())
            }
        }
        WestCommand::Ensure { json } => {
            materialize(&workspace_root, &plan, &paths, false)?;
            emit_status(&workspace_root, &plan, &paths, json)
        }
        WestCommand::Sync { json } => {
            materialize(&workspace_root, &plan, &paths, true)?;
            emit_status(&workspace_root, &plan, &paths, json)
        }
        WestCommand::Gc { json } => {
            let report = garbage_collect(&plan, &paths)?;
            if json {
                super::write_json(&report)
            } else {
                println!("current generation: {}", report.current_generation);
                println!(
                    "retained generations: {}",
                    report.kept_generations.join(", ")
                );
                println!(
                    "removed generations: {}",
                    if report.removed_generations.is_empty() {
                        "none".into()
                    } else {
                        report.removed_generations.join(", ")
                    }
                );
                Ok(())
            }
        }
        WestCommand::Status { json } => emit_status(&workspace_root, &plan, &paths, json),
        WestCommand::Path { mode } => {
            if !paths.current.is_file() {
                return Err(CliError(
                    "West workspace is not materialized; run `nixspace west ensure`".into(),
                ));
            }
            let selected = selected_workspace_path(&workspace_root, &plan, &paths, mode)?;
            println!("{}", selected.display());
            Ok(())
        }
        WestCommand::ExtraModules { mode } => {
            if matches!(mode, WestMode::Release) {
                println!();
                return Ok(());
            }
            let modules = extra_modules(&workspace_root, &plan)?;
            println!(
                "{}",
                modules
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(";")
            );
            Ok(())
        }
        WestCommand::Exec { arguments } => {
            materialize(&workspace_root, &plan, &paths, false)?;
            let _publication_lease = PublicationLease::acquire_shared(&paths.lock)?;
            let active = require_ready_workspace(&plan, &paths)?;
            let _view_lease = ViewLease::acquire(&paths.view_lock)?;
            let projection = ensure_local_projection(&workspace_root, &plan, &active)?;
            let environment =
                project_path_environment(&plan, &active.locked, &projection.projects)?;
            run_inherited_with_environment(
                &plan.tools.west,
                &arguments,
                &active.local,
                &environment,
                "Nix-selected native West",
            )
        }
        WestCommand::Run { cwd, argv } => {
            materialize(&workspace_root, &plan, &paths, false)?;
            let (program, arguments) = argv.split_first().expect("Clap requires external argv");
            if program.is_empty() || argv.iter().any(|argument| argument.contains('\0')) {
                return Err(CliError(
                    "West external argv must begin with a nonempty executable and contain no NUL"
                        .into(),
                ));
            }
            let _publication_lease = PublicationLease::acquire_shared(&paths.lock)?;
            let active = require_ready_workspace(&plan, &paths)?;
            let _view_lease = ViewLease::acquire(&paths.view_lock)?;
            let projection = ensure_local_projection(&workspace_root, &plan, &active)?;
            let environment =
                project_path_environment(&plan, &active.locked, &projection.projects)?;
            let cwd = external_run_directory(&active.local, &cwd)?;
            run_inherited_with_environment(
                Path::new(program),
                arguments,
                &cwd,
                &environment,
                "West external",
            )
        }
    }
}

fn west_plan_path(explicit: Option<PathBuf>) -> Result<PathBuf> {
    explicit
        .or_else(|| env::var_os("NIXSPACE_WEST_PLAN").map(PathBuf::from))
        .ok_or_else(|| {
            CliError(
                "West plan is unavailable; enter the Nix development shell, set NIXSPACE_WEST_PLAN, or pass --west-plan PATH"
                    .into(),
            )
        })
}

fn load_plan(path: &Path) -> Result<WestPlan> {
    let bytes = fs::read(path).map_err(|error| {
        CliError(format!(
            "Nix-generated West plan is unavailable at {}: {error}",
            path.display()
        ))
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        CliError(format!(
            "Nix-generated West plan at {} is unreadable: {error}",
            path.display()
        ))
    })
}

fn validate_plan(plan: &WestPlan, source: &Path) -> Result<()> {
    if plan.api_version != WEST_API_VERSION {
        return Err(CliError(format!(
            "West plan API `{}` is unsupported; nixspace requires `{WEST_API_VERSION}`",
            plan.api_version
        )));
    }
    if plan.kind != WEST_KIND {
        return Err(CliError(format!(
            "West plan kind `{}` is unsupported; nixspace requires `{WEST_KIND}`",
            plan.kind
        )));
    }
    if plan.interface_version != WEST_PLAN_INTERFACE_VERSION {
        return Err(CliError(format!(
            "West plan interface version {} is unsupported; nixspace supports version {}",
            plan.interface_version, WEST_PLAN_INTERFACE_VERSION
        )));
    }
    if plan.cache.layout_version != WEST_CACHE_LAYOUT_VERSION {
        return Err(CliError(format!(
            "West cache layout version {} is unsupported; nixspace supports version {}",
            plan.cache.layout_version, WEST_CACHE_LAYOUT_VERSION
        )));
    }
    if plan.cache.retained_generations == 0 {
        return Err(CliError(
            "West cache retention must keep at least one generation".into(),
        ));
    }
    if plan.product.interface_version == 0 {
        return Err(CliError(
            "West plan product interface version must be positive".into(),
        ));
    }
    for (label, value) in [
        ("product ID", plan.product.id.as_str()),
        ("workspace ID", plan.workspace.id.as_str()),
        ("cache namespace", plan.cache.namespace.as_str()),
    ] {
        if !valid_id(value) {
            return Err(CliError(format!("West plan {label} `{value}` is invalid")));
        }
    }
    if !valid_sha256(&plan.workspace.content_key) {
        return Err(CliError(
            "West plan content key must be a lowercase SHA-256 source-closure identity".into(),
        ));
    }
    if !safe_relative(Path::new(&plan.workspace.source.root))
        || !safe_relative(Path::new(&plan.workspace.manifest.relative_path))
        || contains_config_control(&plan.workspace.manifest.relative_path)
    {
        return Err(CliError(
            "West plan contains an unsafe source-root or manifest path".into(),
        ));
    }
    if plan.workspace.source.input.is_empty()
        || plan.workspace.source.identity.store_path.is_empty()
        || plan.workspace.manifest.resource.is_empty()
    {
        return Err(CliError(
            "West plan has an incomplete Nix source binding".into(),
        ));
    }
    if plan
        .workspace
        .source
        .identity
        .nar_hash
        .as_deref()
        .is_some_and(str::is_empty)
        || plan
            .workspace
            .source
            .identity
            .rev
            .as_deref()
            .is_some_and(str::is_empty)
    {
        return Err(CliError(
            "West plan has an empty locked source identity field".into(),
        ));
    }
    if !plan.workspace.manifest.store_path.is_absolute()
        || !plan.workspace.manifest.store_path.is_file()
    {
        return Err(CliError(format!(
            "Nix-selected West manifest {} from {} does not exist",
            plan.workspace.manifest.store_path.display(),
            source.display()
        )));
    }
    let manifest_repository =
        Path::new(&plan.workspace.source.identity.store_path).join(&plan.workspace.source.root);
    if !Path::new(&plan.workspace.source.identity.store_path).is_absolute()
        || !manifest_repository.is_dir()
    {
        return Err(CliError(format!(
            "Nix-selected West manifest repository does not exist: {}",
            manifest_repository.display()
        )));
    }
    let repository_manifest = manifest_repository.join(&plan.workspace.manifest.relative_path);
    if !repository_manifest.is_file() {
        return Err(CliError(format!(
            "Nix-selected West manifest repository is missing {}",
            repository_manifest.display()
        )));
    }
    if !plan.tools.west.is_absolute() || !plan.tools.west.is_file() {
        return Err(CliError(format!(
            "Nix-selected native West executable does not exist: {}",
            plan.tools.west.display()
        )));
    }
    validate_store_tools(&plan.tools.store)?;
    validate_project_path_environment(&plan.tools.project_path_environment)?;
    if !valid_sha256(&plan.local_view.policy_id) {
        return Err(CliError(
            "West plan local-view policy ID is not a lowercase SHA-256 value".into(),
        ));
    }
    if plan.cache.root.base != RootBase::PlatformCache
        || plan.local_view.root.base != RootBase::Workspace
        || !strict_safe_relative(&plan.cache.root.path)
        || !strict_safe_relative(&plan.local_view.root.path)
        || !strict_safe_relative(&plan.cache.paths.generations)
        || !strict_safe_relative(&plan.cache.paths.generation_gc_root)
        || plan.cache.paths.generation_gc_root.components().count() != 1
        || !strict_safe_relative(&plan.cache.paths.current)
        || !strict_safe_relative(&plan.cache.paths.publication_lock)
        || !strict_safe_relative(&plan.local_view.paths.generations)
        || !strict_safe_relative(&plan.local_view.paths.execution_lock)
        || !starts_with_component(&plan.cache.paths.generations, &plan.cache.namespace)
        || !starts_with_component(&plan.cache.paths.current, &plan.cache.namespace)
        || !starts_with_component(&plan.cache.paths.publication_lock, &plan.cache.namespace)
        || !starts_with_component(&plan.local_view.paths.generations, &plan.product.id)
        || !starts_with_component(&plan.local_view.paths.execution_lock, &plan.product.id)
    {
        return Err(CliError(
            "West plan cache.root must be a safe platform-cache-relative path and localView.root must be a safe workspace-relative path"
                .into(),
        ));
    }
    if paths_overlap(&plan.cache.paths.generations, &plan.cache.paths.current)
        || paths_overlap(
            &plan.cache.paths.generations,
            &plan.cache.paths.publication_lock,
        )
        || paths_overlap(
            &plan.cache.paths.current,
            &plan.cache.paths.publication_lock,
        )
        || paths_overlap(
            &plan.local_view.paths.generations,
            &plan.local_view.paths.execution_lock,
        )
    {
        return Err(CliError(
            "Nix-generated West persistent paths must not be equal or ancestor-overlapping".into(),
        ));
    }
    let mut projects = BTreeSet::new();
    for binding in &plan.local_view.overrides {
        if !valid_id(&binding.project) || !safe_relative(&binding.source) {
            return Err(CliError(format!(
                "West plan has an invalid local override for `{}`",
                binding.project
            )));
        }
        if !projects.insert(&binding.project) {
            return Err(CliError(format!(
                "West plan repeats local override `{}`",
                binding.project
            )));
        }
    }
    Ok(())
}

fn validate_project_path_environment(environment: &ProjectPathEnvironment) -> Result<()> {
    if environment.interface_version != PROJECT_PATH_ENVIRONMENT_INTERFACE_VERSION {
        return Err(CliError(format!(
            "West project-path environment interface version {} is unsupported; nixspace supports version {}",
            environment.interface_version, PROJECT_PATH_ENVIRONMENT_INTERFACE_VERSION
        )));
    }
    if !valid_environment_name(&environment.count_variable)
        || !valid_environment_name(&environment.key_variable_prefix)
        || !valid_environment_name(&environment.value_variable_prefix)
        || environment.key.is_empty()
        || environment.key.contains('\0')
        || environment
            .key_variable_prefix
            .eq_ignore_ascii_case(&environment.value_variable_prefix)
        || environment
            .count_variable
            .to_ascii_lowercase()
            .starts_with(&environment.key_variable_prefix.to_ascii_lowercase())
        || environment
            .count_variable
            .to_ascii_lowercase()
            .starts_with(&environment.value_variable_prefix.to_ascii_lowercase())
    {
        return Err(CliError(
            "Nix-generated West project-path environment contract is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_store_tools(tools: &StoreTools) -> Result<()> {
    if tools.interface_version != STORE_TOOL_INTERFACE_VERSION {
        return Err(CliError(format!(
            "West store-tool interface version {} is unsupported; nixspace supports version {}",
            tools.interface_version, STORE_TOOL_INTERFACE_VERSION
        )));
    }
    validate_tool_command(
        "store seal",
        &tools.seal,
        &[ToolParameter::Source, ToolParameter::GcRoot],
        ToolOutput::StorePath,
    )
}

fn validate_tool_command(
    label: &str,
    command: &ToolCommand,
    expected_parameters: &[ToolParameter],
    expected_output: ToolOutput,
) -> Result<()> {
    if command.output != expected_output || command.argv.is_empty() {
        return Err(CliError(format!(
            "Nix-generated West {label} command has an invalid output or empty argv contract"
        )));
    }
    let mut parameters = Vec::new();
    for argument in &command.argv {
        match (&argument.literal, argument.parameter) {
            (Some(literal), None) if !literal.is_empty() && !literal.contains('\0') => {}
            (None, Some(parameter)) => parameters.push(parameter),
            _ => {
                return Err(CliError(format!(
                    "Nix-generated West {label} argv entries must contain exactly one nonempty literal or typed parameter"
                )))
            }
        }
    }
    let executable = command.argv[0].literal.as_deref().ok_or_else(|| {
        CliError(format!(
            "Nix-generated West {label} argv must begin with a literal executable"
        ))
    })?;
    if !Path::new(executable).is_absolute() || !Path::new(executable).is_file() {
        return Err(CliError(format!(
            "Nix-generated West {label} executable does not exist: {executable}"
        )));
    }
    parameters.sort_unstable();
    let mut expected = expected_parameters.to_vec();
    expected.sort_unstable();
    if parameters != expected {
        return Err(CliError(format!(
            "Nix-generated West {label} command has the wrong typed parameter contract"
        )));
    }
    Ok(())
}

fn render_tool_command(
    command: &ToolCommand,
    parameters: &BTreeMap<ToolParameter, &Path>,
) -> Result<(PathBuf, Vec<OsString>)> {
    let mut rendered = Vec::with_capacity(command.argv.len());
    for argument in &command.argv {
        match (&argument.literal, argument.parameter) {
            (Some(literal), None) => rendered.push(OsString::from(literal)),
            (None, Some(parameter)) => {
                let value = parameters.get(&parameter).ok_or_else(|| {
                    CliError("Nix-generated West tool command is missing a typed parameter".into())
                })?;
                rendered.push(value.as_os_str().to_owned());
            }
            _ => {
                return Err(CliError(
                    "Nix-generated West tool command contains an invalid argv entry".into(),
                ))
            }
        }
    }
    let program = rendered
        .first()
        .map(PathBuf::from)
        .ok_or_else(|| CliError("Nix-generated West tool command has an empty argv".into()))?;
    Ok((program, rendered.into_iter().skip(1).collect()))
}

fn resolve_workspace_root(default_root: &Path, configured: &str) -> Result<PathBuf> {
    let root = env::var_os("NIXSPACE_WORKSPACE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let configured = PathBuf::from(configured);
            if configured.is_absolute() {
                configured
            } else {
                default_root.join(configured)
            }
        });
    absolute_path(&root)
}

fn west_paths(root: &Path, plan: &WestPlan) -> Result<WestPaths> {
    let cache_root = if let Some(configured) = env::var_os("NIXSPACE_WEST_CACHE") {
        absolute_path(Path::new(&configured))?
    } else {
        absolute_path(&platform_cache_root()?.join(&plan.cache.root.path))?
    };
    let configured_local_root = env::var_os("NIXSPACE_WEST_VIEW_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(&plan.local_view.root.path));
    let local_root = absolute_path(&configured_local_root)?;
    Ok(WestPaths {
        generations: cache_root.join(&plan.cache.paths.generations),
        generation_gc_root: plan.cache.paths.generation_gc_root.clone(),
        current: cache_root.join(&plan.cache.paths.current),
        lock: cache_root.join(&plan.cache.paths.publication_lock),
        local_generations: local_root.join(&plan.local_view.paths.generations),
        view_lock: local_root.join(&plan.local_view.paths.execution_lock),
    })
}

fn platform_cache_root() -> Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_CACHE_HOME") {
        return Ok(PathBuf::from(path));
    }
    if cfg!(target_os = "macos") {
        if let Some(home) = env::var_os("HOME") {
            return Ok(PathBuf::from(home).join("Library/Caches"));
        }
    }
    if cfg!(windows) {
        if let Some(path) = env::var_os("LOCALAPPDATA") {
            return Ok(PathBuf::from(path));
        }
    }
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .map(|home| home.join(".cache"))
        .ok_or_else(|| {
            CliError(
                "cannot select a West cache root; set NIXSPACE_WEST_CACHE or XDG_CACHE_HOME".into(),
            )
        })
}

fn materialize(root: &Path, plan: &WestPlan, paths: &WestPaths, force: bool) -> Result<()> {
    let _lease = PublicationLease::acquire_exclusive(&paths.lock)?;
    if !force && ready_workspace(plan, paths).is_some() {
        return Ok(());
    }
    publish_generation(root, plan, paths)?;
    Ok(())
}

fn garbage_collect<'a>(plan: &'a WestPlan, paths: &WestPaths) -> Result<WestGcReport<'a>> {
    let _lease = PublicationLease::acquire_exclusive(&paths.lock)?;
    let active = require_ready_workspace(plan, paths)?;
    let mut generations = generation_names(&paths.generations)?;
    generations.extend(generation_names(&paths.local_generations)?);
    generations.insert(active.generation.clone());

    let mut candidates = generations
        .into_iter()
        .filter(|generation| generation != &active.generation)
        .collect::<Vec<_>>();
    candidates.sort_by_key(|generation| generation_key(generation));
    candidates.reverse();

    // A foreign entry must not consume a retention slot and thereby cause an owned generation to
    // be removed. Prove the full candidate set before choosing either the kept or removed set.
    for generation in &candidates {
        validate_owned_generation(plan, paths, generation)?;
    }

    let additionally_retained = plan.cache.retained_generations.saturating_sub(1);
    let mut kept_generations = vec![active.generation.clone()];
    kept_generations.extend(candidates.iter().take(additionally_retained).cloned());
    let mut removed_generations = candidates
        .into_iter()
        .skip(additionally_retained)
        .collect::<Vec<_>>();
    removed_generations.sort_by_key(|generation| generation_key(generation));

    for generation in &removed_generations {
        remove_if_exists(&paths.local_generations.join(generation))?;
        remove_if_exists(&paths.generations.join(generation))?;
    }

    Ok(WestGcReport {
        interface_version: WEST_PLAN_INTERFACE_VERSION,
        product: &plan.product.id,
        workspace: &plan.workspace.id,
        current_generation: active.generation,
        retained_generations: plan.cache.retained_generations,
        kept_generations,
        removed_generations,
    })
}

fn generation_names(root: &Path) -> Result<BTreeSet<String>> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(error) => {
            return Err(CliError(format!(
                "cannot inspect West generation root {}: {error}",
                root.display()
            )))
        }
    };
    let mut generations = BTreeSet::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            CliError(format!(
                "cannot inspect an entry in West generation root {}: {error}",
                root.display()
            ))
        })?;
        if let Some(name) = entry
            .file_name()
            .to_str()
            .filter(|name| valid_generation(name))
        {
            generations.insert(name.to_owned());
        }
    }
    Ok(generations)
}

fn validate_owned_generation(plan: &WestPlan, paths: &WestPaths, generation: &str) -> Result<()> {
    let published = paths.generations.join(generation);
    let local = paths.local_generations.join(generation);
    let published_metadata = optional_metadata(&published)?;
    let local_metadata = optional_metadata(&local)?;
    if published_metadata.is_none() && local_metadata.is_none() {
        return Err(CliError(format!(
            "West generation `{generation}` disappeared before garbage collection"
        )));
    }

    if let Some(metadata) = published_metadata {
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(not_owned_generation(generation, &published));
        }
        let gc_root = published.join(&paths.generation_gc_root);
        let metadata = optional_metadata(&gc_root)?
            .filter(|metadata| metadata.file_type().is_symlink())
            .ok_or_else(|| not_owned_generation(generation, &published))?;
        debug_assert!(metadata.file_type().is_symlink());
        let locked =
            fs::canonicalize(&gc_root).map_err(|_| not_owned_generation(generation, &published))?;
        if !locked_ready(plan, &locked) {
            return Err(not_owned_generation(generation, &published));
        }
    }
    if let Some(metadata) = local_metadata {
        if !metadata.is_dir() || metadata.file_type().is_symlink() || !local_ready(plan, &local) {
            return Err(not_owned_generation(generation, &local));
        }
    }
    Ok(())
}

fn optional_metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CliError(format!(
            "cannot inspect West generation path {}: {error}",
            path.display()
        ))),
    }
}

fn not_owned_generation(generation: &str, path: &Path) -> CliError {
    CliError(format!(
        "refusing to remove West generation `{generation}` because {} is not owned by this exact Nix plan",
        path.display()
    ))
}

fn publish_generation(root: &Path, plan: &WestPlan, paths: &WestPaths) -> Result<ActiveWorkspace> {
    fs::create_dir_all(&paths.generations).map_err(|error| {
        CliError(format!(
            "cannot create immutable West generation root {}: {error}",
            paths.generations.display()
        ))
    })?;
    fs::create_dir_all(&paths.local_generations).map_err(|error| {
        CliError(format!(
            "cannot create local West generation root {}: {error}",
            paths.local_generations.display()
        ))
    })?;

    let (generation, staged_generation) = create_generation_staging(&paths.generations)?;
    let staged_locked = staged_generation.join("checkout");
    let seed = path_cache_seed(plan, paths);
    if let Err(error) = initialize_locked(plan, &staged_locked, seed.as_deref()) {
        let _ = remove_if_exists(&staged_generation);
        return Err(error);
    }

    let published_generation = paths.generations.join(&generation);
    if let Err(error) = fs::create_dir(&published_generation) {
        let _ = remove_if_exists(&staged_generation);
        return Err(CliError(format!(
            "cannot reserve immutable West generation {}: {error}",
            published_generation.display()
        )));
    }
    let gc_root = published_generation.join(&paths.generation_gc_root);
    let sealed = match seal_checkout(plan, &staged_locked, &gc_root) {
        Ok(path) => path,
        Err(error) => {
            let _ = remove_if_exists(&staged_generation);
            let _ = remove_if_exists(&published_generation);
            return Err(error);
        }
    };
    if let Err(error) = remove_if_exists(&staged_generation) {
        let _ = remove_if_exists(&published_generation);
        return Err(error);
    }
    if !fs::symlink_metadata(&gc_root).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        let _ = remove_if_exists(&published_generation);
        return Err(CliError(format!(
            "Nix store seal command did not create a GC-root link at {}",
            gc_root.display()
        )));
    }
    let retained = match fs::canonicalize(&gc_root) {
        Ok(path) if path == sealed => path,
        Ok(path) => {
            let _ = remove_if_exists(&published_generation);
            return Err(CliError(format!(
                "Nix store seal command linked {} to {}, expected {}",
                gc_root.display(),
                path.display(),
                sealed.display()
            )));
        }
        Err(error) => {
            let _ = remove_if_exists(&published_generation);
            return Err(CliError(format!(
                "Nix store seal command did not create GC root {}: {error}",
                gc_root.display()
            )));
        }
    };
    if !locked_ready(plan, &retained) {
        let _ = remove_if_exists(&published_generation);
        return Err(CliError(format!(
            "Nix store ingestion did not preserve the validated West checkout at {}",
            retained.display()
        )));
    }
    let local = paths.local_generations.join(&generation);
    if let Err(error) = create_local_view(root, plan, &retained, &local) {
        let _ = remove_if_exists(&published_generation);
        return Err(error);
    }
    let pointer = CurrentGeneration {
        interface_version: WEST_PLAN_INTERFACE_VERSION,
        generation: generation.clone(),
    };
    if let Err(error) = write_json_file(&paths.current, &pointer) {
        let _ = remove_if_exists(&local);
        let _ = remove_if_exists(&published_generation);
        return Err(error);
    }
    Ok(ActiveWorkspace {
        generation,
        locked: retained,
        local,
    })
}

fn seal_checkout(plan: &WestPlan, source: &Path, gc_root: &Path) -> Result<PathBuf> {
    let rendered = render_tool_command(
        &plan.tools.store.seal,
        &BTreeMap::from([
            (ToolParameter::Source, source),
            (ToolParameter::GcRoot, gc_root),
        ]),
    )?;
    let output = run_exact_captured(&rendered.0, &rendered.1, source, "Nix store ingestion")?;
    let text = String::from_utf8(output.stdout).map_err(|error| {
        CliError(format!(
            "Nix store ingestion emitted non-UTF-8 output: {error}"
        ))
    })?;
    let lines = text
        .lines()
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.len() != 1 {
        return Err(CliError(format!(
            "Nix store ingestion must emit exactly one store path, received {} lines",
            lines.len()
        )));
    }
    let path = PathBuf::from(lines[0]);
    if !path.is_absolute() || !path.is_dir() {
        return Err(CliError(format!(
            "Nix store ingestion returned an unavailable absolute directory: {}",
            path.display()
        )));
    }
    fs::canonicalize(&path).map_err(|error| {
        CliError(format!(
            "cannot canonicalize Nix-ingested West checkout {}: {error}",
            path.display()
        ))
    })
}

fn create_generation_staging(generations: &Path) -> Result<(String, PathBuf)> {
    for _ in 0..128 {
        let elapsed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let sequence = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
        let generation = format!("{elapsed:x}-{:x}-{sequence:x}", std::process::id());
        let staged = generations.join(format!(".tmp-{generation}"));
        match fs::create_dir(&staged) {
            Ok(()) => return Ok((generation, staged)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(CliError(format!(
                    "cannot stage immutable West generation {}: {error}",
                    staged.display()
                )))
            }
        }
    }
    Err(CliError(
        "cannot allocate a unique immutable West generation".into(),
    ))
}

fn initialize_locked(plan: &WestPlan, locked: &Path, path_cache: Option<&Path>) -> Result<()> {
    fs::create_dir(locked).map_err(|error| {
        CliError(format!(
            "cannot create staged immutable West workspace {}: {error}",
            locked.display()
        ))
    })?;
    let manifest = locked.join("manifest");
    let manifest_source =
        Path::new(&plan.workspace.source.identity.store_path).join(&plan.workspace.source.root);
    symlink_directory(&manifest_source, &manifest)?;
    write_west_config(locked, "manifest", &plan.workspace.manifest.relative_path)?;

    let arguments = west_update_arguments(plan.cache.narrow_update, path_cache);
    let environment = if let Some(seed) = path_cache {
        let projects = resolved_project_paths(plan, seed)?
            .into_iter()
            .map(|(name, relative)| LocalProject {
                name,
                source: seed.join(&relative),
                relative,
            })
            .collect::<Vec<_>>();
        project_path_environment(plan, seed, &projects)?
    } else {
        Vec::new()
    };
    run_inherited_with_environment(
        &plan.tools.west,
        &arguments,
        locked,
        &environment,
        "Nix-selected native West",
    )?;
    if !native_checkout_ready(plan, locked) {
        return Err(CliError(format!(
            "native West did not produce a valid immutable checkout at {}",
            locked.display()
        )));
    }

    let marker = LockedMarker {
        interface_version: WEST_PLAN_INTERFACE_VERSION,
        product: plan.product.id.clone(),
        workspace: plan.workspace.id.clone(),
        content_key: plan.workspace.content_key.clone(),
        manifest_resource: plan.workspace.manifest.resource.clone(),
        manifest_sha256: plan.workspace.manifest.sha256.clone(),
    };
    write_json_file(&locked.join(".nixspace-west.json"), &marker)
}

fn create_local_view(root: &Path, plan: &WestPlan, locked: &Path, local: &Path) -> Result<()> {
    let projection = resolve_local_projection(root, plan, locked)?;
    install_local_view(plan, locked, local, &projection)
}

fn resolve_local_projection(
    root: &Path,
    plan: &WestPlan,
    locked: &Path,
) -> Result<LocalProjection> {
    let resolved = resolved_project_paths(plan, locked)?;
    let overrides: BTreeMap<_, _> = plan
        .local_view
        .overrides
        .iter()
        .map(|binding| (binding.project.as_str(), binding))
        .collect();
    let mut projects = Vec::new();
    let mut effective_overrides = BTreeMap::new();
    let mut project_markers = BTreeMap::new();
    let mut matched_overrides = BTreeSet::new();
    for (name, relative) in resolved {
        let immutable = locked.join(&relative);
        let source = if let Some(binding) = overrides.get(name.as_str()) {
            matched_overrides.insert(name.clone());
            let editable = root.join(&binding.source);
            if editable.is_dir() {
                effective_overrides.insert(name.clone(), editable.display().to_string());
                editable
            } else if binding.required {
                return Err(CliError(format!(
                    "required editable West override `{name}` is missing at {}",
                    editable.display()
                )));
            } else {
                immutable
            }
        } else {
            immutable
        };
        if !source.is_dir() {
            return Err(CliError(format!(
                "native West resolved project `{name}` to missing directory {}",
                source.display()
            )));
        }
        project_markers.insert(
            name.clone(),
            LocalProjectMarker {
                path: relative.display().to_string(),
                source: source.display().to_string(),
            },
        );
        projects.push(LocalProject {
            name,
            relative,
            source,
        });
    }
    let unmatched: Vec<_> = overrides
        .keys()
        .filter(|name| !matched_overrides.contains(**name))
        .copied()
        .collect();
    if !unmatched.is_empty() {
        return Err(CliError(format!(
            "Nix-declared West overrides do not match native West projects: {}",
            unmatched.join(", ")
        )));
    }
    Ok(LocalProjection {
        projects,
        overrides: effective_overrides,
        markers: project_markers,
    })
}

fn project_path_environment(
    plan: &WestPlan,
    locked: &Path,
    projects: &[LocalProject],
) -> Result<Vec<(OsString, OsString)>> {
    let locked = fs::canonicalize(locked).map_err(|error| {
        CliError(format!(
            "cannot resolve sealed West checkout {} for its Nix-declared command environment: {error}",
            locked.display()
        ))
    })?;
    let mut trusted = BTreeSet::new();
    for project in projects {
        let source = fs::canonicalize(&project.source).map_err(|error| {
            CliError(format!(
                "cannot resolve West project `{}` at {} for its Nix-declared command environment: {error}",
                project.name,
                project.source.display()
            ))
        })?;
        if source.starts_with(&locked) {
            trusted.insert(source.clone());
            let metadata = source.join(".git");
            if metadata.is_dir() {
                trusted.insert(metadata);
            }
        }
    }
    let contract = &plan.tools.project_path_environment;
    let mut environment = Vec::with_capacity(1 + trusted.len() * 2);
    environment.push((
        OsString::from(&contract.count_variable),
        OsString::from(trusted.len().to_string()),
    ));
    for (index, path) in trusted.into_iter().enumerate() {
        environment.push((
            OsString::from(format!("{}{index}", contract.key_variable_prefix)),
            OsString::from(&contract.key),
        ));
        environment.push((
            OsString::from(format!("{}{index}", contract.value_variable_prefix)),
            path.into_os_string(),
        ));
    }
    Ok(environment)
}

fn install_local_view(
    plan: &WestPlan,
    locked: &Path,
    local: &Path,
    projection: &LocalProjection,
) -> Result<()> {
    let parent = local
        .parent()
        .expect("local generation always has a parent");
    fs::create_dir_all(parent)
        .map_err(|error| CliError(format!("cannot create local West view parent: {error}")))?;
    let temporary = parent.join(format!(
        ".{}.tmp-{}-{}",
        local
            .file_name()
            .expect("generation has a file name")
            .to_string_lossy(),
        std::process::id(),
        NEXT_GENERATION.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&temporary)
        .map_err(|error| CliError(format!("cannot create temporary local West view: {error}")))?;

    let result = (|| -> Result<()> {
        for project in &projection.projects {
            let destination = temporary.join(&project.relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    CliError(format!("cannot create local West project parent: {error}"))
                })?;
            }
            symlink_directory(&project.source, &destination).map_err(|error| {
                CliError(format!(
                    "cannot project native West project `{}`: {}",
                    project.name, error.0
                ))
            })?;
        }
        if !temporary.join("manifest").exists() {
            symlink_directory(&locked.join("manifest"), &temporary.join("manifest"))?;
        }
        write_west_config(
            &temporary,
            "manifest",
            &plan.workspace.manifest.relative_path,
        )?;
        let marker = LocalMarker {
            interface_version: WEST_PLAN_INTERFACE_VERSION,
            product: plan.product.id.clone(),
            workspace: plan.workspace.id.clone(),
            content_key: plan.workspace.content_key.clone(),
            manifest_sha256: plan.workspace.manifest.sha256.clone(),
            policy_id: plan.local_view.policy_id.clone(),
            overrides: projection.overrides.clone(),
            projects: projection.markers.clone(),
        };
        write_json_file(&temporary.join(".nixspace-west-local.json"), &marker)?;
        fs::rename(&temporary, local).map_err(|error| {
            CliError(format!(
                "cannot install local West view {}: {error}",
                local.display()
            ))
        })
    })();
    if result.is_err() {
        let _ = remove_if_exists(&temporary);
    }
    result
}

fn resolved_project_paths(plan: &WestPlan, locked: &Path) -> Result<Vec<(String, PathBuf)>> {
    let output = run_captured(
        &plan.tools.west,
        &["list".into(), "-f".into(), "{name}|{path}".into()],
        locked,
    )?;
    let text = String::from_utf8(output.stdout).map_err(|error| {
        CliError(format!(
            "native West emitted non-UTF-8 project paths: {error}"
        ))
    })?;
    parse_project_paths(&text)
}

fn native_checkout_ready(plan: &WestPlan, locked: &Path) -> bool {
    run_captured(
        &plan.tools.west,
        &["manifest".into(), "--freeze".into()],
        locked,
    )
    .is_ok()
}

fn parse_project_paths(text: &str) -> Result<Vec<(String, PathBuf)>> {
    let mut result = Vec::new();
    let mut names = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for line in text.lines().filter(|line| !line.is_empty()) {
        let (name, path) = line.split_once('|').ok_or_else(|| {
            CliError(format!(
                "native West returned an invalid project record: {line}"
            ))
        })?;
        let path = PathBuf::from(path);
        if name.is_empty() || !safe_project_path(&path) {
            return Err(CliError(format!(
                "native West returned an unsafe project record: {line}"
            )));
        }
        if !names.insert(name.to_owned())
            || paths
                .iter()
                .any(|existing: &PathBuf| existing.starts_with(&path) || path.starts_with(existing))
            || !paths.insert(path.clone())
        {
            return Err(CliError(format!(
                "native West returned a duplicate project name or path: {line}"
            )));
        }
        result.push((name.to_owned(), path));
    }
    if result.is_empty() {
        return Err(CliError("native West returned no resolved projects".into()));
    }
    Ok(result)
}

fn extra_modules(root: &Path, plan: &WestPlan) -> Result<Vec<PathBuf>> {
    let mut modules = Vec::new();
    for binding in &plan.local_view.overrides {
        if !binding.zephyr_module {
            continue;
        }
        let source = root.join(&binding.source);
        if source.is_dir() {
            modules.push(absolute_path(&source)?);
        } else if binding.required {
            return Err(CliError(format!(
                "required editable Zephyr module `{}` is missing at {}",
                binding.project,
                source.display()
            )));
        }
    }
    Ok(modules)
}

fn locked_ready(plan: &WestPlan, locked: &Path) -> bool {
    read_json_file::<LockedMarker>(&locked.join(".nixspace-west.json")).is_some_and(|marker| {
        marker.interface_version == WEST_PLAN_INTERFACE_VERSION
            && marker.product == plan.product.id
            && marker.workspace == plan.workspace.id
            && marker.content_key == plan.workspace.content_key
            && marker.manifest_resource == plan.workspace.manifest.resource
            && marker.manifest_sha256 == plan.workspace.manifest.sha256
            && locked.join(".west/config").is_file()
    })
}

fn local_ready(plan: &WestPlan, local: &Path) -> bool {
    read_json_file::<LocalMarker>(&local.join(".nixspace-west-local.json")).is_some_and(|marker| {
        marker.interface_version == WEST_PLAN_INTERFACE_VERSION
            && marker.product == plan.product.id
            && marker.workspace == plan.workspace.id
            && marker.content_key == plan.workspace.content_key
            && marker.manifest_sha256 == plan.workspace.manifest.sha256
            && marker.policy_id == plan.local_view.policy_id
            && local.join(".west/config").is_file()
    })
}

fn local_projection_ready(
    root: &Path,
    plan: &WestPlan,
    locked: &Path,
    local: &Path,
) -> Result<bool> {
    let projection = resolve_local_projection(root, plan, locked)?;
    local_projection_matches(plan, locked, local, &projection)
}

fn local_projection_matches(
    plan: &WestPlan,
    locked: &Path,
    local: &Path,
    projection: &LocalProjection,
) -> Result<bool> {
    let marker = match read_json_file::<LocalMarker>(&local.join(".nixspace-west-local.json")) {
        Some(marker) => marker,
        None => return Ok(false),
    };
    if marker.interface_version != WEST_PLAN_INTERFACE_VERSION
        || marker.product != plan.product.id
        || marker.workspace != plan.workspace.id
        || marker.content_key != plan.workspace.content_key
        || marker.manifest_sha256 != plan.workspace.manifest.sha256
        || marker.policy_id != plan.local_view.policy_id
        || marker.overrides != projection.overrides
        || marker.projects != projection.markers
    {
        return Ok(false);
    }
    let expected_config = west_config_contents("manifest", &plan.workspace.manifest.relative_path)?;
    if fs::read(local.join(".west/config")).ok().as_deref() != Some(expected_config.as_bytes()) {
        return Ok(false);
    }
    let projects_manifest = projection
        .projects
        .iter()
        .any(|project| project.relative == Path::new("manifest"));
    for project in &projection.projects {
        if fs::read_link(local.join(&project.relative)).ok().as_deref()
            != Some(project.source.as_path())
        {
            return Ok(false);
        }
    }
    if !projects_manifest
        && fs::read_link(local.join("manifest")).ok().as_deref()
            != Some(locked.join("manifest").as_path())
    {
        return Ok(false);
    }
    Ok(true)
}

fn ensure_local_projection(
    root: &Path,
    plan: &WestPlan,
    active: &ActiveWorkspace,
) -> Result<LocalProjection> {
    let projection = resolve_local_projection(root, plan, &active.locked)?;
    if local_projection_matches(plan, &active.locked, &active.local, &projection)? {
        return Ok(projection);
    }
    remove_if_exists(&active.local)?;
    install_local_view(plan, &active.locked, &active.local, &projection)?;
    Ok(projection)
}

fn selected_workspace(paths: &WestPaths) -> Option<ActiveWorkspace> {
    let pointer = read_json_file::<CurrentGeneration>(&paths.current)?;
    if pointer.interface_version != WEST_PLAN_INTERFACE_VERSION
        || !valid_generation(&pointer.generation)
    {
        return None;
    }
    Some(workspace_for_generation(paths, pointer.generation))
}

fn workspace_for_generation(paths: &WestPaths, generation: String) -> ActiveWorkspace {
    let gc_root = paths
        .generations
        .join(&generation)
        .join(&paths.generation_gc_root);
    ActiveWorkspace {
        locked: fs::canonicalize(&gc_root).unwrap_or(gc_root),
        local: paths.local_generations.join(&generation),
        generation,
    }
}

fn ready_workspace(plan: &WestPlan, paths: &WestPaths) -> Option<ActiveWorkspace> {
    selected_workspace(paths)
        .filter(|active| locked_ready(plan, &active.locked) && local_ready(plan, &active.local))
}

fn require_ready_workspace(plan: &WestPlan, paths: &WestPaths) -> Result<ActiveWorkspace> {
    ready_workspace(plan, paths).ok_or_else(|| {
        CliError(
            "published West generation is unavailable or invalid; run `nixspace west ensure`"
                .into(),
        )
    })
}

fn selected_workspace_path(
    root: &Path,
    plan: &WestPlan,
    paths: &WestPaths,
    mode: WestMode,
) -> Result<PathBuf> {
    let _publication_lease = PublicationLease::acquire_shared(&paths.lock)?;
    let active = ready_workspace(plan, paths).ok_or_else(|| {
        CliError("West workspace is not materialized; run `nixspace west ensure`".into())
    })?;
    match mode {
        WestMode::Release => Ok(active.locked),
        WestMode::Local => {
            let _view_lease = ViewLease::acquire(&paths.view_lock)?;
            ensure_local_projection(root, plan, &active)?;
            Ok(active.local)
        }
    }
}

fn status_document<'a>(root: &Path, plan: &'a WestPlan, paths: &'a WestPaths) -> WestStatus<'a> {
    let active = selected_workspace(paths);
    let ready = active.as_ref().is_some_and(|active| {
        locked_ready(plan, &active.locked)
            && local_ready(plan, &active.local)
            && local_projection_ready(root, plan, &active.locked, &active.local).unwrap_or(false)
    });
    let locked = active
        .as_ref()
        .map(|active| active.locked.clone())
        .unwrap_or_else(|| paths.generations.clone());
    let local = active
        .as_ref()
        .map(|active| active.local.clone())
        .unwrap_or_else(|| paths.local_generations.clone());
    WestStatus {
        interface_version: WEST_PLAN_INTERFACE_VERSION,
        product: &plan.product.id,
        workspace: &plan.workspace.id,
        manifest_resource: &plan.workspace.manifest.resource,
        content_key: &plan.workspace.content_key,
        generation: active.as_ref().map(|active| active.generation.clone()),
        path_cache_seed: path_cache_seed(plan, paths),
        locked,
        local,
        ready,
    }
}

fn emit_status(root: &Path, plan: &WestPlan, paths: &WestPaths, json: bool) -> Result<()> {
    let _lease = if paths.lock.is_file() {
        Some(PublicationLease::acquire_shared(&paths.lock)?)
    } else {
        None
    };
    let status = status_document(root, plan, paths);
    if json {
        return super::write_json(&status);
    }
    println!("product:        {}", status.product);
    println!("workspace:      {}", status.workspace);
    println!("manifest:       {}", status.manifest_resource);
    println!("content:        {}", status.content_key);
    println!(
        "generation:     {}",
        status.generation.as_deref().unwrap_or("none")
    );
    println!("locked product: {}", status.locked.display());
    println!("local product:  {}", status.local.display());
    println!(
        "path cache:     {}",
        status
            .path_cache_seed
            .as_deref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none yet".into())
    );
    println!(
        "ready:          {}",
        if status.ready { "yes" } else { "no" }
    );
    Ok(())
}

fn path_cache_seed(plan: &WestPlan, paths: &WestPaths) -> Option<PathBuf> {
    if !plan.cache.native_path_cache {
        return None;
    }
    selected_workspace(paths)
        .map(|active| active.locked)
        .filter(|locked| locked_ready(plan, locked))
}

fn west_update_arguments(narrow: bool, path_cache: Option<&Path>) -> Vec<String> {
    let mut arguments = vec!["update".into()];
    if narrow {
        arguments.push("--narrow".into());
    }
    if let Some(path) = path_cache {
        arguments.push("--path-cache".into());
        arguments.push(path.display().to_string());
    }
    arguments
}

fn write_west_config(workspace: &Path, manifest_path: &str, manifest_file: &str) -> Result<()> {
    let contents = west_config_contents(manifest_path, manifest_file)?;
    let west = workspace.join(".west");
    fs::create_dir_all(&west)
        .map_err(|error| CliError(format!("cannot create local West configuration: {error}")))?;
    fs::write(west.join("config"), contents)
        .map_err(|error| CliError(format!("cannot write local West configuration: {error}")))
}

fn west_config_contents(manifest_path: &str, manifest_file: &str) -> Result<String> {
    if contains_config_control(manifest_path) || contains_config_control(manifest_file) {
        return Err(CliError(
            "West manifest configuration contains a forbidden control character".into(),
        ));
    }
    Ok(format!(
        "[manifest]\npath = {manifest_path}\nfile = {manifest_file}\n\n[zephyr]\nbase = zephyr\n"
    ))
}

fn contains_config_control(value: &str) -> bool {
    value.bytes().any(|byte| matches!(byte, b'\n' | b'\r' | 0))
}

fn run_inherited_with_environment(
    program: &Path,
    arguments: &[String],
    cwd: &Path,
    environment: &[(OsString, OsString)],
    purpose: &str,
) -> Result<()> {
    let status = Command::new(program)
        .args(arguments)
        .envs(environment.iter().cloned())
        .current_dir(cwd)
        .status()
        .map_err(|error| {
            CliError(format!(
                "cannot invoke {purpose} command {}: {error}",
                program.display()
            ))
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError(format!(
            "{purpose} command failed with {}: {} {}",
            status,
            program.display(),
            arguments.join(" ")
        )))
    }
}

fn run_captured(program: &Path, arguments: &[String], cwd: &Path) -> Result<Output> {
    let output = Command::new(program)
        .args(arguments)
        .current_dir(cwd)
        .output()
        .map_err(|error| {
            CliError(format!(
                "cannot invoke Nix-selected native West {}: {error}",
                program.display()
            ))
        })?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(CliError(format!(
            "native West command failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn run_exact_captured(
    program: &Path,
    arguments: &[OsString],
    cwd: &Path,
    purpose: &str,
) -> Result<Output> {
    let output = Command::new(program)
        .args(arguments)
        .current_dir(cwd)
        .output()
        .map_err(|error| {
            CliError(format!(
                "cannot invoke Nix-generated {purpose} command {}: {error}",
                program.display()
            ))
        })?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(CliError(format!(
            "Nix-generated {purpose} command failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn write_json_file(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| CliError(format!("cannot serialize West state marker: {error}")))?;
    let sequence = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_extension(format!("tmp-{}-{sequence}", std::process::id()));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| CliError(format!("cannot create West state marker: {error}")))?;
        file.write_all(&[bytes.as_slice(), b"\n"].concat())
            .and_then(|()| file.sync_all())
            .map_err(|error| CliError(format!("cannot write West state marker: {error}")))?;
        drop(file);
        fs::rename(&temporary, path)
            .map_err(|error| CliError(format!("cannot install West state marker: {error}")))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(path)
        }
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(CliError(format!(
                "cannot inspect {}: {error}",
                path.display()
            )))
        }
    }
    .map_err(|error| CliError(format!("cannot remove {}: {error}", path.display())))
}

fn symlink_directory(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(unix)]
    let result = std::os::unix::fs::symlink(source, destination);
    #[cfg(windows)]
    let result = std::os::windows::fs::symlink_dir(source, destination);
    result.map_err(|error| {
        CliError(format!(
            "cannot link West directory {} -> {}: {error}",
            destination.display(),
            source.display()
        ))
    })
}

fn safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn safe_project_path(path: &Path) -> bool {
    if !safe_relative(path) {
        return false;
    }
    let first = path.components().find_map(|component| match component {
        Component::Normal(value) => Some(value),
        _ => None,
    });
    match first.and_then(|value| value.to_str()) {
        Some(".west" | ".nixspace-west-local.json") | None => false,
        Some("manifest") => path == Path::new("manifest"),
        Some(_) => true,
    }
}

fn external_run_directory(local_view: &Path, relative: &Path) -> Result<PathBuf> {
    if !safe_relative(relative) {
        return Err(CliError(format!(
            "West external command working directory must be a safe local-view-relative path: {}",
            relative.display()
        )));
    }
    let directory = local_view.join(relative);
    if !directory.is_dir() {
        return Err(CliError(format!(
            "West external command working directory does not exist: {}",
            directory.display()
        )));
    }
    Ok(directory)
}

fn strict_safe_relative(path: &Path) -> bool {
    let rendered = path.to_string_lossy();
    !rendered.is_empty()
        && !rendered.contains('\\')
        && !rendered
            .split('/')
            .next()
            .is_some_and(|segment| segment.ends_with(':'))
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn starts_with_component(path: &Path, expected: &str) -> bool {
    path.components()
        .next()
        .is_some_and(|component| matches!(component, Component::Normal(value) if value == expected))
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

fn valid_id(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_lowercase())
        && characters.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '_' | '-')
        })
}

fn valid_environment_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn valid_generation(value: &str) -> bool {
    generation_key(value).is_some()
}

fn generation_key(value: &str) -> Option<(u128, u128, u128)> {
    if value.is_empty() || value.len() > 128 {
        return None;
    }
    let mut components = value.split('-');
    let elapsed = u128::from_str_radix(components.next()?, 16).ok()?;
    let process = u128::from_str_radix(components.next()?, 16).ok()?;
    let sequence = u128::from_str_radix(components.next()?, 16).ok()?;
    if components.next().is_some() {
        return None;
    }
    Some((elapsed, process, sequence))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|error| CliError(format!("cannot resolve {}: {error}", path.display())))
    }
}

struct PublicationLease(File);

impl PublicationLease {
    fn open(path: &Path, create: bool) -> Result<File> {
        if create {
            fs::create_dir_all(
                path.parent()
                    .expect("content-addressed lock always has a parent"),
            )
            .map_err(|error| CliError(format!("cannot create West lock directory: {error}")))?;
        }
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(create)
            .truncate(false)
            .open(path)
            .map_err(|error| {
                CliError(format!(
                    "cannot open West publication lock {}: {error}",
                    path.display()
                ))
            })
    }

    fn acquire_exclusive(path: &Path) -> Result<Self> {
        let file = Self::open(path, true)?;
        FileExt::lock(&file).map_err(|error| {
            CliError(format!(
                "cannot acquire exclusive West publication lease {}: {error}",
                path.display()
            ))
        })?;
        Ok(Self(file))
    }

    fn acquire_shared(path: &Path) -> Result<Self> {
        let file = Self::open(path, false)?;
        FileExt::lock_shared(&file).map_err(|error| {
            CliError(format!(
                "cannot acquire shared West generation lease {}: {error}",
                path.display()
            ))
        })?;
        Ok(Self(file))
    }
}

impl Drop for PublicationLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

struct ViewLease(File);

impl ViewLease {
    fn acquire(path: &Path) -> Result<Self> {
        fs::create_dir_all(
            path.parent()
                .expect("local West view lock always has a parent"),
        )
        .map_err(|error| CliError(format!("cannot create West view lock directory: {error}")))?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|error| {
                CliError(format!(
                    "cannot open West view execution lock {}: {error}",
                    path.display()
                ))
            })?;
        FileExt::lock(&file).map_err(|error| {
            CliError(format!(
                "cannot acquire exclusive West view execution lease {}: {error}",
                path.display()
            ))
        })?;
        Ok(Self(file))
    }
}

impl Drop for ViewLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_west_records_supply_paths_without_a_manifest_model() {
        let projects = parse_project_paths(
            "manifest|manifest\nzephyr|zephyr\ntelemetry|modules/lib/telemetry\n",
        )
        .expect("native west records are valid");
        assert_eq!(
            projects,
            vec![
                ("manifest".into(), PathBuf::from("manifest")),
                ("zephyr".into(), PathBuf::from("zephyr")),
                ("telemetry".into(), PathBuf::from("modules/lib/telemetry")),
            ]
        );
    }

    #[test]
    fn native_west_cannot_escape_or_alias_a_local_view() {
        for invalid in [
            "telemetry|../outside\n",
            "telemetry|/outside\n",
            "telemetry|.west/config\n",
            "telemetry|.nixspace-west-local.json\n",
            "telemetry|manifest/nested\n",
            "telemetry|modules/lib/telemetry\ntelemetry|other\n",
            "telemetry|modules/lib/telemetry\nother|modules/lib/telemetry\n",
            "telemetry|modules/lib\nother|modules/lib/other\n",
        ] {
            assert!(
                parse_project_paths(invalid).is_err(),
                "accepted {invalid:?}"
            );
        }
    }

    #[test]
    fn stale_lock_file_does_not_block_a_new_publisher() {
        let base = env::temp_dir().join(format!("nixspace-west-stale-lock-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let lock = base.join("publication.lock");
        fs::write(&lock, "stale process metadata\n").unwrap();

        let first = PublicationLease::acquire_exclusive(&lock).unwrap();
        drop(first);
        let second = PublicationLease::acquire_exclusive(&lock).unwrap();
        drop(second);

        assert!(lock.is_file(), "advisory lock identity remains stable");
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn shared_generation_lease_excludes_publication_until_reader_releases() {
        let base =
            env::temp_dir().join(format!("nixspace-west-reader-lease-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let lock = base.join("publication.lock");
        fs::write(&lock, []).unwrap();

        let reader = PublicationLease::acquire_shared(&lock).unwrap();
        let writer = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock)
            .unwrap();
        assert!(FileExt::try_lock(&writer).is_err());
        drop(reader);
        FileExt::try_lock(&writer).expect("publication proceeds after reader exits");
        FileExt::unlock(&writer).unwrap();

        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn exclusive_view_lease_serializes_arbitrary_commands() {
        let base = env::temp_dir().join(format!("nixspace-west-view-lease-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let lock = base.join("execution.lock");

        let first = ViewLease::acquire(&lock).unwrap();
        let contender = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock)
            .unwrap();
        assert!(FileExt::try_lock(&contender).is_err());
        drop(first);
        FileExt::try_lock(&contender).expect("next local-view command proceeds after release");
        FileExt::unlock(&contender).unwrap();

        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn typed_store_argv_renders_only_nix_declared_literals_and_paths() {
        let executable = env::current_exe().unwrap();
        let command = ToolCommand {
            argv: vec![
                ToolArgument {
                    literal: Some(executable.display().to_string()),
                    parameter: None,
                },
                ToolArgument {
                    literal: Some("store".into()),
                    parameter: None,
                },
                ToolArgument {
                    literal: None,
                    parameter: Some(ToolParameter::Source),
                },
                ToolArgument {
                    literal: None,
                    parameter: Some(ToolParameter::GcRoot),
                },
            ],
            output: ToolOutput::StorePath,
        };
        validate_tool_command(
            "store seal",
            &command,
            &[ToolParameter::Source, ToolParameter::GcRoot],
            ToolOutput::StorePath,
        )
        .unwrap();
        let source = Path::new("/typed/source");
        let gc_root = Path::new("/typed/root");
        let (program, arguments) = render_tool_command(
            &command,
            &BTreeMap::from([
                (ToolParameter::Source, source),
                (ToolParameter::GcRoot, gc_root),
            ]),
        )
        .unwrap();
        assert_eq!(program, executable);
        assert_eq!(
            arguments,
            [
                OsString::from("store"),
                source.as_os_str().to_owned(),
                gc_root.as_os_str().to_owned()
            ]
        );
    }

    #[test]
    fn current_pointer_switches_between_immutable_generation_paths() {
        let base = env::temp_dir().join(format!(
            "nixspace-west-generation-pointer-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("product")).unwrap();
        let paths = WestPaths {
            generations: base.join("product/generations"),
            generation_gc_root: PathBuf::from("locked"),
            local_generations: base.join("local/generations"),
            current: base.join("product/current.json"),
            lock: base.join("publication.lock"),
            view_lock: base.join("view.lock"),
        };
        let first_generation = "abc-1-0";
        write_json_file(
            &paths.current,
            &CurrentGeneration {
                interface_version: WEST_PLAN_INTERFACE_VERSION,
                generation: first_generation.into(),
            },
        )
        .unwrap();
        let first = selected_workspace(&paths).unwrap();

        let second_generation = "def-1-1";
        write_json_file(
            &paths.current,
            &CurrentGeneration {
                interface_version: WEST_PLAN_INTERFACE_VERSION,
                generation: second_generation.into(),
            },
        )
        .unwrap();
        let second = selected_workspace(&paths).unwrap();

        assert_eq!(first.generation, first_generation);
        assert_eq!(second.generation, second_generation);
        assert_ne!(first.locked, second.locked);
        assert!(first.locked.ends_with("abc-1-0/locked"));
        assert!(second.locked.ends_with("def-1-1/locked"));
        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn sealed_generations_survive_sync_until_safe_explicit_gc() {
        use std::os::unix::fs::PermissionsExt;

        let base = env::temp_dir().join(format!(
            "nixspace-west-immutable-sync-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let source = base.join("source");
        fs::create_dir_all(&source).unwrap();
        let manifest = source.join("west.yml");
        fs::write(&manifest, "manifest:\n  projects: []\n").unwrap();
        let west = base.join("fake-west");
        fs::write(
            &west,
            concat!(
                "#!/bin/sh\n",
                "case \"$1\" in\n",
                "  update) test ! -f \"$0.fail\" || exit 9; mkdir -p zephyr/.git ;;\n",
                "  manifest) test \"$2\" = --freeze ;;\n",
                "  list) printf 'manifest|manifest\\nzephyr|zephyr\\n' ;;\n",
                "  *) exit 10 ;;\n",
                "esac\n"
            ),
        )
        .unwrap();
        fs::set_permissions(&west, fs::Permissions::from_mode(0o755)).unwrap();
        let store_seal = base.join("fake-store-seal");
        fs::write(
            &store_seal,
            concat!(
                "#!/bin/sh\n",
                "store=\"$0.store\"\n",
                "test $# = 2 || exit 10\n",
                "if test ! -d \"$store\"; then\n",
                "  cp -R \"$1\" \"$store\" || exit 11\n",
                "  chmod -R a-w \"$store\" || exit 12\n",
                "fi\n",
                "ln -s \"$store\" \"$2\" || exit 13\n",
                "printf '%s\\n' \"$store\"\n"
            ),
        )
        .unwrap();
        fs::set_permissions(&store_seal, fs::Permissions::from_mode(0o755)).unwrap();

        let key = "c".repeat(64);
        let policy = "b".repeat(64);
        let mut plan = WestPlan {
            api_version: WEST_API_VERSION.into(),
            kind: WEST_KIND.into(),
            interface_version: WEST_PLAN_INTERFACE_VERSION,
            product: Product {
                id: "test-product".into(),
                interface_version: 1,
            },
            workspace_root: ".".into(),
            workspace: Workspace {
                id: "test-workspace".into(),
                source: SourceBinding {
                    input: "source".into(),
                    root: ".".into(),
                    identity: SourceIdentity {
                        store_path: source.display().to_string(),
                        nar_hash: None,
                        rev: Some("1".repeat(40)),
                    },
                },
                manifest: ManifestBinding {
                    resource: "test-workspace:west-manifest".into(),
                    relative_path: "west.yml".into(),
                    store_path: manifest,
                    sha256: "a".repeat(64),
                },
                content_key: key.clone(),
            },
            cache: CachePolicy {
                layout_version: WEST_CACHE_LAYOUT_VERSION,
                namespace: "test-product".into(),
                retained_generations: 2,
                root: RootPolicy {
                    base: RootBase::PlatformCache,
                    path: PathBuf::from("nixspace/test-product"),
                },
                native_path_cache: true,
                narrow_update: true,
                paths: CachePaths {
                    generations: PathBuf::from("generations"),
                    generation_gc_root: PathBuf::from("locked"),
                    current: PathBuf::from("current.json"),
                    publication_lock: PathBuf::from("publication.lock"),
                },
            },
            local_view: LocalViewPolicy {
                root: RootPolicy {
                    base: RootBase::Workspace,
                    path: PathBuf::from("views"),
                },
                overrides: Vec::new(),
                policy_id: policy,
                paths: LocalViewPaths {
                    generations: PathBuf::from("views/generations"),
                    execution_lock: PathBuf::from("views/execution.lock"),
                },
            },
            tools: Tools {
                west: west.clone(),
                store: StoreTools {
                    interface_version: STORE_TOOL_INTERFACE_VERSION,
                    seal: ToolCommand {
                        argv: vec![
                            ToolArgument {
                                literal: Some(store_seal.display().to_string()),
                                parameter: None,
                            },
                            ToolArgument {
                                literal: None,
                                parameter: Some(ToolParameter::Source),
                            },
                            ToolArgument {
                                literal: None,
                                parameter: Some(ToolParameter::GcRoot),
                            },
                        ],
                        output: ToolOutput::StorePath,
                    },
                },
                project_path_environment: ProjectPathEnvironment {
                    interface_version: PROJECT_PATH_ENVIRONMENT_INTERFACE_VERSION,
                    count_variable: "GIT_CONFIG_COUNT".into(),
                    key_variable_prefix: "GIT_CONFIG_KEY_".into(),
                    value_variable_prefix: "GIT_CONFIG_VALUE_".into(),
                    key: "safe.directory".into(),
                },
            },
        };
        let paths = WestPaths {
            generations: base.join("cache/generations"),
            generation_gc_root: PathBuf::from("locked"),
            current: base.join("cache/current.json"),
            lock: base.join("cache/locks").join(format!("{key}.lock")),
            view_lock: base.join("views/execution.lock"),
            local_generations: base.join("views/generations"),
        };

        materialize(&base, &plan, &paths, false).unwrap();
        let first = require_ready_workspace(&plan, &paths).unwrap();
        let projection = resolve_local_projection(&base, &plan, &first.locked).unwrap();
        let environment = project_path_environment(&plan, &first.locked, &projection.projects)
            .expect("Nix-declared environment renders exact sealed project paths");
        assert_eq!(
            environment[0],
            (OsString::from("GIT_CONFIG_COUNT"), OsString::from("2"))
        );
        assert!(environment
            .iter()
            .skip(1)
            .filter(|(name, _)| name.to_string_lossy().starts_with("GIT_CONFIG_VALUE_"))
            .all(|(_, value)| Path::new(value).starts_with(&first.locked)));
        assert!(fs::write(first.local.join("zephyr/poison"), []).is_err());
        let malicious = base.join("malicious");
        fs::create_dir_all(&malicious).unwrap();
        fs::remove_file(first.local.join("zephyr")).unwrap();
        symlink_directory(&malicious, &first.local.join("zephyr")).unwrap();
        assert!(!local_projection_ready(&base, &plan, &first.locked, &first.local).unwrap());
        assert!(!status_document(&base, &plan, &paths).ready);
        let selected = selected_workspace_path(&base, &plan, &paths, WestMode::Local).unwrap();
        assert_eq!(selected, first.local);
        assert!(status_document(&base, &plan, &paths).ready);
        assert_eq!(
            fs::read_link(first.local.join("zephyr")).unwrap(),
            first.locked.join("zephyr")
        );
        fs::write(west.with_extension("fail"), []).unwrap();

        assert!(materialize(&base, &plan, &paths, true).is_err());
        let after_failure = require_ready_workspace(&plan, &paths).unwrap();
        assert_eq!(after_failure.generation, first.generation);
        assert!(first.locked.join("zephyr").is_dir());
        assert!(first.local.join("zephyr").is_symlink());

        fs::remove_file(west.with_extension("fail")).unwrap();
        materialize(&base, &plan, &paths, true).unwrap();
        let second = require_ready_workspace(&plan, &paths).unwrap();
        assert_ne!(second.generation, first.generation);
        assert!(first.locked.join("zephyr").is_dir());
        assert!(first.local.join("zephyr").is_symlink());
        assert!(second.locked.join("zephyr").is_dir());

        let first_published = paths.generations.join(&first.generation);
        let foreign = paths
            .generations
            .join("ffffffffffffffffffffffffffffffff-1-0");
        fs::create_dir_all(&foreign).unwrap();
        let error = garbage_collect(&plan, &paths).unwrap_err();
        assert!(error.0.contains("not owned by this exact Nix plan"));
        assert!(first_published.is_dir());
        assert!(first.local.is_dir());
        fs::remove_dir(&foreign).unwrap();

        plan.cache.retained_generations = 1;
        let report = garbage_collect(&plan, &paths).unwrap();
        assert_eq!(report.current_generation, second.generation);
        assert_eq!(report.kept_generations, vec![second.generation.clone()]);
        assert_eq!(report.removed_generations, vec![first.generation.clone()]);
        assert!(!first_published.exists());
        assert!(!first.local.exists());
        assert!(paths.generations.join(&second.generation).is_dir());
        assert!(second.local.is_dir());

        let sealed = store_seal.with_extension("store");
        fn make_writable(path: &Path) {
            let metadata = fs::symlink_metadata(path).unwrap();
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                let mut permissions = metadata.permissions();
                permissions.set_mode(0o755);
                fs::set_permissions(path, permissions).unwrap();
                for entry in fs::read_dir(path).unwrap() {
                    make_writable(&entry.unwrap().path());
                }
            }
        }
        make_writable(&sealed);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn west_update_argv_contains_one_native_path_cache_pair() {
        let seed = Path::new("/cache/seed");
        assert_eq!(west_update_arguments(false, None), vec!["update"]);
        assert_eq!(
            west_update_arguments(true, Some(seed)),
            vec!["update", "--narrow", "--path-cache", "/cache/seed"]
        );
    }

    #[test]
    fn west_config_rejects_injected_control_characters() {
        let base = env::temp_dir().join(format!(
            "nixspace-west-config-control-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        assert!(write_west_config(&base, "manifest", "west.yml\n[evil]").is_err());
        assert!(!base.exists());
    }

    #[test]
    fn safe_paths_never_target_a_global_src_west_directory() {
        assert!(safe_relative(Path::new("src/telemetry")));
        assert!(!safe_relative(Path::new("../src/.west")));
        assert!(!safe_relative(Path::new("/workspace/src/.west")));
    }

    #[test]
    fn persistent_plan_paths_must_not_equal_or_contain_one_another() {
        assert!(paths_overlap(
            Path::new("namespace/generations"),
            Path::new("namespace/generations/current.json")
        ));
        assert!(paths_overlap(
            Path::new("namespace/lock"),
            Path::new("namespace/lock")
        ));
        assert!(!paths_overlap(
            Path::new("namespace/generations"),
            Path::new("namespace/current.json")
        ));
    }

    #[test]
    fn external_run_directory_is_relative_to_the_materialized_view() {
        let base =
            env::temp_dir().join(format!("nixspace-west-external-cwd-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("modules/lib/example")).unwrap();

        assert_eq!(
            external_run_directory(&base, Path::new("modules/lib/example")).unwrap(),
            base.join("modules/lib/example")
        );
        assert!(external_run_directory(&base, Path::new("../example")).is_err());
        assert!(external_run_directory(&base, Path::new("modules/lib/missing")).is_err());

        fs::remove_dir_all(base).unwrap();
    }
}
