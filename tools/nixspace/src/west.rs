use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

use super::{CliError, Result};

const WEST_PLAN_INTERFACE_VERSION: u64 = 1;
const WEST_CACHE_LAYOUT_VERSION: u64 = 1;
const WEST_API_VERSION: &str = "nixspace/v1";
const WEST_KIND: &str = "WestWorkspace";

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
    root: RootPolicy,
    native_path_cache: bool,
    narrow_update: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalViewPolicy {
    root: RootPolicy,
    overrides: Vec<LocalOverride>,
    policy_id: String,
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
struct Tools {
    west: PathBuf,
}

#[derive(Debug)]
struct WestPaths {
    cache: PathBuf,
    locked: PathBuf,
    local: PathBuf,
    lock: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WestStatus<'a> {
    interface_version: u64,
    product: &'a str,
    workspace: &'a str,
    manifest_resource: &'a str,
    content_key: &'a str,
    locked: &'a Path,
    local: &'a Path,
    path_cache_seed: Option<PathBuf>,
    ready: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LockedMarker {
    interface_version: u64,
    product: String,
    workspace: String,
    manifest_resource: String,
    manifest_sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalMarker {
    interface_version: u64,
    product: String,
    workspace: String,
    manifest_sha256: String,
    policy_id: String,
    overrides: BTreeMap<String, String>,
}

pub(crate) fn run(root: &Path, explicit_plan: Option<PathBuf>, command: WestCommand) -> Result<()> {
    let plan_path = west_plan_path(explicit_plan)?;
    let plan = load_plan(&plan_path)?;
    validate_plan(&plan, &plan_path)?;
    let workspace_root = resolve_workspace_root(root, &plan.workspace_root)?;
    let paths = west_paths(&workspace_root, &plan)?;

    match command {
        WestCommand::Validate { json } => {
            let status = status_document(&plan, &paths);
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
            emit_status(&plan, &paths, json)
        }
        WestCommand::Sync { json } => {
            materialize(&workspace_root, &plan, &paths, true)?;
            emit_status(&plan, &paths, json)
        }
        WestCommand::Status { json } => emit_status(&plan, &paths, json),
        WestCommand::Path { mode } => {
            println!(
                "{}",
                match mode {
                    WestMode::Local => &paths.local,
                    WestMode::Release => &paths.locked,
                }
                .display()
            );
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
            run_inherited(&plan.tools.west, &arguments, &paths.local)
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
            let cwd = external_run_directory(&paths.local, &cwd)?;
            run_inherited(Path::new(program), arguments, &cwd)
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
    if !plan.workspace.manifest.store_path.is_file() {
        return Err(CliError(format!(
            "Nix-selected West manifest {} from {} does not exist",
            plan.workspace.manifest.store_path.display(),
            source.display()
        )));
    }
    let manifest_repository =
        Path::new(&plan.workspace.source.identity.store_path).join(&plan.workspace.source.root);
    if !manifest_repository.is_dir() {
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
    if !plan.tools.west.is_file() {
        return Err(CliError(format!(
            "Nix-selected native West executable does not exist: {}",
            plan.tools.west.display()
        )));
    }
    if !valid_sha256(&plan.local_view.policy_id) {
        return Err(CliError(
            "West plan local-view policy ID is not a lowercase SHA-256 value".into(),
        ));
    }
    if plan.cache.root.base != RootBase::PlatformCache
        || plan.local_view.root.base != RootBase::Workspace
        || !strict_safe_relative(&plan.cache.root.path)
        || !strict_safe_relative(&plan.local_view.root.path)
    {
        return Err(CliError(
            "West plan cache.root must be a safe platform-cache-relative path and localView.root must be a safe workspace-relative path"
                .into(),
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
    let cache = if let Some(configured) = env::var_os("NIXSPACE_WEST_CACHE") {
        absolute_path(Path::new(&configured))?
    } else {
        platform_cache_root()?.join(&plan.cache.root.path)
    };
    let product_root = cache.join("workspaces").join(&plan.workspace.content_key);
    let local = env::var_os("NIXSPACE_WEST_VIEW_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(&plan.local_view.root.path))
        .join(&plan.product.id)
        .join(&plan.workspace.content_key)
        .join(&plan.local_view.policy_id);
    Ok(WestPaths {
        locked: product_root.join("locked"),
        lock: cache
            .join("locks")
            .join(format!("{}.lock", plan.workspace.content_key)),
        cache,
        local,
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
    fs::create_dir_all(
        paths
            .lock
            .parent()
            .expect("content-addressed lock always has a parent"),
    )
    .map_err(|error| CliError(format!("cannot create West lock directory: {error}")))?;
    let _guard = ProductLock::acquire(&paths.lock)?;

    let marker_ready = locked_ready(plan, &paths.locked);
    if force || !marker_ready {
        initialize_locked(plan, paths, force)?;
    } else if !native_checkout_ready(plan, &paths.locked) {
        initialize_locked(plan, paths, true)?;
    }
    if force || !local_ready(plan, &paths.local) {
        create_local_view(root, plan, paths)?;
    }
    Ok(())
}

fn initialize_locked(plan: &WestPlan, paths: &WestPaths, force: bool) -> Result<()> {
    let already_initialized = paths.locked.join(".west/config").is_file();
    if !already_initialized {
        remove_if_exists(&paths.locked)?;
        fs::create_dir_all(&paths.locked).map_err(|error| {
            CliError(format!(
                "cannot create immutable West workspace {}: {error}",
                paths.locked.display()
            ))
        })?;
        let manifest = paths.locked.join("manifest");
        let manifest_source =
            Path::new(&plan.workspace.source.identity.store_path).join(&plan.workspace.source.root);
        symlink_directory(&manifest_source, &manifest)?;
        write_west_config(
            &paths.locked,
            "manifest",
            &plan.workspace.manifest.relative_path,
        )?;
    } else if !force && !locked_ready(plan, &paths.locked) {
        remove_if_exists(&paths.locked)?;
        return initialize_locked(plan, paths, false);
    }

    let seed = plan
        .cache
        .native_path_cache
        .then(|| path_cache_seed(&paths.cache, &paths.locked))
        .flatten();
    let arguments = west_update_arguments(plan.cache.narrow_update, seed.as_deref());
    run_inherited(&plan.tools.west, &arguments, &paths.locked)?;

    let marker = LockedMarker {
        interface_version: WEST_PLAN_INTERFACE_VERSION,
        product: plan.product.id.clone(),
        workspace: plan.workspace.id.clone(),
        manifest_resource: plan.workspace.manifest.resource.clone(),
        manifest_sha256: plan.workspace.manifest.sha256.clone(),
    };
    write_json_file(&paths.locked.join(".nixspace-west.json"), &marker)
}

fn create_local_view(root: &Path, plan: &WestPlan, paths: &WestPaths) -> Result<()> {
    let projects = resolved_project_paths(plan, &paths.locked)?;
    let overrides: BTreeMap<_, _> = plan
        .local_view
        .overrides
        .iter()
        .map(|binding| (binding.project.as_str(), binding))
        .collect();
    let parent = paths
        .local
        .parent()
        .expect("local view always has a content-addressed parent");
    fs::create_dir_all(parent)
        .map_err(|error| CliError(format!("cannot create local West view parent: {error}")))?;
    let temporary = parent.join(format!(
        ".{}.tmp-{}",
        plan.local_view.policy_id,
        std::process::id()
    ));
    remove_if_exists(&temporary)?;
    fs::create_dir_all(&temporary)
        .map_err(|error| CliError(format!("cannot create temporary local West view: {error}")))?;

    let mut effective_overrides = BTreeMap::new();
    let mut matched_overrides = BTreeSet::new();
    for (name, relative) in projects {
        let immutable = paths.locked.join(&relative);
        let source = if let Some(binding) = overrides.get(name.as_str()) {
            matched_overrides.insert(name.clone());
            let editable = root.join(&binding.source);
            if editable.is_dir() {
                effective_overrides.insert(name.clone(), editable.display().to_string());
                editable
            } else if binding.required {
                remove_if_exists(&temporary)?;
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
            remove_if_exists(&temporary)?;
            return Err(CliError(format!(
                "native West resolved project `{name}` to missing directory {}",
                source.display()
            )));
        }
        let destination = temporary.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                CliError(format!("cannot create local West project parent: {error}"))
            })?;
        }
        symlink_directory(&source, &destination)?;
    }
    let unmatched: Vec<_> = overrides
        .keys()
        .filter(|name| !matched_overrides.contains(**name))
        .copied()
        .collect();
    if !unmatched.is_empty() {
        remove_if_exists(&temporary)?;
        return Err(CliError(format!(
            "Nix-declared West overrides do not match native West projects: {}",
            unmatched.join(", ")
        )));
    }
    if !temporary.join("manifest").exists() {
        symlink_directory(&paths.locked.join("manifest"), &temporary.join("manifest"))?;
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
        manifest_sha256: plan.workspace.manifest.sha256.clone(),
        policy_id: plan.local_view.policy_id.clone(),
        overrides: effective_overrides,
    };
    write_json_file(&temporary.join(".nixspace-west-local.json"), &marker)?;
    remove_if_exists(&paths.local)?;
    fs::rename(&temporary, &paths.local).map_err(|error| {
        CliError(format!(
            "cannot install local West view {}: {error}",
            paths.local.display()
        ))
    })
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
        if name.is_empty() || !safe_relative(&path) {
            return Err(CliError(format!(
                "native West returned an unsafe project record: {line}"
            )));
        }
        if !names.insert(name.to_owned()) || !paths.insert(path.clone()) {
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
            && marker.manifest_sha256 == plan.workspace.manifest.sha256
            && marker.policy_id == plan.local_view.policy_id
            && local.join(".west/config").is_file()
    })
}

fn status_document<'a>(plan: &'a WestPlan, paths: &'a WestPaths) -> WestStatus<'a> {
    WestStatus {
        interface_version: WEST_PLAN_INTERFACE_VERSION,
        product: &plan.product.id,
        workspace: &plan.workspace.id,
        manifest_resource: &plan.workspace.manifest.resource,
        content_key: &plan.workspace.content_key,
        locked: &paths.locked,
        local: &paths.local,
        path_cache_seed: path_cache_seed(&paths.cache, &paths.locked),
        ready: locked_ready(plan, &paths.locked) && local_ready(plan, &paths.local),
    }
}

fn emit_status(plan: &WestPlan, paths: &WestPaths, json: bool) -> Result<()> {
    let seed = path_cache_seed(&paths.cache, &paths.locked);
    let status = WestStatus {
        interface_version: WEST_PLAN_INTERFACE_VERSION,
        product: &plan.product.id,
        workspace: &plan.workspace.id,
        manifest_resource: &plan.workspace.manifest.resource,
        content_key: &plan.workspace.content_key,
        locked: &paths.locked,
        local: &paths.local,
        path_cache_seed: seed,
        ready: locked_ready(plan, &paths.locked) && local_ready(plan, &paths.local),
    };
    if json {
        return super::write_json(&status);
    }
    println!("product:        {}", status.product);
    println!("workspace:      {}", status.workspace);
    println!("manifest:       {}", status.manifest_resource);
    println!("content:        {}", status.content_key);
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

fn path_cache_seed(cache: &Path, current: &Path) -> Option<PathBuf> {
    let workspaces = cache.join("workspaces");
    let mut candidates = fs::read_dir(workspaces)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().join("locked"))
        .filter(|candidate| {
            candidate != current
                && candidate.join(".nixspace-west.json").is_file()
                && candidate.join(".west/config").is_file()
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.into_iter().next()
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
    if contains_config_control(manifest_path) || contains_config_control(manifest_file) {
        return Err(CliError(
            "West manifest configuration contains a forbidden control character".into(),
        ));
    }
    let west = workspace.join(".west");
    fs::create_dir_all(&west)
        .map_err(|error| CliError(format!("cannot create local West configuration: {error}")))?;
    fs::write(
        west.join("config"),
        format!(
            "[manifest]\npath = {manifest_path}\nfile = {manifest_file}\n\n[zephyr]\nbase = zephyr\n"
        ),
    )
    .map_err(|error| CliError(format!("cannot write local West configuration: {error}")))
}

fn contains_config_control(value: &str) -> bool {
    value.bytes().any(|byte| matches!(byte, b'\n' | b'\r' | 0))
}

fn run_inherited(program: &Path, arguments: &[String], cwd: &Path) -> Result<()> {
    let status = Command::new(program)
        .args(arguments)
        .current_dir(cwd)
        .status()
        .map_err(|error| {
            CliError(format!(
                "cannot invoke Nix-selected native West {}: {error}",
                program.display()
            ))
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError(format!(
            "native West command failed with {}: {} {}",
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

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn write_json_file(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| CliError(format!("cannot serialize West state marker: {error}")))?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temporary, [bytes.as_slice(), b"\n"].concat())
        .map_err(|error| CliError(format!("cannot write West state marker: {error}")))?;
    fs::rename(&temporary, path)
        .map_err(|error| CliError(format!("cannot install West state marker: {error}")))
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

struct ProductLock {
    path: PathBuf,
}

impl ProductLock {
    fn acquire(path: &Path) -> Result<Self> {
        fs::create_dir(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                CliError(format!(
                    "West product cache is already being updated: {}; retry after the other command finishes",
                    path.display()
                ))
            } else {
                CliError(format!("cannot acquire West product lock {}: {error}", path.display()))
            }
        })?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for ProductLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.path);
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
            "telemetry|modules/lib/telemetry\ntelemetry|other\n",
            "telemetry|modules/lib/telemetry\nother|modules/lib/telemetry\n",
        ] {
            assert!(
                parse_project_paths(invalid).is_err(),
                "accepted {invalid:?}"
            );
        }
    }

    #[test]
    fn path_cache_uses_only_complete_other_product_workspaces() {
        let base = env::temp_dir().join(format!("nixspace-west-path-cache-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let ready = base.join("workspaces/aaa/locked");
        let incomplete = base.join("workspaces/bbb/locked");
        fs::create_dir_all(ready.join(".west")).unwrap();
        fs::create_dir_all(incomplete.join(".west")).unwrap();
        fs::write(ready.join(".west/config"), "[manifest]\n").unwrap();
        fs::write(incomplete.join(".west/config"), "[manifest]\n").unwrap();
        fs::write(ready.join(".nixspace-west.json"), "{}\n").unwrap();

        assert_eq!(path_cache_seed(&base, &incomplete), Some(ready));
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
