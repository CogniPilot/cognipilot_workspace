use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use clap::{Args, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::action::Outcome;
use crate::{write_json, CliError, Result};

const SUPPORTED_API_VERSION: &str = "nixspace/v1";
const SUPPORTED_KIND: &str = "Host";
const SUPPORTED_INTERFACE_VERSION: u64 = 4;
const MANAGED_BEGIN: &str = "# BEGIN NIXSPACE MANAGED SETTINGS";
const MANAGED_END: &str = "# END NIXSPACE MANAGED SETTINGS";

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum NixMode {
    Auto,
    SingleUser,
    Daemon,
}

#[derive(Debug)]
pub(crate) struct Options {
    pub(crate) plan: Option<PathBuf>,
    pub(crate) nix_config: Option<PathBuf>,
    pub(crate) mode: Option<NixMode>,
    pub(crate) index: Option<PathBuf>,
    pub(crate) source_plan: Option<PathBuf>,
    pub(crate) launch_plan: Option<PathBuf>,
    pub(crate) resolution_plan: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub(crate) struct DoctorArgs {
    /// Emit the complete diagnostic report as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct SetupArgs {
    /// Verify the managed configuration and tools without changing the host.
    #[arg(long)]
    check: bool,
}

#[derive(Debug, Args)]
pub(crate) struct CacheCoverageArgs {
    /// Emit the complete cache coverage report as JSON on standard output.
    #[arg(long)]
    json: bool,

    /// Atomically retain the JSON report at this path.
    #[arg(long, value_name = "PATH")]
    output: Option<PathBuf>,

    /// Append a Markdown table to this summary file.
    #[arg(long, value_name = "PATH")]
    summary: Option<PathBuf>,

    /// Return failure after writing reports unless every declared root is fully cached.
    #[arg(long)]
    require_complete: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostPlan {
    api_version: String,
    kind: String,
    interface_version: u64,
    nix: NixExpectation,
    readiness: ReadinessExpectation,
    tools: BTreeMap<String, ToolExpectation>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReadinessExpectation {
    storage: StorageExpectation,
    daemon: DaemonExpectation,
    required_documents: Vec<DocumentKind>,
    source_selection: String,
    launch: LaunchExpectation,
    cache: CacheExpectation,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CacheExpectation {
    store_directory: PathBuf,
    roots: Vec<CacheRootExpectation>,
    stores: Vec<CacheStoreExpectation>,
    coverage_mode: CacheCoverageMode,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum CacheCoverageMode {
    Union,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CacheRootExpectation {
    name: String,
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CacheStoreExpectation {
    name: String,
    uri: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StorageExpectation {
    path: PathBuf,
    minimum_available_bytes: u64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum DaemonWhen {
    Always,
    DaemonMode,
    Never,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DaemonExpectation {
    when: DaemonWhen,
    probe_argv: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "lowercase")]
enum DocumentKind {
    Index,
    Source,
    Launch,
    Resolution,
}

impl DocumentKind {
    fn name(self) -> &'static str {
        match self {
            Self::Index => "index",
            Self::Source => "source",
            Self::Launch => "launch",
            Self::Resolution => "resolution",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LaunchExpectation {
    allow_active_sessions: bool,
    require_manager_socket: bool,
    require_available_declared_ports: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NixExpectation {
    minimum_version: String,
    settings: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ToolExpectation {
    executable: String,
    expected_version: String,
    version_argv: Vec<String>,
    install_argv: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
enum EffectiveMode {
    SingleUser,
    Daemon,
}

#[derive(Debug)]
struct Target {
    path: PathBuf,
    mode: EffectiveMode,
    system_config: bool,
    declarative_reason: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticReport {
    api_version: &'static str,
    kind: &'static str,
    interface_version: u64,
    compliant: bool,
    plan: PathBuf,
    configuration: ConfigurationDiagnostic,
    nix: NixDiagnostic,
    tools: BTreeMap<String, ToolDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    readiness: Option<ReadinessDiagnostic>,
    security_warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadinessDiagnostic {
    satisfied: bool,
    storage: StorageDiagnostic,
    daemon: DaemonDiagnostic,
    cache: CacheDiagnostic,
    documents: BTreeMap<String, DocumentDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<crate::source::SourceReadiness>,
    #[serde(skip_serializing_if = "Option::is_none")]
    launch: Option<crate::launch_exec::LaunchReadiness>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheDiagnostic {
    coverage_mode: CacheCoverageMode,
    roots: Vec<CacheRootDiagnostic>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheCoverageReport {
    api_version: &'static str,
    kind: &'static str,
    interface_version: u64,
    complete: bool,
    plan: PathBuf,
    cache: CacheDiagnostic,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheRootDiagnostic {
    name: String,
    path: PathBuf,
    store_path: Option<PathBuf>,
    observed: bool,
    path_count: u64,
    nar_bytes: u64,
    union: Option<CacheUnionDiagnostic>,
    stores: Vec<CacheStoreDiagnostic>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheUnionDiagnostic {
    observed_store_count: u64,
    hit_path_count: u64,
    miss_path_count: u64,
    hit_nar_bytes: u64,
    miss_nar_bytes: u64,
    /// The least compressed payload bytes advertised for each path by any
    /// observed store. This is null when no observed hit exposes a
    /// `downloadSize` for one or more covered paths or when the total cannot
    /// be represented as a `u64`.
    cache_download_bytes: Option<u64>,
    /// Cache inventory does not observe bytes transferred by an earlier
    /// build or copy.
    transferred_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheStoreDiagnostic {
    name: String,
    uri: String,
    observed: bool,
    hit_path_count: u64,
    miss_path_count: u64,
    hit_nar_bytes: u64,
    miss_nar_bytes: u64,
    /// Compressed payload bytes advertised by the binary cache for hit paths.
    /// This is null when a store does not expose `downloadSize`.
    cache_download_bytes: Option<u64>,
    /// The stable `nix path-info --json` interface exposes inventory metadata,
    /// not bytes actually transferred by a previous build/copy.
    transferred_bytes: Option<u64>,
    error: Option<String>,
}

#[derive(Debug)]
struct CacheStoreObservation {
    diagnostic: CacheStoreDiagnostic,
    hit_download_bytes: BTreeMap<String, Option<u64>>,
}

#[derive(Clone, Copy, Debug)]
struct PathInfoMeasurement {
    nar_size: u64,
    download_size: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StorageDiagnostic {
    path: PathBuf,
    minimum_available_bytes: u64,
    available_bytes: Option<u64>,
    satisfied: bool,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DaemonDiagnostic {
    required: bool,
    checked: bool,
    probe_argv: Vec<String>,
    satisfied: bool,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DocumentDiagnostic {
    path: Option<PathBuf>,
    ready: bool,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigurationDiagnostic {
    path: PathBuf,
    mode: EffectiveMode,
    exists: bool,
    declarative: bool,
    declarative_reason: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NixDiagnostic {
    installed: bool,
    version: Option<String>,
    minimum_version: String,
    version_satisfied: bool,
    config_available: bool,
    config_error: Option<String>,
    settings: Vec<SettingDiagnostic>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingDiagnostic {
    name: String,
    actual_name: String,
    expected: Value,
    actual: Option<Value>,
    satisfied: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolDiagnostic {
    executable: String,
    expected_version: String,
    installed: bool,
    version: Option<String>,
    satisfied: bool,
    error: Option<String>,
}

struct Loaded<'a> {
    path: PathBuf,
    plan: HostPlan,
    target: Target,
    root: &'a Path,
    index: Option<PathBuf>,
    source_plan: Option<PathBuf>,
    launch_plan: Option<PathBuf>,
    resolution_plan: Option<PathBuf>,
}

pub(crate) fn doctor(root: &Path, options: &Options, arguments: DoctorArgs) -> Result<Outcome> {
    let loaded = load(root, options)?;
    let report = diagnose(&loaded, true);
    emit_report(&report, arguments.json)?;
    if report.compliant {
        Ok(Outcome::Success)
    } else {
        Ok(Outcome::Exit(1))
    }
}

pub(crate) fn cache_coverage(
    root: &Path,
    options: &Options,
    arguments: CacheCoverageArgs,
) -> Result<Outcome> {
    let (plan_path, plan) = load_plan(root, options)?;
    let cache = diagnose_cache(&plan.readiness.cache, root);
    let complete = !cache.roots.is_empty()
        && cache.roots.iter().all(|root| {
            root.observed
                && root.union.as_ref().is_some_and(|union| {
                    union.observed_store_count > 0 && union.miss_path_count == 0
                })
        });
    let report = CacheCoverageReport {
        api_version: SUPPORTED_API_VERSION,
        kind: "CacheCoverage",
        interface_version: 2,
        complete,
        plan: plan_path,
        cache,
    };
    if let Some(path) = arguments.output {
        write_cache_report(&resolve(root, path), &report)?;
    }
    if let Some(path) = arguments.summary {
        append_cache_summary(&resolve(root, path), &report)?;
    }
    if arguments.json {
        write_json(&report)?;
    } else {
        emit_cache_diagnostic(&report.cache);
        println!(
            "Cache coverage: {}",
            if report.complete {
                "complete"
            } else {
                "incomplete"
            }
        );
    }
    if arguments.require_complete && !report.complete {
        Ok(Outcome::Exit(1))
    } else {
        Ok(Outcome::Success)
    }
}

pub(crate) fn setup(root: &Path, options: &Options, arguments: SetupArgs) -> Result<Outcome> {
    let loaded = load(root, options)?;
    emit_security_warnings(&security_warnings(&loaded.plan));

    let expected = render_configuration(&loaded.plan.nix.settings)?;
    let existing = read_optional(&loaded.target.path)?;
    let merged = merge_managed_block(existing.as_deref(), &expected)?;
    let configuration_current = existing.as_deref() == Some(merged.as_str());

    if arguments.check {
        let declarative = loaded.target.declarative_reason.is_some();
        if !configuration_current && !declarative {
            eprintln!(
                "Nix configuration is not current at {}",
                loaded.target.path.display()
            );
        }
        let report = diagnose(&loaded, false);
        emit_report(&report, false)?;
        return if (configuration_current || declarative) && report.compliant {
            Ok(Outcome::Success)
        } else {
            Ok(Outcome::Exit(1))
        };
    }

    if let Some(reason) = &loaded.target.declarative_reason {
        return Err(CliError(format!(
            "refusing to update declaratively managed Nix configuration at {}: {reason}",
            loaded.target.path.display()
        )));
    }

    let mutated = if configuration_current {
        println!(
            "Nix configuration is already current: {}",
            loaded.target.path.display()
        );
        false
    } else {
        install_configuration(&loaded.target, &merged)?;
        println!(
            "Updated Nix configuration: {}",
            loaded.target.path.display()
        );
        true
    };
    if mutated && matches!(loaded.target.mode, EffectiveMode::Daemon) {
        restart_daemon(loaded.root)?;
    }

    for (name, expectation) in &loaded.plan.tools {
        let diagnostic = inspect_tool(expectation, loaded.root);
        if diagnostic.satisfied {
            println!(
                "Tool {name} is current: {}",
                diagnostic.version.as_deref().unwrap_or("unknown")
            );
            continue;
        }
        println!(
            "Installing {name} {} with the Nix-generated command...",
            expectation.expected_version
        );
        let status = run_streaming(&expectation.install_argv, loaded.root, "tool installer")?;
        if !status.success() {
            return Ok(Outcome::Exit(status.code().unwrap_or(1)));
        }
        let after = inspect_tool(expectation, loaded.root);
        if !after.satisfied {
            return Err(CliError(format!(
                "tool `{name}` is still not at expected version `{}` after installation{}",
                expectation.expected_version,
                after
                    .error
                    .as_deref()
                    .map(|error| format!(": {error}"))
                    .unwrap_or_default()
            )));
        }
    }

    let report = diagnose(&loaded, false);
    emit_report(&report, false)?;
    if report.compliant {
        Ok(Outcome::Success)
    } else {
        Ok(Outcome::Exit(1))
    }
}

fn load<'a>(root: &'a Path, options: &Options) -> Result<Loaded<'a>> {
    let (path, plan) = load_plan(root, options)?;
    let target = target(root, options)?;
    Ok(Loaded {
        path,
        plan,
        target,
        root,
        index: options.index.clone(),
        source_plan: options.source_plan.clone(),
        launch_plan: options.launch_plan.clone(),
        resolution_plan: options.resolution_plan.clone(),
    })
}

fn load_plan(root: &Path, options: &Options) -> Result<(PathBuf, HostPlan)> {
    let path = options
        .plan
        .clone()
        .or_else(|| env::var_os("NIXSPACE_HOST_PLAN").map(PathBuf::from))
        .map(|path| resolve(root, path))
        .ok_or_else(|| {
            CliError(
                "host setup requires an explicit Nix-generated plan; pass --host-plan or set NIXSPACE_HOST_PLAN"
                    .into(),
            )
        })?;
    let bytes = fs::read(&path).map_err(|error| {
        CliError(format!(
            "host plan is unavailable at {}: {error}; set --host-plan or NIXSPACE_HOST_PLAN",
            path.display()
        ))
    })?;
    let plan: HostPlan = serde_json::from_slice(&bytes).map_err(|error| {
        CliError(format!(
            "Nix-generated host plan at {} is unreadable: {error}",
            path.display()
        ))
    })?;
    validate_plan(&plan)?;
    Ok((path, plan))
}

fn validate_plan(plan: &HostPlan) -> Result<()> {
    if plan.api_version != SUPPORTED_API_VERSION {
        return Err(CliError(format!(
            "host API `{}` is unsupported; this nixspace requires `{SUPPORTED_API_VERSION}`",
            plan.api_version
        )));
    }
    if plan.kind != SUPPORTED_KIND {
        return Err(CliError(format!(
            "host kind `{}` is unsupported; this nixspace requires `{SUPPORTED_KIND}`",
            plan.kind
        )));
    }
    if plan.interface_version != SUPPORTED_INTERFACE_VERSION {
        return Err(CliError(format!(
            "host interface version {} is unsupported; this nixspace supports version {}",
            plan.interface_version, SUPPORTED_INTERFACE_VERSION
        )));
    }
    parse_version(&plan.nix.minimum_version).ok_or_else(|| {
        CliError(format!(
            "host nix.minimumVersion `{}` is not a dotted numeric version",
            plan.nix.minimum_version
        ))
    })?;
    if plan.nix.settings.is_empty() {
        return Err(CliError("host nix.settings must not be empty".into()));
    }
    if plan.readiness.storage.path.as_os_str().is_empty()
        || plan.readiness.storage.path.to_string_lossy().contains('\0')
    {
        return Err(CliError(
            "host readiness.storage.path must be nonempty and contain no NUL".into(),
        ));
    }
    if !matches!(plan.readiness.daemon.when, DaemonWhen::Never)
        && (plan.readiness.daemon.probe_argv.is_empty()
            || plan.readiness.daemon.probe_argv[0].is_empty()
            || plan
                .readiness
                .daemon
                .probe_argv
                .iter()
                .any(|argument| argument.contains('\0')))
    {
        return Err(CliError(
            "host readiness.daemon.probeArgv must begin with a nonempty executable and contain no NUL"
                .into(),
        ));
    }
    if plan.readiness.source_selection.is_empty()
        || plan.readiness.source_selection.contains(['\0', '\n', '\r'])
    {
        return Err(CliError(
            "host readiness.sourceSelection must be nonempty and contain no control characters"
                .into(),
        ));
    }
    let mut cache_root_names = BTreeSet::new();
    if !plan.readiness.cache.store_directory.is_absolute()
        || plan
            .readiness
            .cache
            .store_directory
            .to_string_lossy()
            .contains(['\0', '\n', '\r'])
    {
        return Err(CliError(
            "host readiness.cache.storeDirectory must be an absolute path".into(),
        ));
    }
    for root in &plan.readiness.cache.roots {
        if !valid_observation_name(&root.name) || !cache_root_names.insert(&root.name) {
            return Err(CliError(
                "host readiness.cache.roots must have unique names containing only ASCII letters, digits, `.`, `_`, or `-`"
                    .into(),
            ));
        }
        if root.path.as_os_str().is_empty()
            || root.path.to_string_lossy().contains(['\0', '\n', '\r'])
            || (!root.path.is_absolute()
                && root.path.components().any(|component| {
                    matches!(
                        component,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                }))
        {
            return Err(CliError(
                "host readiness.cache.roots paths must be nonempty absolute paths or safe workspace-relative paths"
                    .into(),
            ));
        }
    }
    let configured_substituters = plan
        .nix
        .settings
        .get("extra-substituters")
        .or_else(|| plan.nix.settings.get("substituters"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    if plan.readiness.cache.stores.is_empty() {
        return Err(CliError(
            "host readiness.cache.stores must declare at least one union store".into(),
        ));
    }
    let mut cache_store_names = BTreeSet::new();
    for store in &plan.readiness.cache.stores {
        if !valid_observation_name(&store.name) || !cache_store_names.insert(&store.name) {
            return Err(CliError(
                "host readiness.cache.stores must have unique names containing only ASCII letters, digits, `.`, `_`, or `-`"
                    .into(),
            ));
        }
        if !public_cache_uri(&store.uri) {
            return Err(CliError(
                "host readiness.cache store URIs must be credential-free public HTTPS URLs".into(),
            ));
        }
        if !configured_substituters.contains(store.uri.as_str()) {
            return Err(CliError(
                "every host readiness.cache store URI must also be an expected Nix substituter"
                    .into(),
            ));
        }
    }
    let documents = plan
        .readiness
        .required_documents
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if documents.len() != plan.readiness.required_documents.len() {
        return Err(CliError(
            "host readiness.requiredDocuments must not contain duplicates".into(),
        ));
    }
    for (name, value) in &plan.nix.settings {
        validate_setting(name, value)?;
    }
    for (name, tool) in &plan.tools {
        if name.is_empty()
            || tool.executable.is_empty()
            || tool.expected_version.is_empty()
            || tool.version_argv.is_empty()
            || tool.version_argv[0] != tool.executable
            || tool.install_argv.is_empty()
            || tool
                .version_argv
                .iter()
                .chain(tool.install_argv.iter())
                .any(|argument| argument.contains('\0'))
        {
            return Err(CliError(format!(
                "host tool `{name}` must declare a nonempty executable, expectedVersion, versionArgv beginning with that executable, and installArgv, without NUL bytes"
            )));
        }
    }
    Ok(())
}

fn valid_observation_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn public_cache_uri(value: &str) -> bool {
    let Some(authority_and_path) = value.strip_prefix("https://") else {
        return false;
    };
    let authority = authority_and_path
        .split_once('/')
        .map_or(authority_and_path, |(authority, _)| authority);
    !authority.is_empty() && !value.contains(['\0', '\n', '\r', '\t', ' ', '?', '#', '@'])
}

fn validate_setting(name: &str, value: &Value) -> Result<()> {
    if name.is_empty()
        || name.contains(char::is_whitespace)
        || name.contains('=')
        || name.contains('\0')
        || name.contains('\n')
        || name.contains('\r')
    {
        return Err(CliError(format!("invalid Nix setting name `{name}`")));
    }
    match value {
        Value::Bool(_) | Value::Number(_) => Ok(()),
        Value::String(value) => validate_setting_scalar(name, value),
        Value::Array(values) => {
            for value in values {
                let value = value.as_str().ok_or_else(|| {
                    CliError(format!(
                        "Nix setting `{name}` arrays must contain only strings"
                    ))
                })?;
                validate_setting_scalar(name, value)?;
                if value.contains(char::is_whitespace) {
                    return Err(CliError(format!(
                        "Nix setting `{name}` list item `{value}` contains whitespace and cannot be represented losslessly in nix.conf"
                    )));
                }
            }
            Ok(())
        }
        _ => Err(CliError(format!(
            "Nix setting `{name}` must be a boolean, number, string, or string array"
        ))),
    }
}

fn validate_setting_scalar(name: &str, value: &str) -> Result<()> {
    if value.contains('\0') || value.contains('\n') || value.contains('\r') {
        Err(CliError(format!(
            "Nix setting `{name}` contains a control character"
        )))
    } else {
        Ok(())
    }
}

fn target(root: &Path, options: &Options) -> Result<Target> {
    let explicit_path = options
        .nix_config
        .clone()
        .or_else(|| env::var_os("NIXSPACE_NIX_CONFIG").map(PathBuf::from));
    let requested_mode = options
        .mode
        .or_else(|| {
            env::var("NIXSPACE_NIX_MODE")
                .ok()
                .and_then(|value| NixMode::from_str(&value, true).ok())
        })
        .unwrap_or(NixMode::Auto);
    let mode = match requested_mode {
        NixMode::SingleUser => EffectiveMode::SingleUser,
        NixMode::Daemon => EffectiveMode::Daemon,
        NixMode::Auto if daemon_detected() => EffectiveMode::Daemon,
        NixMode::Auto => EffectiveMode::SingleUser,
    };
    let path = if let Some(path) = explicit_path.as_ref() {
        resolve(root, path.clone())
    } else if matches!(mode, EffectiveMode::Daemon) {
        PathBuf::from("/etc/nix/nix.conf")
    } else {
        user_config_path()?
    };
    let system_config = matches!(mode, EffectiveMode::Daemon) && path.starts_with("/etc");
    let symlink = fs::symlink_metadata(&path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false);
    let nixos_marker = env::var_os("NIXSPACE_NIXOS_MARKER")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/NIXOS"));
    let declarative_reason = if symlink {
        Some("the configuration path is a symlink".into())
    } else if matches!(mode, EffectiveMode::Daemon) && nixos_marker.exists() {
        Some(format!(
            "NixOS marker {} exists; declare these settings in the operating-system configuration",
            nixos_marker.display()
        ))
    } else {
        None
    };
    Ok(Target {
        path,
        mode,
        system_config,
        declarative_reason,
    })
}

fn resolve(root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn user_config_path() -> Result<PathBuf> {
    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(xdg).join("nix/nix.conf"));
    }
    env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".config/nix/nix.conf"))
        .ok_or_else(|| CliError("cannot select single-user nix.conf because HOME is unset".into()))
}

fn daemon_detected() -> bool {
    Path::new("/nix/var/nix/daemon-socket/socket").exists()
        || (cfg!(target_os = "macos")
            && Path::new("/Library/LaunchDaemons/org.nixos.nix-daemon.plist").exists())
}

fn diagnose(loaded: &Loaded<'_>, include_readiness: bool) -> DiagnosticReport {
    let version_output = command_output(&["nix", "--version"], loaded.root);
    let (installed, version, version_satisfied) = match version_output {
        Ok(output) if output.status.success() => {
            let version = command_version(&output);
            let satisfied = version
                .as_deref()
                .is_some_and(|version| version_at_least(version, &loaded.plan.nix.minimum_version));
            (true, version, satisfied)
        }
        _ => (false, None, false),
    };
    let config_output = command_output(&["nix", "config", "show", "--json"], loaded.root);
    let (config, config_error) = match config_output {
        Ok(output) if output.status.success() => {
            match serde_json::from_slice::<Value>(&output.stdout) {
                Ok(document) => (Some(document), None),
                Err(error) => (
                    None,
                    Some(format!("nix config returned invalid JSON: {error}")),
                ),
            }
        }
        Ok(output) => (
            None,
            Some(command_failure("nix config show --json failed", &output)),
        ),
        Err(error) => (None, Some(error.0)),
    };
    let settings = loaded
        .plan
        .nix
        .settings
        .iter()
        .map(|(name, expected)| inspect_setting(config.as_ref(), name, expected))
        .collect::<Vec<_>>();
    let tools: BTreeMap<_, _> = loaded
        .plan
        .tools
        .iter()
        .map(|(name, expectation)| (name.clone(), inspect_tool(expectation, loaded.root)))
        .collect();
    let declarative = loaded.target.declarative_reason.is_some();
    let readiness = include_readiness.then(|| diagnose_readiness(loaded));
    let compliant = installed
        && version_satisfied
        && config.is_some()
        && settings.iter().all(|setting| setting.satisfied)
        && tools.values().all(|tool| tool.satisfied)
        && readiness
            .as_ref()
            .is_none_or(|readiness| readiness.satisfied);
    DiagnosticReport {
        api_version: SUPPORTED_API_VERSION,
        kind: "HostDiagnostics",
        interface_version: SUPPORTED_INTERFACE_VERSION,
        compliant,
        plan: loaded.path.clone(),
        configuration: ConfigurationDiagnostic {
            path: loaded.target.path.clone(),
            mode: loaded.target.mode,
            exists: loaded.target.path.exists(),
            declarative,
            declarative_reason: loaded.target.declarative_reason.clone(),
        },
        nix: NixDiagnostic {
            installed,
            version,
            minimum_version: loaded.plan.nix.minimum_version.clone(),
            version_satisfied,
            config_available: config.is_some(),
            config_error,
            settings,
        },
        tools,
        readiness,
        security_warnings: security_warnings(&loaded.plan),
    }
}

fn diagnose_readiness(loaded: &Loaded<'_>) -> ReadinessDiagnostic {
    let storage_path = resolve(loaded.root, loaded.plan.readiness.storage.path.clone());
    let (available_bytes, storage_error) = match fs4::available_space(&storage_path) {
        Ok(bytes) => (Some(bytes), None),
        Err(error) => (
            None,
            Some(format!(
                "cannot inspect available space at {}: {error}",
                storage_path.display()
            )),
        ),
    };
    let storage = StorageDiagnostic {
        path: storage_path,
        minimum_available_bytes: loaded.plan.readiness.storage.minimum_available_bytes,
        available_bytes,
        satisfied: available_bytes.is_some_and(|available| {
            available >= loaded.plan.readiness.storage.minimum_available_bytes
        }),
        error: storage_error,
    };

    let daemon_required = match loaded.plan.readiness.daemon.when {
        DaemonWhen::Always => true,
        DaemonWhen::DaemonMode => matches!(loaded.target.mode, EffectiveMode::Daemon),
        DaemonWhen::Never => false,
    };
    let (daemon_checked, daemon_satisfied, daemon_error) = if daemon_required {
        match command_output_strings(&loaded.plan.readiness.daemon.probe_argv, loaded.root) {
            Ok(output) if output.status.success() => (true, true, None),
            Ok(output) => (
                true,
                false,
                Some(command_failure("daemon liveness probe failed", &output)),
            ),
            Err(error) => (true, false, Some(error.0)),
        }
    } else {
        (false, true, None)
    };
    let daemon = DaemonDiagnostic {
        required: daemon_required,
        checked: daemon_checked,
        probe_argv: loaded.plan.readiness.daemon.probe_argv.clone(),
        satisfied: daemon_satisfied,
        error: daemon_error,
    };

    let cache = diagnose_cache(&loaded.plan.readiness.cache, loaded.root);

    let required = loaded
        .plan
        .readiness
        .required_documents
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut documents = BTreeMap::new();
    let mut source = None;
    let mut launch = None;
    for kind in required {
        let (path, result): (Option<PathBuf>, std::result::Result<(), String>) = match kind {
            DocumentKind::Index => {
                let path = loaded
                    .index
                    .clone()
                    .or_else(|| env::var_os("NIXSPACE_INDEX").map(PathBuf::from))
                    .map(|path| resolve(loaded.root, path));
                let result = path
                    .as_deref()
                    .ok_or_else(|| {
                        "workspace index path is not configured by Nix or the CLI".to_owned()
                    })
                    .and_then(|path| crate::load_index(path).map(|_| ()).map_err(|error| error.0));
                (path, result)
            }
            DocumentKind::Source => {
                let path = configured_plan_path(
                    loaded.root,
                    loaded.source_plan.clone(),
                    "NIXSPACE_SOURCE_PLAN",
                );
                let result = match crate::source::readiness(
                    loaded.root,
                    loaded.source_plan.clone(),
                    &loaded.plan.readiness.source_selection,
                ) {
                    Ok(diagnostic) => {
                        source = Some(diagnostic);
                        Ok(())
                    }
                    Err(error) => Err(error.0),
                };
                (path, result)
            }
            DocumentKind::Launch => {
                let path = configured_plan_path(
                    loaded.root,
                    loaded.launch_plan.clone(),
                    "NIXSPACE_LAUNCH_PLAN",
                );
                let expectation = &loaded.plan.readiness.launch;
                let result = match crate::launch_exec::readiness(
                    loaded.root,
                    loaded.launch_plan.clone(),
                    expectation.allow_active_sessions,
                    expectation.require_manager_socket,
                    expectation.require_available_declared_ports,
                ) {
                    Ok(diagnostic) => {
                        launch = Some(diagnostic);
                        Ok(())
                    }
                    Err(error) => Err(error.0),
                };
                (path, result)
            }
            DocumentKind::Resolution => {
                let path = configured_plan_path(
                    loaded.root,
                    loaded.resolution_plan.clone(),
                    "NIXSPACE_RESOLUTION_PLAN",
                );
                let result =
                    crate::resolution::readiness(loaded.root, loaded.resolution_plan.clone())
                        .map_err(|error| error.0);
                (path, result)
            }
        };
        let (ready, error) = match result {
            Ok(()) => (true, None),
            Err(error) => (false, Some(error)),
        };
        documents.insert(
            kind.name().to_owned(),
            DocumentDiagnostic { path, ready, error },
        );
    }
    let satisfied = storage.satisfied
        && daemon.satisfied
        && documents.values().all(|document| document.ready)
        && source.as_ref().is_none_or(|source| source.satisfied)
        && launch.as_ref().is_none_or(|launch| launch.satisfied);
    ReadinessDiagnostic {
        satisfied,
        storage,
        daemon,
        cache,
        documents,
        source,
        launch,
    }
}

fn diagnose_cache(expectation: &CacheExpectation, root: &Path) -> CacheDiagnostic {
    let roots = expectation
        .roots
        .iter()
        .map(|expected_root| {
            diagnose_cache_root(
                expected_root,
                &expectation.store_directory,
                &expectation.stores,
                root,
            )
        })
        .collect();
    CacheDiagnostic {
        coverage_mode: expectation.coverage_mode,
        roots,
    }
}

fn diagnose_cache_root(
    expectation: &CacheRootExpectation,
    store_directory: &Path,
    stores: &[CacheStoreExpectation],
    root: &Path,
) -> CacheRootDiagnostic {
    let path = resolve(root, expectation.path.clone());
    let unavailable = |error: &str| CacheRootDiagnostic {
        name: expectation.name.clone(),
        path: path.clone(),
        store_path: None,
        observed: false,
        path_count: 0,
        nar_bytes: 0,
        union: None,
        stores: Vec::new(),
        error: Some(error.to_owned()),
    };
    // A routine doctor invocation must stay offline and must never evaluate or
    // realize a flake output just to obtain cache statistics. The generated
    // contract therefore names an existing store path (or an out-link to one),
    // and a missing path is reported without contacting any cache.
    if !path.exists() {
        return unavailable("cache root is not realized on this host");
    }
    let canonical_store_directory = match fs::canonicalize(store_directory) {
        Ok(directory) => directory,
        Err(_) => return unavailable("the declared Nix store directory is unavailable"),
    };
    let store_path = match fs::canonicalize(&path) {
        Ok(store_path)
            if store_path.parent() == Some(canonical_store_directory.as_path())
                && conventional_store_path_name(&store_path) =>
        {
            store_path
        }
        Ok(_) => return unavailable("cache root does not resolve to a direct Nix store path"),
        Err(_) => return unavailable("cache root cannot be resolved on this host"),
    };
    let path_text = store_path.to_string_lossy().into_owned();
    let local_arguments = vec![
        "nix".to_owned(),
        "path-info".to_owned(),
        "--json".to_owned(),
        "--recursive".to_owned(),
        "--size".to_owned(),
        path_text,
    ];
    let local_output = match command_output_strings(&local_arguments, root) {
        Ok(output) if output.status.success() => output,
        Ok(_) => return unavailable("Nix could not inspect the realized cache root"),
        Err(_) => return unavailable("Nix is unavailable while inspecting the cache root"),
    };
    let local = match parse_path_info(&local_output.stdout) {
        Ok(entries) => entries,
        Err(_) => return unavailable("Nix returned malformed cache-root path information"),
    };
    if local.is_empty() || local.values().any(Option::is_none) {
        return unavailable("Nix did not return a complete realized cache-root closure");
    }
    let path_count = match u64::try_from(local.len()) {
        Ok(count) => count,
        Err(_) => return unavailable("cache-root closure contains too many paths to report"),
    };
    let nar_bytes = match checked_nar_sum(
        local
            .values()
            .filter_map(|measurement| measurement.map(|value| value.nar_size)),
    ) {
        Some(bytes) => bytes,
        None => return unavailable("cache-root NAR byte total overflowed"),
    };
    let input = local.keys().fold(String::new(), |mut input, path| {
        input.push_str(path);
        input.push('\n');
        input
    });
    let store_observations = stores
        .iter()
        .map(|store| diagnose_cache_store(store, &local, &input, root))
        .collect::<Vec<_>>();
    let union = diagnose_cache_union(&local, &store_observations);
    let store_diagnostics = store_observations
        .into_iter()
        .map(|observation| observation.diagnostic)
        .collect();
    CacheRootDiagnostic {
        name: expectation.name.clone(),
        path,
        store_path: Some(store_path),
        observed: true,
        path_count,
        nar_bytes,
        union: Some(union),
        stores: store_diagnostics,
        error: None,
    }
}

fn conventional_store_path_name(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    let bytes = name.as_bytes();
    bytes.len() > 33
        && bytes[32] == b'-'
        && bytes[..32].iter().all(|byte| {
            matches!(
                byte,
                b'0'..=b'9'
                    | b'a'
                    | b'b'
                    | b'c'
                    | b'd'
                    | b'f'
                    | b'g'
                    | b'h'
                    | b'i'
                    | b'j'
                    | b'k'
                    | b'l'
                    | b'm'
                    | b'n'
                    | b'p'
                    | b'q'
                    | b'r'
                    | b's'
                    | b'v'
                    | b'w'
                    | b'x'
                    | b'y'
                    | b'z'
            )
        })
}

fn diagnose_cache_store(
    expectation: &CacheStoreExpectation,
    local: &BTreeMap<String, Option<PathInfoMeasurement>>,
    input: &str,
    root: &Path,
) -> CacheStoreObservation {
    let unavailable = |error: &str| CacheStoreObservation {
        diagnostic: CacheStoreDiagnostic {
            name: expectation.name.clone(),
            uri: expectation.uri.clone(),
            observed: false,
            hit_path_count: 0,
            miss_path_count: 0,
            hit_nar_bytes: 0,
            miss_nar_bytes: 0,
            cache_download_bytes: None,
            transferred_bytes: None,
            error: Some(error.to_owned()),
        },
        hit_download_bytes: BTreeMap::new(),
    };
    let arguments = vec![
        "nix".to_owned(),
        "path-info".to_owned(),
        "--json".to_owned(),
        "--size".to_owned(),
        "--store".to_owned(),
        expectation.uri.clone(),
        "--stdin".to_owned(),
    ];
    let output = match command_output_with_stdin(&arguments, input.as_bytes(), root) {
        Ok(output) if output.status.success() => output,
        Ok(_) => return unavailable("cache path query failed"),
        Err(_) => return unavailable("cache path query could not start"),
    };
    let remote = match parse_path_info(&output.stdout) {
        Ok(entries) => entries,
        Err(_) => return unavailable("cache returned malformed path information"),
    };
    if remote.len() != local.len() || remote.keys().any(|path| !local.contains_key(path)) {
        return unavailable("cache returned an incomplete or unexpected path set");
    }
    let mut hit_path_count = 0_u64;
    let mut miss_path_count = 0_u64;
    let mut hit_nar_bytes = 0_u64;
    let mut miss_nar_bytes = 0_u64;
    let mut cache_download_bytes = Some(0_u64);
    let mut hit_download_bytes = BTreeMap::new();
    for (path, local_size) in local {
        let local_size = local_size
            .expect("local closure was checked above")
            .nar_size;
        match remote.get(path) {
            Some(Some(remote_measurement)) if remote_measurement.nar_size == local_size => {
                hit_path_count = match hit_path_count.checked_add(1) {
                    Some(count) => count,
                    None => return unavailable("cache hit count overflowed"),
                };
                hit_nar_bytes = match hit_nar_bytes.checked_add(local_size) {
                    Some(bytes) => bytes,
                    None => return unavailable("cache hit NAR byte total overflowed"),
                };
                cache_download_bytes =
                    match (cache_download_bytes, remote_measurement.download_size) {
                        (Some(total), Some(bytes)) => match total.checked_add(bytes) {
                            Some(total) => Some(total),
                            None => return unavailable("cache hit download byte total overflowed"),
                        },
                        _ => None,
                    };
                hit_download_bytes.insert(path.clone(), remote_measurement.download_size);
            }
            Some(None) => {
                miss_path_count = match miss_path_count.checked_add(1) {
                    Some(count) => count,
                    None => return unavailable("cache miss count overflowed"),
                };
                miss_nar_bytes = match miss_nar_bytes.checked_add(local_size) {
                    Some(bytes) => bytes,
                    None => return unavailable("cache miss NAR byte total overflowed"),
                };
            }
            Some(Some(_)) => return unavailable("cache returned an inconsistent NAR size"),
            None => return unavailable("cache returned an incomplete or unexpected path set"),
        }
    }
    CacheStoreObservation {
        diagnostic: CacheStoreDiagnostic {
            name: expectation.name.clone(),
            uri: expectation.uri.clone(),
            observed: true,
            hit_path_count,
            miss_path_count,
            hit_nar_bytes,
            miss_nar_bytes,
            cache_download_bytes,
            // There is no stable Nix JSON field for compressed bytes actually
            // transferred. Never relabel NAR sizes as transport measurements.
            transferred_bytes: None,
            error: None,
        },
        hit_download_bytes,
    }
}

fn diagnose_cache_union(
    local: &BTreeMap<String, Option<PathInfoMeasurement>>,
    stores: &[CacheStoreObservation],
) -> CacheUnionDiagnostic {
    let observed_store_count = stores
        .iter()
        .filter(|store| store.diagnostic.observed)
        .count()
        .try_into()
        .expect("an in-memory store count fits in u64");
    let mut hit_path_count = 0_u64;
    let mut miss_path_count = 0_u64;
    let mut hit_nar_bytes = 0_u64;
    let mut miss_nar_bytes = 0_u64;
    let mut cache_download_bytes = Some(0_u64);

    for (path, local_measurement) in local {
        let local_nar_size = local_measurement
            .expect("local closure was checked above")
            .nar_size;
        let mut hit = false;
        let mut least_download_bytes = None;
        for store in stores.iter().filter(|store| store.diagnostic.observed) {
            if let Some(download_bytes) = store.hit_download_bytes.get(path) {
                hit = true;
                if let Some(download_bytes) = download_bytes {
                    least_download_bytes = Some(
                        least_download_bytes
                            .map_or(*download_bytes, |least: u64| least.min(*download_bytes)),
                    );
                }
            }
        }

        if hit {
            hit_path_count = hit_path_count
                .checked_add(1)
                .expect("a local closure path count fits in u64");
            hit_nar_bytes = hit_nar_bytes
                .checked_add(local_nar_size)
                .expect("the checked local NAR total bounds union hit bytes");
            cache_download_bytes = match (cache_download_bytes, least_download_bytes) {
                (Some(total), Some(bytes)) => total.checked_add(bytes),
                _ => None,
            };
        } else {
            miss_path_count = miss_path_count
                .checked_add(1)
                .expect("a local closure path count fits in u64");
            miss_nar_bytes = miss_nar_bytes
                .checked_add(local_nar_size)
                .expect("the checked local NAR total bounds union miss bytes");
        }
    }

    CacheUnionDiagnostic {
        observed_store_count,
        hit_path_count,
        miss_path_count,
        hit_nar_bytes,
        miss_nar_bytes,
        cache_download_bytes,
        transferred_bytes: None,
    }
}

fn checked_nar_sum(mut values: impl Iterator<Item = u64>) -> Option<u64> {
    values.try_fold(0_u64, u64::checked_add)
}

fn parse_path_info(
    bytes: &[u8],
) -> std::result::Result<BTreeMap<String, Option<PathInfoMeasurement>>, ()> {
    let document: Value = serde_json::from_slice(bytes).map_err(|_| ())?;
    let object = document.as_object().ok_or(())?;
    if object.get("version").and_then(Value::as_u64) == Some(2) {
        let store_dir = object.get("storeDir").and_then(Value::as_str).ok_or(())?;
        let info = object.get("info").and_then(Value::as_object).ok_or(())?;
        info.iter()
            .map(|(base_name, value)| {
                if base_name.is_empty()
                    || base_name.contains(['\0', '\n', '\r'])
                    || base_name.contains('/')
                {
                    return Err(());
                }
                let path = format!("{}/{base_name}", store_dir.trim_end_matches('/'));
                Ok((path, path_info_measurement(value)?))
            })
            .collect()
    } else {
        object
            .iter()
            .map(|(path, value)| {
                if !Path::new(path).is_absolute() || path.contains(['\0', '\n', '\r']) {
                    return Err(());
                }
                Ok((path.clone(), path_info_measurement(value)?))
            })
            .collect()
    }
}

fn path_info_measurement(value: &Value) -> std::result::Result<Option<PathInfoMeasurement>, ()> {
    if value.is_null() {
        Ok(None)
    } else {
        let info = value.as_object().ok_or(())?;
        let nar_size = info.get("narSize").and_then(Value::as_u64).ok_or(())?;
        let download_size = match info.get("downloadSize") {
            Some(value) => Some(value.as_u64().ok_or(())?),
            None => None,
        };
        Ok(Some(PathInfoMeasurement {
            nar_size,
            download_size,
        }))
    }
}

fn configured_plan_path(root: &Path, explicit: Option<PathBuf>, variable: &str) -> Option<PathBuf> {
    explicit
        .or_else(|| env::var_os(variable).map(PathBuf::from))
        .map(|path| resolve(root, path))
}

fn inspect_setting(config: Option<&Value>, name: &str, expected: &Value) -> SettingDiagnostic {
    let direct = config.and_then(|document| config_value(document, name));
    let (actual_name, actual) = if direct.is_some() {
        (name.to_owned(), direct)
    } else if let Some(base) = name.strip_prefix("extra-") {
        (
            base.to_owned(),
            config.and_then(|document| config_value(document, base)),
        )
    } else {
        (name.to_owned(), None)
    };
    let satisfied = actual
        .as_ref()
        .is_some_and(|actual| expectation_satisfied(expected, actual));
    SettingDiagnostic {
        name: name.to_owned(),
        actual_name,
        expected: expected.clone(),
        actual,
        satisfied,
    }
}

fn config_value(document: &Value, name: &str) -> Option<Value> {
    let entry = document.as_object()?.get(name)?;
    entry
        .as_object()
        .and_then(|entry| entry.get("value"))
        .cloned()
}

fn expectation_satisfied(expected: &Value, actual: &Value) -> bool {
    match (expected.as_array(), actual.as_array()) {
        (Some(expected), Some(actual)) => expected.iter().all(|item| actual.contains(item)),
        _ => expected == actual,
    }
}

fn inspect_tool(expectation: &ToolExpectation, root: &Path) -> ToolDiagnostic {
    match command_output_strings(&expectation.version_argv, root) {
        Ok(output) if output.status.success() => {
            let version = tool_version(&output, &expectation.expected_version);
            let satisfied = version.as_deref() == Some(expectation.expected_version.as_str());
            ToolDiagnostic {
                executable: expectation.executable.clone(),
                expected_version: expectation.expected_version.clone(),
                installed: true,
                version,
                satisfied,
                error: None,
            }
        }
        Ok(output) => ToolDiagnostic {
            executable: expectation.executable.clone(),
            expected_version: expectation.expected_version.clone(),
            installed: true,
            version: tool_version(&output, &expectation.expected_version),
            satisfied: false,
            error: Some(command_failure("version command failed", &output)),
        },
        Err(error) => ToolDiagnostic {
            executable: expectation.executable.clone(),
            expected_version: expectation.expected_version.clone(),
            installed: false,
            version: None,
            satisfied: false,
            error: Some(error.0),
        },
    }
}

fn command_version(output: &Output) -> Option<String> {
    command_text(output)
        .split_whitespace()
        .find(|token| parse_version(token).is_some())
        .map(str::to_owned)
}

fn tool_version(output: &Output, expected: &str) -> Option<String> {
    let text = command_text(output);
    let mut tokens = text.split_whitespace();
    if text.split_whitespace().any(|token| token == expected) {
        return Some(expected.to_owned());
    }
    tokens
        .find(|token| parse_version(token).is_some())
        .or_else(|| text.split_whitespace().last())
        .map(str::to_owned)
}

fn command_text(output: &Output) -> std::borrow::Cow<'_, str> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stdout.trim().is_empty() {
        stderr
    } else {
        stdout
    }
}

fn version_at_least(actual: &str, minimum: &str) -> bool {
    match (parse_version(actual), parse_version(minimum)) {
        (Some(mut actual), Some(mut minimum)) => {
            let length = actual.len().max(minimum.len());
            actual.resize(length, 0);
            minimum.resize(length, 0);
            actual >= minimum
        }
        _ => false,
    }
}

fn parse_version(value: &str) -> Option<Vec<u64>> {
    if value.is_empty() {
        return None;
    }
    value
        .split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect()
}

fn command_output(arguments: &[&str], cwd: &Path) -> Result<Output> {
    let owned: Vec<_> = arguments.iter().map(|value| (*value).to_owned()).collect();
    command_output_strings(&owned, cwd)
}

fn command_output_strings(arguments: &[String], cwd: &Path) -> Result<Output> {
    let executable = arguments
        .first()
        .ok_or_else(|| CliError("cannot execute an empty argv".into()))?;
    Command::new(executable)
        .args(&arguments[1..])
        .current_dir(cwd)
        .output()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                CliError(format!("`{executable}` is unavailable on PATH"))
            } else {
                CliError(format!("cannot execute `{executable}`: {error}"))
            }
        })
}

fn command_output_with_stdin(arguments: &[String], input: &[u8], cwd: &Path) -> Result<Output> {
    let executable = arguments
        .first()
        .ok_or_else(|| CliError("cannot execute an empty argv".into()))?;
    let mut child = Command::new(executable)
        .args(&arguments[1..])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                CliError(format!("`{executable}` is unavailable on PATH"))
            } else {
                CliError(format!("cannot execute `{executable}`: {error}"))
            }
        })?;
    child
        .stdin
        .take()
        .ok_or_else(|| CliError("cannot open cache query standard input".into()))?
        .write_all(input)
        .map_err(|error| CliError(format!("cannot write cache query standard input: {error}")))?;
    child
        .wait_with_output()
        .map_err(|error| CliError(format!("cannot wait for cache query: {error}")))
}

fn run_streaming(
    arguments: &[String],
    cwd: &Path,
    purpose: &str,
) -> Result<std::process::ExitStatus> {
    let executable = arguments
        .first()
        .ok_or_else(|| CliError(format!("{purpose} argv is empty")))?;
    Command::new(executable)
        .args(&arguments[1..])
        .current_dir(cwd)
        .status()
        .map_err(|error| CliError(format!("cannot start {purpose} `{executable}`: {error}")))
}

fn command_failure(message: &str, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        format!("{message} with status {}", output.status)
    } else {
        format!("{message}: {stderr}")
    }
}

fn security_warnings(plan: &HostPlan) -> Vec<String> {
    let trusted_all = plan
        .nix
        .settings
        .get("trusted-users")
        .and_then(Value::as_array)
        .is_some_and(|users| users.iter().any(|user| user.as_str() == Some("*")));
    if trusted_all {
        vec![
            "trusted-users = * grants every local user root-equivalent control through the Nix daemon"
                .into(),
        ]
    } else {
        Vec::new()
    }
}

fn emit_security_warnings(warnings: &[String]) {
    for warning in warnings {
        eprintln!("WARNING: {warning}");
    }
}

fn emit_report(report: &DiagnosticReport, json: bool) -> Result<()> {
    if json {
        return write_json(report);
    }
    println!(
        "Nix: {} (required >= {})",
        report.nix.version.as_deref().unwrap_or("unavailable"),
        report.nix.minimum_version
    );
    println!(
        "Configuration: {} ({:?})",
        report.configuration.path.display(),
        report.configuration.mode
    );
    for setting in &report.nix.settings {
        println!(
            "{} {}",
            if setting.satisfied { "ok" } else { "missing" },
            setting.name
        );
    }
    for (name, tool) in &report.tools {
        println!(
            "{} tool {}: {} (expected {})",
            if tool.satisfied { "ok" } else { "missing" },
            name,
            tool.version.as_deref().unwrap_or("unavailable"),
            tool.expected_version
        );
    }
    if let Some(readiness) = &report.readiness {
        println!(
            "{} storage: {} available (minimum {}) at {}",
            if readiness.storage.satisfied {
                "ok"
            } else {
                "low"
            },
            readiness
                .storage
                .available_bytes
                .map(|bytes| bytes.to_string())
                .as_deref()
                .unwrap_or("unavailable"),
            readiness.storage.minimum_available_bytes,
            readiness.storage.path.display()
        );
        println!(
            "{} daemon liveness",
            if readiness.daemon.satisfied {
                "ok"
            } else {
                "failed"
            }
        );
        emit_cache_diagnostic(&readiness.cache);
        for (name, document) in &readiness.documents {
            println!(
                "{} {} plan: {}",
                if document.ready { "ok" } else { "missing" },
                name,
                document
                    .path
                    .as_deref()
                    .map(|path| path.display().to_string())
                    .as_deref()
                    .unwrap_or("unconfigured")
            );
        }
        if let Some(source) = &readiness.source {
            let missing = source
                .repositories
                .iter()
                .filter(|repository| !repository.satisfied)
                .count();
            println!(
                "{} sources: {} selected, {} not ready",
                if source.satisfied { "ok" } else { "failed" },
                source.repositories.len(),
                missing
            );
        }
        if let Some(launch) = &readiness.launch {
            let occupied = launch.ports.iter().filter(|port| !port.available).count();
            println!(
                "{} launches: {} sessions, {} occupied declared ports",
                if launch.satisfied { "ok" } else { "failed" },
                launch.sessions.len(),
                occupied
            );
        }
    }
    emit_security_warnings(&report.security_warnings);
    println!(
        "Host: {}",
        if report.compliant {
            "compliant"
        } else {
            "not compliant"
        }
    );
    Ok(())
}

fn emit_cache_diagnostic(cache: &CacheDiagnostic) {
    for root in &cache.roots {
        if !root.observed {
            println!(
                "skip cache root {}: {}",
                root.name,
                root.error.as_deref().unwrap_or("not observed")
            );
            continue;
        }
        println!(
            "cache root {}: {} paths, {} NAR bytes",
            root.name, root.path_count, root.nar_bytes
        );
        if let Some(union) = &root.union {
            println!(
                "cache union: {} observed stores; {} hit paths / {} miss paths; {} hit NAR bytes / {} miss NAR bytes; {} cache download bytes; transferred bytes unavailable",
                union.observed_store_count,
                union.hit_path_count,
                union.miss_path_count,
                union.hit_nar_bytes,
                union.miss_nar_bytes,
                union
                    .cache_download_bytes
                    .map(|bytes| bytes.to_string())
                    .as_deref()
                    .unwrap_or("unavailable")
            );
        }
        for store in &root.stores {
            if store.observed {
                println!(
                    "cache {}: {} hit paths / {} miss paths; {} hit NAR bytes / {} miss NAR bytes; {} cache download bytes; transferred bytes unavailable",
                    store.name,
                    store.hit_path_count,
                    store.miss_path_count,
                    store.hit_nar_bytes,
                    store.miss_nar_bytes,
                    store
                        .cache_download_bytes
                        .map(|bytes| bytes.to_string())
                        .as_deref()
                        .unwrap_or("unavailable")
                );
            } else {
                println!(
                    "cache {}: unavailable ({})",
                    store.name,
                    store.error.as_deref().unwrap_or("not observed")
                );
            }
        }
    }
}

fn write_cache_report(path: &Path, report: &CacheCoverageReport) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| CliError(format!("cannot serialize cache coverage report: {error}")))?;
    bytes.push(b'\n');
    let parent = path.parent().ok_or_else(|| {
        CliError(format!(
            "cache coverage output path has no parent: {}",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        CliError(format!(
            "cannot create cache coverage output directory {}: {error}",
            parent.display()
        ))
    })?;
    atomic_write_output(path, &bytes, ".nixspace-cache-coverage")
}

fn append_cache_summary(path: &Path, report: &CacheCoverageReport) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        CliError(format!(
            "cache coverage summary path has no parent: {}",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        CliError(format!(
            "cannot create cache coverage summary directory {}: {error}",
            parent.display()
        ))
    })?;
    let mut summary = String::from(
        "## Nix cache coverage\n\n| Root | Cache | Hit paths | Miss paths | Hit NAR bytes | Miss NAR bytes | Cache download bytes | Transferred bytes |\n|---|---|---:|---:|---:|---:|---:|---|\n",
    );
    for root in &report.cache.roots {
        if !root.observed {
            summary.push_str(&format!(
                "| {} | — | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable |\n",
                root.name
            ));
            continue;
        }
        if let Some(union) = &root.union {
            summary.push_str(&format!(
                "| {} | union ({} observed) | {} | {} | {} | {} | {} | unavailable |\n",
                root.name,
                union.observed_store_count,
                union.hit_path_count,
                union.miss_path_count,
                union.hit_nar_bytes,
                union.miss_nar_bytes,
                union
                    .cache_download_bytes
                    .map(|bytes| bytes.to_string())
                    .as_deref()
                    .unwrap_or("unavailable")
            ));
        }
        for store in &root.stores {
            if store.observed {
                summary.push_str(&format!(
                    "| {} | {} | {} | {} | {} | {} | {} | unavailable |\n",
                    root.name,
                    store.name,
                    store.hit_path_count,
                    store.miss_path_count,
                    store.hit_nar_bytes,
                    store.miss_nar_bytes,
                    store
                        .cache_download_bytes
                        .map(|bytes| bytes.to_string())
                        .as_deref()
                        .unwrap_or("unavailable")
                ));
            } else {
                summary.push_str(&format!(
                    "| {} | {} | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable |\n",
                    root.name, store.name
                ));
            }
        }
    }
    summary.push_str(&format!(
        "\nCoverage result: **{}**. Union `cacheDownloadBytes` sums each covered path once, using the least `downloadSize` advertised by any observed store for that path; it is unavailable when a covered path has no advertised size. `transferredBytes` is unavailable because this command inventories the cache; it does not observe an earlier transfer.\n\n",
        if report.complete { "complete" } else { "incomplete" }
    ));
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(summary.as_bytes()))
        .map_err(|error| {
            CliError(format!(
                "cannot append cache coverage summary {}: {error}",
                path.display()
            ))
        })
}

fn render_configuration(settings: &BTreeMap<String, Value>) -> Result<String> {
    let mut rendered = String::new();
    rendered.push_str(MANAGED_BEGIN);
    rendered.push('\n');
    for (name, value) in settings {
        let value = match value {
            Value::Bool(value) => value.to_string(),
            Value::Number(value) => value.to_string(),
            Value::String(value) => value.clone(),
            Value::Array(values) => values
                .iter()
                .map(|value| value.as_str().expect("validated string array"))
                .collect::<Vec<_>>()
                .join(" "),
            _ => unreachable!("validated setting type"),
        };
        rendered.push_str(name);
        rendered.push_str(" = ");
        rendered.push_str(&value);
        rendered.push('\n');
    }
    rendered.push_str(MANAGED_END);
    rendered.push('\n');
    Ok(rendered)
}

fn merge_managed_block(existing: Option<&str>, block: &str) -> Result<String> {
    let existing = existing.unwrap_or_default();
    let begin = marker_lines(existing, MANAGED_BEGIN);
    let end = marker_lines(existing, MANAGED_END);
    match (begin.as_slice(), end.as_slice()) {
        ([], []) => {
            let mut merged = existing.to_owned();
            if !merged.is_empty() && !merged.ends_with('\n') {
                merged.push('\n');
            }
            if !merged.is_empty() && !merged.ends_with("\n\n") {
                merged.push('\n');
            }
            merged.push_str(block);
            Ok(merged)
        }
        ([(begin, _)], [(end, suffix)]) if begin < end => {
            let mut merged = existing[..*begin].to_owned();
            merged.push_str(block);
            merged.push_str(&existing[*suffix..]);
            Ok(merged)
        }
        _ => Err(CliError(format!(
            "Nix configuration has malformed or duplicate `{MANAGED_BEGIN}` / `{MANAGED_END}` markers"
        ))),
    }
}

fn marker_lines(contents: &str, marker: &str) -> Vec<(usize, usize)> {
    let mut offset = 0;
    let mut matches = Vec::new();
    for line in contents.split_inclusive('\n') {
        let value = line.strip_suffix('\n').unwrap_or(line);
        let value = value.strip_suffix('\r').unwrap_or(value);
        if value == marker {
            matches.push((offset, offset + line.len()));
        }
        offset += line.len();
    }
    matches
}

fn read_optional(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CliError(format!(
            "cannot read Nix configuration at {}: {error}",
            path.display()
        ))),
    }
}

fn install_configuration(target: &Target, contents: &str) -> Result<()> {
    if target.system_config && !effective_root() {
        install_system_configuration(target, contents)
    } else {
        install_user_configuration(target, contents)
    }
}

fn install_user_configuration(target: &Target, contents: &str) -> Result<()> {
    let parent = target.path.parent().ok_or_else(|| {
        CliError(format!(
            "Nix configuration path has no parent: {}",
            target.path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        CliError(format!(
            "cannot create Nix configuration directory {}: {error}",
            parent.display()
        ))
    })?;
    backup_direct(&target.path)?;
    atomic_write(&target.path, contents.as_bytes())
}

fn install_system_configuration(target: &Target, contents: &str) -> Result<()> {
    let staged = temporary_in(env::temp_dir(), "nixspace-nix-conf")?;
    let mut guard = TemporaryFile(Some(staged.clone()));
    fs::write(&staged, contents).map_err(|error| {
        CliError(format!(
            "cannot stage Nix configuration at {}: {error}",
            staged.display()
        ))
    })?;
    let parent = target.path.parent().expect("system nix.conf has parent");
    run_privileged([OsStr::new("mkdir"), OsStr::new("-p"), parent.as_os_str()])?;
    let backup = backup_path(&target.path);
    if target.path.exists() && !path_lexists(&backup) {
        run_privileged([
            OsStr::new("cp"),
            OsStr::new("-p"),
            OsStr::new("--"),
            target.path.as_os_str(),
            backup.as_os_str(),
        ])?;
    }
    run_privileged([
        OsStr::new("install"),
        OsStr::new("-m"),
        OsStr::new("0644"),
        staged.as_os_str(),
        target.path.as_os_str(),
    ])?;
    guard.0 = None;
    let _ = fs::remove_file(&staged);
    Ok(())
}

fn backup_direct(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let backup = backup_path(path);
    if path_lexists(&backup) {
        return Ok(());
    }
    fs::copy(path, &backup).map_err(|error| {
        CliError(format!(
            "cannot back up Nix configuration {} to {}: {error}",
            path.display(),
            backup.display()
        ))
    })?;
    let permissions = fs::metadata(path)
        .and_then(|metadata| fs::set_permissions(&backup, metadata.permissions()));
    permissions.map_err(|error| {
        CliError(format!(
            "cannot preserve Nix configuration backup permissions: {error}"
        ))
    })
}

fn backup_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(".pre-nixspace");
    PathBuf::from(value)
}

fn atomic_write_output(destination: &Path, bytes: &[u8], prefix: &str) -> Result<()> {
    let parent = destination.parent().ok_or_else(|| {
        CliError(format!(
            "output path has no parent: {}",
            destination.display()
        ))
    })?;
    let temporary = temporary_in(parent, prefix)?;
    let mut guard = TemporaryFile(Some(temporary.clone()));
    let mut file = OpenOptions::new()
        .write(true)
        .open(&temporary)
        .map_err(|error| CliError(format!("cannot open temporary output: {error}")))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| CliError(format!("cannot write temporary output: {error}")))?;
    drop(file);
    fs::rename(&temporary, destination).map_err(|error| {
        CliError(format!(
            "cannot atomically replace output {}: {error}",
            destination.display()
        ))
    })?;
    guard.0 = None;
    Ok(())
}

fn atomic_write(destination: &Path, bytes: &[u8]) -> Result<()> {
    let parent = destination.parent().expect("configuration path has parent");
    let temporary = temporary_in(parent, ".nixspace-nix-conf")?;
    let mut guard = TemporaryFile(Some(temporary.clone()));
    let mut file = OpenOptions::new()
        .write(true)
        .open(&temporary)
        .map_err(|error| CliError(format!("cannot open temporary nix.conf: {error}")))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| CliError(format!("cannot write temporary nix.conf: {error}")))?;
    let existing_permissions = fs::metadata(destination)
        .ok()
        .map(|metadata| metadata.permissions());
    set_configuration_permissions(&file, existing_permissions)?;
    drop(file);
    fs::rename(&temporary, destination).map_err(|error| {
        CliError(format!(
            "cannot atomically replace Nix configuration {}: {error}",
            destination.display()
        ))
    })?;
    guard.0 = None;
    Ok(())
}

fn temporary_in(parent: impl AsRef<Path>, prefix: &str) -> Result<PathBuf> {
    let parent = parent.as_ref();
    for _ in 0..128 {
        let sequence = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!("{prefix}.{}.{}.tmp", std::process::id(), sequence));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(CliError(format!(
                    "cannot create temporary file in {}: {error}",
                    parent.display()
                )))
            }
        }
    }
    Err(CliError(format!(
        "cannot allocate a temporary file in {}",
        parent.display()
    )))
}

#[cfg(unix)]
fn set_configuration_permissions(file: &File, existing: Option<fs::Permissions>) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(existing.unwrap_or_else(|| fs::Permissions::from_mode(0o644)))
        .map_err(|error| CliError(format!("cannot set nix.conf permissions: {error}")))
}

#[cfg(not(unix))]
fn set_configuration_permissions(_file: &File, _existing: Option<fs::Permissions>) -> Result<()> {
    Ok(())
}

fn path_lexists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn restart_daemon(root: &Path) -> Result<()> {
    let arguments: Vec<OsString> = if cfg!(target_os = "macos") {
        [
            "launchctl",
            "kickstart",
            "-k",
            "system/org.nixos.nix-daemon",
        ]
        .into_iter()
        .map(OsString::from)
        .collect()
    } else {
        ["systemctl", "restart", "nix-daemon.service"]
            .into_iter()
            .map(OsString::from)
            .collect()
    };
    println!("Restarting the Nix daemon after configuration mutation...");
    run_privileged_in(&arguments, root)
}

fn run_privileged<I, S>(arguments: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let arguments: Vec<_> = arguments
        .into_iter()
        .map(|argument| argument.as_ref().to_owned())
        .collect();
    run_privileged_in(&arguments, Path::new("."))
}

fn run_privileged_in(arguments: &[OsString], cwd: &Path) -> Result<()> {
    let (executable, trailing): (&OsStr, &[OsString]) = if effective_root() {
        (
            arguments
                .first()
                .ok_or_else(|| CliError("privileged command argv is empty".into()))?,
            &arguments[1..],
        )
    } else {
        (OsStr::new("sudo"), arguments)
    };
    let status = Command::new(executable)
        .args(trailing)
        .current_dir(cwd)
        .status()
        .map_err(|error| CliError(format!("cannot execute privileged command: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError(format!(
            "privileged command failed with status {status}"
        )))
    }
}

#[cfg(unix)]
fn effective_root() -> bool {
    unsafe extern "C" {
        fn geteuid() -> u32;
    }
    // SAFETY: geteuid takes no arguments and has no preconditions.
    unsafe { geteuid() == 0 }
}

#[cfg(not(unix))]
fn effective_root() -> bool {
    false
}

struct TemporaryFile(Option<PathBuf>);

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{merge_managed_block, version_at_least, MANAGED_BEGIN, MANAGED_END};

    #[test]
    fn versions_compare_numerically() {
        assert!(version_at_least("2.18", "2.18"));
        assert!(version_at_least("2.34.7", "2.18"));
        assert!(!version_at_least("2.9", "2.18"));
        assert!(!version_at_least("unstable", "2.18"));
    }

    #[test]
    fn managed_block_replacement_preserves_every_unmanaged_byte() {
        let old =
            format!("keep = one\n\n{MANAGED_BEGIN}\nold = value\n{MANAGED_END}\ntail = two\n");
        let block = format!("{MANAGED_BEGIN}\nnew = value\n{MANAGED_END}\n");
        assert_eq!(
            merge_managed_block(Some(&old), &block).unwrap(),
            format!("keep = one\n\n{block}tail = two\n")
        );
    }
}
