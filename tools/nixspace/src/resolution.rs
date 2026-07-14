use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use clap::Args;
use fs4::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::action::Outcome;
use crate::{write_json, CliError, Result};

const SUPPORTED_API_VERSION: &str = "nixspace/v1";
const SUPPORTED_KIND: &str = "WorkspaceResolution";
const SUPPORTED_INTERFACE_VERSION: u64 = 1;
const GENERATION_POINTER_KIND: &str = "ActionGenerationPointer";
const GENERATION_KIND: &str = "ActionGeneration";
const GENERATION_INTERFACE_VERSION: u64 = 1;

#[derive(Debug, Args)]
pub(crate) struct EnvArgs {
    /// Package whose exact command-scoped resolution should be explained.
    #[arg(value_name = "PACKAGE")]
    package: String,

    /// Explain every Nix-selected dependency candidate and binding.
    #[arg(long, required = true)]
    explain: bool,

    /// Render the explanation as versioned JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PrefixArgs {
    /// Package whose exact selected prefix should be printed.
    #[arg(value_name = "PACKAGE")]
    package: String,

    /// Render the selected prefix and provenance as versioned JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ResourceArgs {
    /// Exact workspace resource coordinate.
    #[arg(value_name = "PACKAGE/RESOURCE")]
    coordinate: String,

    /// Render the selected resource and provenance as versioned JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct RunArgs {
    /// Package plan that owns this command's complete candidate selection.
    #[arg(long, value_name = "PACKAGE")]
    selection_root: Option<String>,

    /// Exact workspace executable coordinate.
    #[arg(value_name = "PACKAGE/EXECUTABLE")]
    coordinate: String,

    /// Arguments appended after the executable's Nix-emitted fixed argv.
    #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
    arguments: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResolutionPlan {
    api_version: String,
    kind: String,
    interface_version: u64,
    roots: ResolutionRoots,
    package_plans: BTreeMap<String, PackagePlan>,
    packages: BTreeMap<String, PackageResolution>,
    artifacts: BTreeMap<String, ArtifactResolution>,
    resources: BTreeMap<String, ResourceResolution>,
    executables: BTreeMap<String, ExecutableResolution>,
    action_bindings: BTreeMap<String, ActionBinding>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResolutionRoots {
    workspace: PathBuf,
    task_state: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PackagePlan {
    package_id: String,
    dependency_closure: Vec<String>,
    reverse_closure: Vec<String>,
    compiled_reverse_closure: Vec<String>,
    selected_scope: CandidateSelection,
    command_scopes: CommandScopes,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CommandScopes {
    local: Option<CommandScope>,
    locked: Option<CommandScope>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommandScope {
    commands: Vec<String>,
    selected_candidates: BTreeMap<String, CandidateSelection>,
    #[serde(rename = "override")]
    override_policy: OverridePolicy,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OverridePolicy {
    blocked_commands: Vec<String>,
    affected_reverse_closure: Vec<String>,
    required_rebuild: Vec<String>,
    refused: bool,
    refusal_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum CandidateSelection {
    Local,
    Locked,
}

impl CandidateSelection {
    fn label(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Locked => "locked",
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PackageResolution {
    package_id: String,
    candidates: Candidates<LocalPackageCandidate, LockedCandidate>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Candidates<L, R> {
    local: Option<L>,
    locked: Option<R>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocalPackageCandidate {
    kind: String,
    deployable: bool,
    prefix: SourcePrefix,
    provenance: LocalProvenance,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourcePrefix {
    kind: String,
    source_input: String,
    repository_id: String,
    source_root: String,
    workspace_path: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocalProvenance {
    kind: String,
    clean_label: String,
    dirty_label: String,
    source_input: String,
    inspection: ProvenanceInspection,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProvenanceInspection {
    working_directory: PathBuf,
    dirty: DirtyInspection,
    revision: RevisionInspection,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DirtyInspection {
    argv: Vec<String>,
    clean_when: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RevisionInspection {
    argv: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LockedCandidate {
    kind: String,
    deployable: bool,
    installable: String,
    provider: String,
    package: String,
    target_id: String,
    store_path: PathBuf,
    relative_path: PathBuf,
    provenance: LockedProvenance,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LockedProvenance {
    kind: String,
    label: String,
    provider: String,
    package: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArtifactResolution {
    coordinate: String,
    package_id: String,
    target_id: String,
    artifact_id: String,
    kind: String,
    contract: Contract,
    candidates: Candidates<LocalArtifactCandidate, LockedCandidate>,
    consumers: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocalArtifactCandidate {
    kind: String,
    relative_path: PathBuf,
    workspace_path: PathBuf,
    generation: GenerationSelection,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GenerationSelection {
    producer_task: String,
    store: GenerationStore,
    pointer: ExpectedGenerationPointer,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GenerationStore {
    kind: String,
    workspace_path: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExpectedGenerationPointer {
    api_version: String,
    kind: String,
    interface_version: u64,
    file: String,
    identity: ActionTaskIdentity,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Contract {
    name: String,
    version: u64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResourceResolution {
    coordinate: String,
    package_id: String,
    resource_id: String,
    kind: String,
    candidates: Candidates<LocalResourceCandidate, LockedCandidate>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocalResourceCandidate {
    kind: String,
    source_input: String,
    repository_id: String,
    source_root: String,
    relative_path: PathBuf,
    workspace_path: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExecutableResolution {
    coordinate: String,
    package_id: String,
    executable_id: String,
    artifact: String,
    argv: Vec<String>,
    candidates: Candidates<ArtifactCandidate, ArtifactCandidate>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArtifactCandidate {
    kind: String,
    artifact: String,
    candidate: CandidateSelection,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActionBinding {
    package_id: String,
    target_id: String,
    action_id: String,
    scope: String,
    artifacts: BTreeMap<String, BoundArtifact>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BoundArtifact {
    artifact: String,
    contract: Contract,
    environment: Option<BoundEnvironment>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BoundEnvironment {
    name: String,
    value: SelectedArtifactPath,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SelectedArtifactPath {
    kind: String,
    artifact: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GenerationPointer {
    api_version: String,
    kind: String,
    interface_version: u64,
    generation: String,
    manifest: PathBuf,
    identity: ActionTaskIdentity,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GenerationManifest {
    api_version: String,
    kind: String,
    interface_version: u64,
    generation: String,
    identity: GenerationIdentity,
    execution: GenerationExecution,
    outputs: Vec<GenerationOutput>,
    result: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GenerationIdentity {
    declared: ActionTaskIdentity,
    cwd: PathBuf,
    argv: Vec<Value>,
    environment: BTreeMap<String, String>,
    environment_paths: BTreeMap<String, PathBuf>,
    path_prefixes: Vec<PathBuf>,
    locks: Vec<PathBuf>,
    outputs: Vec<DeclaredGenerationOutput>,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActionTaskIdentity {
    api_version: String,
    kind: String,
    interface_version: u64,
    declaration: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclaredGenerationOutput {
    coordinate: String,
    path: PathBuf,
    kind: OutputKind,
    contract: Contract,
    proof: DeclaredOutputProof,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeclaredOutputProof {
    kind: String,
    argv_prefix: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GenerationExecution {
    started_unix_millis: u64,
    finished_unix_millis: u64,
    duration_millis: u64,
    exit_status: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GenerationOutput {
    coordinate: String,
    path: PathBuf,
    kind: OutputKind,
    contract: Contract,
    proof: OutputProof,
    mode: Option<u32>,
    symlink_target: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum OutputKind {
    File,
    Executable,
    Directory,
    Symlink,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct OutputProof {
    kind: String,
    digest: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrefixResult<'a> {
    api_version: &'static str,
    kind: &'static str,
    interface_version: u64,
    package_id: &'a str,
    selected_candidate: CandidateSelection,
    path: &'a Path,
    provenance: ProvenanceResult<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResourceResult<'a> {
    api_version: &'static str,
    kind: &'static str,
    interface_version: u64,
    coordinate: &'a str,
    selected_candidate: CandidateSelection,
    path: &'a Path,
    provenance: ProvenanceResult<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProvenanceResult<'a> {
    kind: &'a str,
    label: &'a str,
    provider: &'a str,
    revision: Option<String>,
}

struct InspectedProvenance<'a> {
    label: &'a str,
    revision: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentExplanation<'a> {
    api_version: &'static str,
    kind: &'static str,
    interface_version: u64,
    package_id: &'a str,
    selected_scope: CandidateSelection,
    dependency_closure: &'a [String],
    reverse_closure: &'a [String],
    #[serde(rename = "override")]
    override_policy: &'a OverridePolicy,
    selections: Vec<SelectionExplanation<'a>>,
    action_bindings: Vec<BindingExplanation<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SelectionExplanation<'a> {
    package_id: &'a str,
    selected_candidate: CandidateSelection,
    provenance: ProvenanceResult<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BindingExplanation<'a> {
    action: &'a str,
    name: &'a str,
    environment: Option<&'a str>,
    artifact: &'a str,
    expected_contract: &'a Contract,
    actual_contract: &'a Contract,
    selected_candidate: CandidateSelection,
    provenance: ProvenanceResult<'a>,
    path: PathBuf,
}

struct LoadedPlan {
    plan: ResolutionPlan,
}

struct SelectedPlan<'a> {
    plan: &'a ResolutionPlan,
    package: &'a PackagePlan,
    scope: &'a CommandScope,
}

struct ResolvedPath<'a> {
    path: PathBuf,
    selection: CandidateSelection,
    provenance: ProvenanceResult<'a>,
}

struct ResolvedArtifact {
    path: PathBuf,
    _lease: Option<GenerationLease>,
}

struct GenerationLease(File);

impl Drop for GenerationLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

pub(crate) fn explain_environment(
    root: &Path,
    explicit_plan: Option<PathBuf>,
    arguments: EnvArgs,
) -> Result<()> {
    debug_assert!(arguments.explain, "Clap requires --explain");
    let loaded = load(root, explicit_plan)?;
    let selected = loaded.select(&arguments.package)?;
    if !selected
        .scope
        .commands
        .iter()
        .any(|command| command == "env")
    {
        return Err(CliError(format!(
            "package `{}` selected scope does not authorize `env` explanation",
            arguments.package
        )));
    }
    let mut selections = Vec::new();
    for package_id in &selected.package.dependency_closure {
        let candidate = selected.candidate_for(package_id)?;
        selections.push(SelectionExplanation {
            package_id,
            selected_candidate: candidate,
            provenance: selected.provenance(root, package_id, candidate, true)?,
        });
    }
    let mut action_bindings = Vec::new();
    let mut generation_leases = Vec::new();
    for (action, binding) in loaded
        .plan
        .action_bindings
        .iter()
        .filter(|(_, binding)| binding.package_id == arguments.package)
    {
        for (name, bound) in &binding.artifacts {
            let artifact = loaded
                .plan
                .artifacts
                .get(&bound.artifact)
                .expect("validated action binding artifact exists");
            let candidate = selected.candidate_for(&artifact.package_id)?;
            let resolved = selected.artifact_path(root, &bound.artifact, artifact, candidate)?;
            let path = resolved.path.clone();
            generation_leases.push(resolved);
            action_bindings.push(BindingExplanation {
                action,
                name,
                environment: bound.environment.as_ref().map(|value| value.name.as_str()),
                artifact: &bound.artifact,
                expected_contract: &bound.contract,
                actual_contract: &artifact.contract,
                selected_candidate: candidate,
                provenance: selected.provenance(root, &artifact.package_id, candidate, true)?,
                path,
            });
        }
    }
    let explanation = EnvironmentExplanation {
        api_version: SUPPORTED_API_VERSION,
        kind: "EnvironmentResolution",
        interface_version: SUPPORTED_INTERFACE_VERSION,
        package_id: &arguments.package,
        selected_scope: selected.package.selected_scope,
        dependency_closure: &selected.package.dependency_closure,
        reverse_closure: &selected.package.reverse_closure,
        override_policy: &selected.scope.override_policy,
        selections,
        action_bindings,
    };
    if arguments.json {
        return write_json(&explanation);
    }
    println!("Package: {}", explanation.package_id);
    println!("Scope: {}", explanation.selected_scope.label());
    println!(
        "Dependencies: {}",
        display_list(explanation.dependency_closure)
    );
    println!(
        "Reverse closure: {}",
        display_list(explanation.reverse_closure)
    );
    if explanation.override_policy.refused {
        println!(
            "Override: REFUSED ({})",
            explanation
                .override_policy
                .refusal_reason
                .as_deref()
                .expect("validated refused override has a reason")
        );
    } else if !explanation.override_policy.required_rebuild.is_empty() {
        println!(
            "Override: allowed after rebuilding {}",
            display_list(&explanation.override_policy.required_rebuild)
        );
    } else {
        println!("Override: allowed");
    }
    println!("PACKAGE\tCANDIDATE\tPROVENANCE\tREVISION\tPROVIDER");
    for selection in &explanation.selections {
        println!(
            "{}\t{}\t{}\t{}\t{}",
            selection.package_id,
            selection.selected_candidate.label(),
            selection.provenance.label,
            selection.provenance.revision.as_deref().unwrap_or("-"),
            selection.provenance.provider
        );
    }
    println!("ACTION\tBINDING\tENVIRONMENT\tARTIFACT\tCONTRACT\tCANDIDATE\tPROVENANCE\tPATH");
    for binding in &explanation.action_bindings {
        println!(
            "{}\t{}\t{}\t{}\t{}@{}\t{}\t{}\t{}",
            binding.action,
            binding.name,
            binding.environment.unwrap_or("-"),
            binding.artifact,
            binding.actual_contract.name,
            binding.actual_contract.version,
            binding.selected_candidate.label(),
            binding.provenance.label,
            binding.path.display()
        );
    }
    drop(generation_leases);
    Ok(())
}

pub(crate) fn package_prefix(
    root: &Path,
    explicit_plan: Option<PathBuf>,
    arguments: PrefixArgs,
) -> Result<()> {
    let loaded = load(root, explicit_plan)?;
    let selected = loaded.select(&arguments.package)?;
    selected.require_allowed("package-prefix")?;
    let selection = selected.candidate_for(&arguments.package)?;
    let resolved = selected.package_path(root, &arguments.package, selection)?;
    require_directory(&resolved.path, "selected package prefix")?;
    if arguments.json {
        return write_json(&PrefixResult {
            api_version: SUPPORTED_API_VERSION,
            kind: "PackagePrefix",
            interface_version: SUPPORTED_INTERFACE_VERSION,
            package_id: &arguments.package,
            selected_candidate: resolved.selection,
            path: &resolved.path,
            provenance: resolved.provenance,
        });
    }
    println!("{}", resolved.path.display());
    Ok(())
}

pub(crate) fn resource(
    root: &Path,
    explicit_plan: Option<PathBuf>,
    arguments: ResourceArgs,
) -> Result<()> {
    let loaded = load(root, explicit_plan)?;
    let resource = loaded
        .plan
        .resources
        .get(&arguments.coordinate)
        .ok_or_else(|| {
            CliError(format!(
                "resource `{}` is absent from the Nix-generated resolution plan",
                arguments.coordinate
            ))
        })?;
    let selected = loaded.select(&resource.package_id)?;
    selected.require_allowed("resource")?;
    let selection = selected.candidate_for(&resource.package_id)?;
    let resolved = selected.resource_path(root, resource, selection)?;
    require_present(&resolved.path, "selected resource")?;
    if arguments.json {
        return write_json(&ResourceResult {
            api_version: SUPPORTED_API_VERSION,
            kind: "ResourceResolution",
            interface_version: SUPPORTED_INTERFACE_VERSION,
            coordinate: &arguments.coordinate,
            selected_candidate: resolved.selection,
            path: &resolved.path,
            provenance: resolved.provenance,
        });
    }
    println!("{}", resolved.path.display());
    Ok(())
}

pub(crate) fn run(
    root: &Path,
    explicit_plan: Option<PathBuf>,
    arguments: RunArgs,
) -> Result<Outcome> {
    let loaded = load(root, explicit_plan)?;
    let executable = loaded
        .plan
        .executables
        .get(&arguments.coordinate)
        .ok_or_else(|| {
            CliError(format!(
                "executable `{}` is absent from the Nix-generated resolution plan",
                arguments.coordinate
            ))
        })?;
    let selection_root = arguments
        .selection_root
        .as_deref()
        .unwrap_or(&executable.package_id);
    let selected = loaded.select(selection_root)?;
    selected.require_allowed("run")?;
    let executable_selection = selected.candidate_for(&executable.package_id)?;
    let executable_candidate = match executable_selection {
        CandidateSelection::Local => executable.candidates.local.as_ref(),
        CandidateSelection::Locked => executable.candidates.locked.as_ref(),
    }
    .ok_or_else(|| {
        selected_candidate_missing("executable", &arguments.coordinate, executable_selection)
    })?;
    if executable_candidate.kind != "artifact-candidate"
        || executable_candidate.artifact != executable.artifact
        || executable_candidate.candidate != executable_selection
    {
        return Err(CliError(format!(
            "executable `{}` does not bind its exact selected {} artifact candidate",
            arguments.coordinate,
            executable_selection.label()
        )));
    }
    let artifact = loaded
        .plan
        .artifacts
        .get(&executable.artifact)
        .ok_or_else(|| {
            CliError(format!(
                "executable `{}` refers to absent artifact `{}`",
                arguments.coordinate, executable.artifact
            ))
        })?;
    let artifact_selection = selected.candidate_for(&artifact.package_id)?;
    if artifact_selection != executable_candidate.candidate {
        return Err(CliError(format!(
            "executable `{}` selects {} but artifact `{}` selects {}; mixed-candidate execution is refused",
            arguments.coordinate,
            executable_candidate.candidate.label(),
            executable.artifact,
            artifact_selection.label()
        )));
    }
    let resolved =
        selected.artifact_path(root, &executable.artifact, artifact, artifact_selection)?;
    require_executable(&resolved.path, &executable.artifact)?;

    let mut command = Command::new(&resolved.path);
    command
        .args(&executable.argv)
        .args(arguments.arguments)
        .current_dir(root);
    let status = command.status().map_err(|error| {
        CliError(format!(
            "cannot execute selected artifact `{}` at {}: {error}",
            executable.artifact,
            resolved.path.display()
        ))
    })?;
    if status.success() {
        Ok(Outcome::Success)
    } else {
        Ok(Outcome::Exit(status.code().unwrap_or(1)))
    }
}

pub(crate) fn readiness(root: &Path, explicit_plan: Option<PathBuf>) -> Result<()> {
    load(root, explicit_plan).map(|_| ())
}

pub(crate) fn authorize_command(
    root: &Path,
    explicit_plan: Option<PathBuf>,
    command: &str,
    package: Option<&str>,
) -> Result<()> {
    let loaded = load(root, explicit_plan)?;
    if let Some(package) = package {
        return loaded.select(package)?.require_allowed(command);
    }
    for package in loaded.plan.package_plans.keys() {
        loaded.select(package)?.require_allowed(command)?;
    }
    Ok(())
}

fn load(root: &Path, explicit_plan: Option<PathBuf>) -> Result<LoadedPlan> {
    let path = explicit_plan
        .or_else(|| env::var_os("NIXSPACE_RESOLUTION_PLAN").map(PathBuf::from))
        .ok_or_else(|| {
            CliError(
                "workspace resolution requires an explicit Nix-generated plan; pass --resolution-plan or set NIXSPACE_RESOLUTION_PLAN"
                    .into(),
            )
        })?;
    let path = resolve_workspace_path(root, &path, "resolution plan path")?;
    let bytes = fs::read(&path).map_err(|error| {
        CliError(format!(
            "workspace resolution plan is unavailable at {}: {error}",
            path.display()
        ))
    })?;
    let plan: ResolutionPlan = serde_json::from_slice(&bytes).map_err(|error| {
        CliError(format!(
            "Nix-generated workspace resolution plan at {} is unreadable: {error}",
            path.display()
        ))
    })?;
    validate(&plan)?;
    Ok(LoadedPlan { plan })
}

fn validate(plan: &ResolutionPlan) -> Result<()> {
    if plan.api_version != SUPPORTED_API_VERSION {
        return Err(CliError(format!(
            "workspace resolution API `{}` is unsupported; this nixspace requires `{SUPPORTED_API_VERSION}`",
            plan.api_version
        )));
    }
    if plan.kind != SUPPORTED_KIND {
        return Err(CliError(format!(
            "workspace resolution kind `{}` is unsupported; this nixspace requires `{SUPPORTED_KIND}`",
            plan.kind
        )));
    }
    if plan.interface_version != SUPPORTED_INTERFACE_VERSION {
        return Err(CliError(format!(
            "workspace resolution interface version {} is unsupported; this nixspace supports version {SUPPORTED_INTERFACE_VERSION}",
            plan.interface_version
        )));
    }
    validate_runtime_path(&plan.roots.workspace, "resolution roots.workspace")?;
    validate_runtime_path(&plan.roots.task_state, "resolution roots.taskState")?;
    for (key, package) in &plan.packages {
        if key != &package.package_id || key.is_empty() {
            return Err(CliError(format!(
                "package resolution key `{key}` must equal its nonempty packageId `{}`",
                package.package_id
            )));
        }
        validate_package_candidate(&plan.roots, key, package)?;
    }
    for (key, package_plan) in &plan.package_plans {
        if key != &package_plan.package_id || !plan.packages.contains_key(key) {
            return Err(CliError(format!(
                "package plan key `{key}` must equal its packageId and reference a declared package"
            )));
        }
        validate_unique("dependencyClosure", key, &package_plan.dependency_closure)?;
        validate_unique("reverseClosure", key, &package_plan.reverse_closure)?;
        validate_unique(
            "compiledReverseClosure",
            key,
            &package_plan.compiled_reverse_closure,
        )?;
        if !package_plan
            .dependency_closure
            .iter()
            .any(|item| item == key)
        {
            return Err(CliError(format!(
                "package plan `{key}` dependencyClosure must contain itself"
            )));
        }
        let scope = selected_scope(package_plan)?;
        validate_unique("commands", key, &scope.commands)?;
        let closure: BTreeSet<_> = package_plan.dependency_closure.iter().collect();
        let scope_selections: BTreeSet<_> = scope.selected_candidates.keys().collect();
        if closure != scope_selections {
            return Err(CliError(format!(
                "package plan `{key}` selected command scope must exactly cover dependencyClosure"
            )));
        }
        for (package_id, selection) in &scope.selected_candidates {
            let package = plan.packages.get(package_id).ok_or_else(|| {
                CliError(format!(
                    "package plan `{key}` selects absent package `{package_id}`"
                ))
            })?;
            require_candidate(&package.candidates, *selection, "package", package_id)?;
        }
        validate_unique(
            "override.affectedReverseClosure",
            key,
            &scope.override_policy.affected_reverse_closure,
        )?;
        validate_unique(
            "override.requiredRebuild",
            key,
            &scope.override_policy.required_rebuild,
        )?;
        validate_unique(
            "override.blockedCommands",
            key,
            &scope.override_policy.blocked_commands,
        )?;
        let reverse: BTreeSet<_> = package_plan.reverse_closure.iter().collect();
        if package_plan
            .compiled_reverse_closure
            .iter()
            .any(|package| !reverse.contains(package))
        {
            return Err(CliError(format!(
                "package plan `{key}` compiledReverseClosure must be contained in reverseClosure"
            )));
        }
        if scope
            .override_policy
            .affected_reverse_closure
            .iter()
            .any(|package| !reverse.contains(package))
        {
            return Err(CliError(format!(
                "package plan `{key}` override affectedReverseClosure must be contained in reverseClosure"
            )));
        }
        if scope
            .override_policy
            .required_rebuild
            .iter()
            .any(|package| !closure.contains(package))
        {
            return Err(CliError(format!(
                "package plan `{key}` override requiredRebuild must be contained in dependencyClosure"
            )));
        }
        if scope
            .override_policy
            .blocked_commands
            .iter()
            .any(|command| !scope.commands.contains(command))
        {
            return Err(CliError(format!(
                "package plan `{key}` override blockedCommands must be contained in commands"
            )));
        }
        match (
            scope.override_policy.refused,
            scope.override_policy.refusal_reason.as_deref(),
        ) {
            (true, Some(reason))
                if !reason.trim().is_empty()
                    && !scope.override_policy.blocked_commands.is_empty() => {}
            (false, None) if scope.override_policy.blocked_commands.is_empty() => {}
            _ => {
                return Err(CliError(format!(
                    "package plan `{key}` must declare a nonempty refusalReason exactly when override.refused is true"
                )))
            }
        }
    }
    for (coordinate, artifact) in &plan.artifacts {
        if coordinate != &artifact.coordinate {
            return Err(CliError(format!(
                "artifact key `{coordinate}` must equal its coordinate `{}`",
                artifact.coordinate
            )));
        }
        validate_identity(
            coordinate,
            &artifact.package_id,
            &artifact.target_id,
            &artifact.artifact_id,
            ':',
            "artifact",
        )?;
        require_package(plan, &artifact.package_id, "artifact", coordinate)?;
        validate_contract(&artifact.contract, coordinate)?;
        validate_artifact_candidates(&plan.roots, coordinate, artifact)?;
        validate_unique("consumers", coordinate, &artifact.consumers)?;
    }
    for (coordinate, resource) in &plan.resources {
        if coordinate != &resource.coordinate
            || coordinate != &format!("{}/{}", resource.package_id, resource.resource_id)
        {
            return Err(CliError(format!(
                "resource key `{coordinate}` must equal packageId/resourceId"
            )));
        }
        require_package(plan, &resource.package_id, "resource", coordinate)?;
        validate_resource_candidates(plan, coordinate, resource)?;
    }
    for (coordinate, executable) in &plan.executables {
        if coordinate != &executable.coordinate
            || coordinate != &format!("{}/{}", executable.package_id, executable.executable_id)
        {
            return Err(CliError(format!(
                "executable key `{coordinate}` must equal packageId/executableId"
            )));
        }
        require_package(plan, &executable.package_id, "executable", coordinate)?;
        let artifact = plan.artifacts.get(&executable.artifact).ok_or_else(|| {
            CliError(format!(
                "executable `{coordinate}` refers to absent artifact `{}`",
                executable.artifact
            ))
        })?;
        if artifact.kind != "executable" {
            return Err(CliError(format!(
                "executable `{coordinate}` must reference an artifact of kind `executable`"
            )));
        }
        validate_argv(
            &executable.argv,
            &format!("executable `{coordinate}` argv"),
            false,
        )?;
        validate_executable_candidate(coordinate, executable, CandidateSelection::Local)?;
        validate_executable_candidate(coordinate, executable, CandidateSelection::Locked)?;
    }
    for (coordinate, binding) in &plan.action_bindings {
        if coordinate
            != &format!(
                "{}:{}:{}",
                binding.package_id, binding.target_id, binding.action_id
            )
        {
            return Err(CliError(format!(
                "action binding key `{coordinate}` must equal packageId:targetId:actionId"
            )));
        }
        if binding.scope != "action" {
            return Err(CliError(format!(
                "action binding `{coordinate}` scope must be `action`"
            )));
        }
        require_package(plan, &binding.package_id, "action binding", coordinate)?;
        let mut environment_names = BTreeSet::new();
        for (name, bound) in &binding.artifacts {
            let artifact = plan.artifacts.get(&bound.artifact).ok_or_else(|| {
                CliError(format!(
                    "action binding `{coordinate}` artifact `{name}` refers to absent `{}`",
                    bound.artifact
                ))
            })?;
            validate_contract(&bound.contract, &bound.artifact)?;
            if bound.contract != artifact.contract {
                return Err(CliError(format!(
                    "action binding `{coordinate}` contract for `{}` does not equal the artifact contract",
                    bound.artifact
                )));
            }
            if let Some(environment) = &bound.environment {
                if environment.value.kind != "selected-artifact-path"
                    || environment.value.artifact != bound.artifact
                {
                    return Err(CliError(format!(
                        "action binding `{coordinate}` environment for `{}` must reference its exact selected artifact path",
                        bound.artifact
                    )));
                }
                validate_environment_name(&environment.name, coordinate)?;
                if !environment_names.insert(&environment.name) {
                    return Err(CliError(format!(
                        "action binding `{coordinate}` repeats environment name `{}`",
                        environment.name
                    )));
                }
            }
        }
    }
    Ok(())
}

impl LoadedPlan {
    fn select(&self, package: &str) -> Result<SelectedPlan<'_>> {
        let package_plan = self.plan.package_plans.get(package).ok_or_else(|| {
            CliError(format!(
                "package `{package}` has no command-scoped entry in the Nix-generated resolution plan"
            ))
        })?;
        let scope = selected_scope(package_plan)?;
        Ok(SelectedPlan {
            plan: &self.plan,
            package: package_plan,
            scope,
        })
    }
}

impl SelectedPlan<'_> {
    fn require_allowed(&self, command: &str) -> Result<()> {
        if !self
            .scope
            .commands
            .iter()
            .any(|declared| declared == command)
        {
            return Err(CliError(format!(
                "package `{}` selected scope `{}` does not authorize `{command}`",
                self.package.package_id,
                self.package.selected_scope.label()
            )));
        }
        if self.scope.override_policy.refused
            && self
                .scope
                .override_policy
                .blocked_commands
                .iter()
                .any(|blocked| blocked == command)
        {
            return Err(CliError(format!(
                "package `{}` {} `{command}` resolution is refused for reverse closure [{}]: {}",
                self.package.package_id,
                self.package.selected_scope.label(),
                display_list(&self.scope.override_policy.affected_reverse_closure),
                self.scope
                    .override_policy
                    .refusal_reason
                    .as_deref()
                    .expect("validated refusal has a reason")
            )));
        }
        Ok(())
    }

    fn candidate_for(&self, package: &str) -> Result<CandidateSelection> {
        self.scope
            .selected_candidates
            .get(package)
            .copied()
            .ok_or_else(|| {
                CliError(format!(
                    "package `{}` command scope does not explicitly select candidate `{package}`; fallback is forbidden",
                    self.package.package_id
                ))
            })
    }

    fn provenance<'a>(
        &'a self,
        root: &Path,
        package_id: &str,
        selection: CandidateSelection,
        inspect: bool,
    ) -> Result<ProvenanceResult<'a>> {
        let package = self
            .plan
            .packages
            .get(package_id)
            .expect("validated selected package exists");
        match selection {
            CandidateSelection::Local => {
                let candidate =
                    package.candidates.local.as_ref().ok_or_else(|| {
                        selected_candidate_missing("package", package_id, selection)
                    })?;
                let inspected = if inspect {
                    Some(inspect_local_provenance(root, &candidate.provenance)?)
                } else {
                    None
                };
                Ok(ProvenanceResult {
                    kind: &candidate.provenance.kind,
                    label: inspected
                        .as_ref()
                        .map_or(candidate.provenance.dirty_label.as_str(), |value| {
                            value.label
                        }),
                    provider: &candidate.provenance.source_input,
                    revision: inspected.map(|value| value.revision),
                })
            }
            CandidateSelection::Locked => {
                let candidate =
                    package.candidates.locked.as_ref().ok_or_else(|| {
                        selected_candidate_missing("package", package_id, selection)
                    })?;
                Ok(ProvenanceResult {
                    kind: &candidate.provenance.kind,
                    label: &candidate.provenance.label,
                    provider: &candidate.provenance.provider,
                    revision: None,
                })
            }
        }
    }

    fn package_path<'a>(
        &'a self,
        root: &Path,
        package_id: &str,
        selection: CandidateSelection,
    ) -> Result<ResolvedPath<'a>> {
        let package = self
            .plan
            .packages
            .get(package_id)
            .expect("validated selected package exists");
        let path = match selection {
            CandidateSelection::Local => resolve_workspace_path(
                root,
                &package
                    .candidates
                    .local
                    .as_ref()
                    .ok_or_else(|| selected_candidate_missing("package", package_id, selection))?
                    .prefix
                    .workspace_path,
                "local package prefix",
            )?,
            CandidateSelection::Locked => {
                locked_path(
                    package.candidates.locked.as_ref().ok_or_else(|| {
                        selected_candidate_missing("package", package_id, selection)
                    })?,
                    "locked package prefix",
                )?
            }
        };
        Ok(ResolvedPath {
            path,
            selection,
            provenance: self.provenance(root, package_id, selection, true)?,
        })
    }

    fn resource_path<'a>(
        &'a self,
        root: &Path,
        resource: &ResourceResolution,
        selection: CandidateSelection,
    ) -> Result<ResolvedPath<'a>> {
        let path = match selection {
            CandidateSelection::Local => resolve_workspace_path(
                root,
                &resource
                    .candidates
                    .local
                    .as_ref()
                    .ok_or_else(|| {
                        selected_candidate_missing("resource", &resource.resource_id, selection)
                    })?
                    .workspace_path,
                "local resource path",
            )?,
            CandidateSelection::Locked => locked_path(
                resource.candidates.locked.as_ref().ok_or_else(|| {
                    selected_candidate_missing("resource", &resource.resource_id, selection)
                })?,
                "locked resource path",
            )?,
        };
        Ok(ResolvedPath {
            path,
            selection,
            provenance: self.provenance(root, &resource.package_id, selection, true)?,
        })
    }

    fn artifact_path(
        &self,
        root: &Path,
        coordinate: &str,
        artifact: &ArtifactResolution,
        selection: CandidateSelection,
    ) -> Result<ResolvedArtifact> {
        match selection {
            CandidateSelection::Local => {
                resolve_local_artifact(
                    root,
                    coordinate,
                    &artifact.contract,
                    artifact.candidates.local.as_ref().ok_or_else(|| {
                        selected_candidate_missing("artifact", coordinate, selection)
                    })?,
                )
            }
            CandidateSelection::Locked => Ok(ResolvedArtifact {
                path: locked_path(
                    artifact.candidates.locked.as_ref().ok_or_else(|| {
                        selected_candidate_missing("artifact", coordinate, selection)
                    })?,
                    "locked artifact path",
                )?,
                _lease: None,
            }),
        }
    }
}

fn validate_package_candidate(
    roots: &ResolutionRoots,
    coordinate: &str,
    package: &PackageResolution,
) -> Result<()> {
    if let Some(local) = &package.candidates.local {
        if local.kind != "local-worktree"
            || local.deployable
            || local.prefix.kind != "source-relative"
            || local.prefix.source_input.is_empty()
            || local.prefix.repository_id.is_empty()
            || local.prefix.source_root.is_empty()
            || local.provenance.kind != "local-git"
            || local.provenance.clean_label != "LOCAL commit"
            || local.provenance.dirty_label != "LOCAL dirty"
            || local.provenance.source_input != local.prefix.source_input
        {
            return Err(CliError(format!(
                "local package candidate `{coordinate}` must be a non-deployable source-relative local-worktree with exact local-git provenance"
            )));
        }
        validate_runtime_path(&local.prefix.workspace_path, "local package workspacePath")?;
        let source_root = Path::new(&local.prefix.source_root);
        validate_portable_relative(source_root, "local package sourceRoot", true)?;
        if source_root != Path::new(".") && !local.prefix.workspace_path.ends_with(source_root) {
            return Err(CliError(format!(
                "local package candidate `{coordinate}` workspacePath must end in its declared sourceRoot"
            )));
        }
        if !source_path_allowed(&roots.workspace, &local.prefix.workspace_path) {
            return Err(CliError(format!(
                "local package candidate `{coordinate}` relative workspacePath must remain below roots.workspace"
            )));
        }
        validate_runtime_path(
            &local.provenance.inspection.working_directory,
            "local provenance workingDirectory",
        )?;
        if local.provenance.inspection.working_directory != local.prefix.workspace_path {
            return Err(CliError(format!(
                "local package candidate `{coordinate}` provenance inspection must run at its exact prefix"
            )));
        }
        if local.provenance.inspection.dirty.clean_when != "stdout-empty" {
            return Err(CliError(format!(
                "local package candidate `{coordinate}` provenance dirty.cleanWhen must be `stdout-empty`"
            )));
        }
        validate_argv(
            &local.provenance.inspection.dirty.argv,
            "local dirty-provenance inspection argv",
            true,
        )?;
        validate_argv(
            &local.provenance.inspection.revision.argv,
            "local revision-provenance inspection argv",
            true,
        )?;
    }
    if let Some(locked) = &package.candidates.locked {
        validate_locked_candidate(locked, coordinate)?;
    }
    if package.candidates.local.is_none() && package.candidates.locked.is_none() {
        return Err(CliError(format!(
            "package `{coordinate}` must declare at least one candidate"
        )));
    }
    Ok(())
}

fn validate_locked_candidate(candidate: &LockedCandidate, coordinate: &str) -> Result<()> {
    if candidate.kind != "nix-store"
        || !candidate.deployable
        || candidate.installable.is_empty()
        || candidate.provider.is_empty()
        || candidate.package.is_empty()
        || candidate.target_id.is_empty()
        || !candidate.store_path.is_absolute()
        || candidate.provenance.kind != "locked-output"
        || candidate.provenance.label != "LOCKED"
        || candidate.provenance.provider != candidate.provider
        || candidate.provenance.package != candidate.package
    {
        return Err(CliError(format!(
            "locked candidate `{coordinate}` must be a deployable nix-store with an absolute storePath and exact locked-output provenance"
        )));
    }
    validate_portable_relative(
        &candidate.relative_path,
        "locked candidate relativePath",
        true,
    )
}

fn validate_artifact_candidates(
    roots: &ResolutionRoots,
    coordinate: &str,
    artifact: &ArtifactResolution,
) -> Result<()> {
    if let Some(local) = &artifact.candidates.local {
        if local.kind != "published-generation" || local.generation.producer_task.is_empty() {
            return Err(CliError(format!(
                "local artifact `{coordinate}` must be a published-generation with a producerTask"
            )));
        }
        validate_portable_relative(&local.relative_path, "artifact relativePath", false)?;
        validate_runtime_path(&local.workspace_path, "artifact workspacePath")?;
        if !source_path_allowed(&roots.workspace, &local.workspace_path) {
            return Err(CliError(format!(
                "local artifact `{coordinate}` relative workspacePath must remain below roots.workspace"
            )));
        }
        if local.generation.store.kind != "workspace-relative" {
            return Err(CliError(format!(
                "local artifact `{coordinate}` generation store kind must be `workspace-relative`"
            )));
        }
        validate_runtime_path(
            &local.generation.store.workspace_path,
            "generation store workspacePath",
        )?;
        if !lexically_below(&roots.task_state, &local.generation.store.workspace_path) {
            return Err(CliError(format!(
                "local artifact `{coordinate}` generation store must remain below roots.taskState"
            )));
        }
        let pointer = &local.generation.pointer;
        if pointer.api_version != SUPPORTED_API_VERSION
            || pointer.kind != GENERATION_POINTER_KIND
            || pointer.interface_version != GENERATION_INTERFACE_VERSION
            || pointer.file != "current"
            || !valid_action_identity(&pointer.identity)
        {
            return Err(CliError(format!(
                "local artifact `{coordinate}` must declare the exact nonempty ActionGenerationPointer v1 identity at `current`"
            )));
        }
    }
    if let Some(locked) = &artifact.candidates.locked {
        validate_locked_candidate(locked, coordinate)?;
    }
    if artifact.candidates.local.is_none() && artifact.candidates.locked.is_none() {
        return Err(CliError(format!(
            "artifact `{coordinate}` must declare at least one candidate"
        )));
    }
    Ok(())
}

fn validate_resource_candidates(
    plan: &ResolutionPlan,
    coordinate: &str,
    resource: &ResourceResolution,
) -> Result<()> {
    if let Some(local) = &resource.candidates.local {
        if local.kind != "source-relative"
            || local.source_input.is_empty()
            || local.repository_id.is_empty()
            || local.source_root.is_empty()
        {
            return Err(CliError(format!(
                "local resource `{coordinate}` must be source-relative with source identity"
            )));
        }
        validate_portable_relative(&local.relative_path, "resource relativePath", true)?;
        validate_runtime_path(&local.workspace_path, "resource workspacePath")?;
        let package = plan
            .packages
            .get(&resource.package_id)
            .expect("resource package validated before candidates");
        let prefix = package.candidates.local.as_ref().ok_or_else(|| {
            CliError(format!(
                "local resource `{coordinate}` requires a local package prefix"
            ))
        })?;
        if local.source_input != prefix.prefix.source_input
            || local.repository_id != prefix.prefix.repository_id
            || local.source_root != prefix.prefix.source_root
        {
            return Err(CliError(format!(
                "local resource `{coordinate}` source identity must equal its package prefix"
            )));
        }
        let expected = join_relative(&prefix.prefix.workspace_path, &local.relative_path);
        if local.workspace_path != expected
            || !source_path_allowed(&plan.roots.workspace, &local.workspace_path)
        {
            return Err(CliError(format!(
                "local resource `{coordinate}` workspacePath must equal its package prefix plus relativePath; relative paths must remain below roots.workspace"
            )));
        }
    }
    if let Some(locked) = &resource.candidates.locked {
        validate_locked_candidate(locked, coordinate)?;
    }
    if resource.candidates.local.is_none() && resource.candidates.locked.is_none() {
        return Err(CliError(format!(
            "resource `{coordinate}` must declare at least one candidate"
        )));
    }
    Ok(())
}

fn validate_executable_candidate(
    coordinate: &str,
    executable: &ExecutableResolution,
    selection: CandidateSelection,
) -> Result<()> {
    let candidate = match selection {
        CandidateSelection::Local => executable.candidates.local.as_ref(),
        CandidateSelection::Locked => executable.candidates.locked.as_ref(),
    };
    if let Some(candidate) = candidate {
        if candidate.kind != "artifact-candidate"
            || candidate.artifact != executable.artifact
            || candidate.candidate != selection
        {
            return Err(CliError(format!(
                "executable `{coordinate}` {} candidate must bind its exact artifact and candidate kind",
                selection.label()
            )));
        }
    }
    Ok(())
}

fn selected_scope(package: &PackagePlan) -> Result<&CommandScope> {
    match package.selected_scope {
        CandidateSelection::Local => package.command_scopes.local.as_ref(),
        CandidateSelection::Locked => package.command_scopes.locked.as_ref(),
    }
    .ok_or_else(|| {
        CliError(format!(
            "package plan `{}` selectedScope `{}` has no matching command scope; scope fallback is forbidden",
            package.package_id,
            package.selected_scope.label()
        ))
    })
}

fn require_candidate<L, R>(
    candidates: &Candidates<L, R>,
    selection: CandidateSelection,
    kind: &str,
    coordinate: &str,
) -> Result<()> {
    let present = match selection {
        CandidateSelection::Local => candidates.local.is_some(),
        CandidateSelection::Locked => candidates.locked.is_some(),
    };
    if present {
        Ok(())
    } else {
        Err(selected_candidate_missing(kind, coordinate, selection))
    }
}

fn selected_candidate_missing(
    kind: &str,
    coordinate: &str,
    selection: CandidateSelection,
) -> CliError {
    CliError(format!(
        "{kind} `{coordinate}` explicitly selects {} but that candidate is absent; fallback is forbidden",
        selection.label()
    ))
}

fn resolve_local_artifact(
    root: &Path,
    coordinate: &str,
    contract: &Contract,
    candidate: &LocalArtifactCandidate,
) -> Result<ResolvedArtifact> {
    let generation_root = resolve_workspace_path(
        root,
        &candidate.generation.store.workspace_path,
        "generation store path",
    )?;
    let lock = OpenOptions::new()
        .read(true)
        .open(generation_root.join(".publish.lock"))
        .map_err(|error| {
            CliError(format!(
                "selected local artifact `{coordinate}` generation lock is unavailable: {error}; locked fallback is forbidden"
            ))
        })?;
    FileExt::lock_shared(&lock).map_err(|error| {
        CliError(format!(
            "cannot acquire selected local artifact `{coordinate}` generation lease: {error}"
        ))
    })?;
    let lease = GenerationLease(lock);
    let pointer_path = generation_root.join(&candidate.generation.pointer.file);
    let bytes = fs::read(&pointer_path).map_err(|error| {
        CliError(format!(
            "selected local artifact `{coordinate}` has no readable atomic generation pointer at {}: {error}; locked fallback is forbidden",
            pointer_path.display()
        ))
    })?;
    let pointer: GenerationPointer = serde_json::from_slice(&bytes).map_err(|error| {
        CliError(format!(
            "selected local artifact `{coordinate}` generation pointer is unreadable: {error}; locked fallback is forbidden"
        ))
    })?;
    if pointer.api_version != candidate.generation.pointer.api_version
        || pointer.kind != candidate.generation.pointer.kind
        || pointer.interface_version != candidate.generation.pointer.interface_version
        || pointer.identity != candidate.generation.pointer.identity
        || !portable_segment(&pointer.generation)
    {
        return Err(CliError(format!(
            "selected local artifact `{coordinate}` generation pointer does not match the exact Nix-selected version, kind, generation, and identity; locked fallback is forbidden"
        )));
    }
    let expected_manifest = PathBuf::from("generations")
        .join(&pointer.generation)
        .join("manifest.json");
    if pointer.manifest != expected_manifest {
        return Err(CliError(format!(
            "selected local artifact `{coordinate}` generation pointer manifest is not the exact immutable generation path"
        )));
    }
    let manifest_path = generation_root.join(&pointer.manifest);
    let manifest_bytes = fs::read(&manifest_path).map_err(|error| {
        CliError(format!(
            "selected local artifact `{coordinate}` generation manifest is unavailable at {}: {error}",
            manifest_path.display()
        ))
    })?;
    let manifest: GenerationManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|error| {
            CliError(format!(
                "selected local artifact `{coordinate}` generation manifest is unreadable: {error}"
            ))
        })?;
    validate_manifest(&manifest, &pointer, coordinate)?;
    let expected_path = resolve_workspace_path(
        root,
        &candidate.workspace_path,
        "local artifact workspacePath",
    )?;
    if !candidate.workspace_path.ends_with(&candidate.relative_path) {
        return Err(CliError(format!(
            "selected local artifact `{coordinate}` workspacePath does not end in its declared relativePath"
        )));
    }
    let mut declared_matching = manifest
        .identity
        .outputs
        .iter()
        .filter(|output| output.coordinate == coordinate);
    let declared = declared_matching.next().ok_or_else(|| {
        CliError(format!(
            "selected local generation identity does not declare artifact `{coordinate}`"
        ))
    })?;
    if declared_matching.next().is_some()
        || declared.path != candidate.workspace_path
        || &declared.contract != contract
    {
        return Err(CliError(format!(
            "selected local generation identity must declare exactly the Nix-selected path and contract for `{coordinate}`"
        )));
    }
    let mut matching = manifest
        .outputs
        .iter()
        .filter(|output| output.coordinate == coordinate);
    let output = matching.next().ok_or_else(|| {
        CliError(format!(
            "selected local generation `{}` does not contain artifact `{coordinate}`",
            manifest.generation
        ))
    })?;
    if matching.next().is_some()
        || output.path != expected_path
        || output.kind != declared.kind
        || &output.contract != contract
        || output.proof.kind != declared.proof.kind
    {
        return Err(CliError(format!(
            "selected local generation must contain exactly one `{coordinate}` output at the Nix-selected path with its declared kind, proof, and contract {}@{}",
            contract.name, contract.version
        )));
    }
    let result_outputs: Vec<GenerationOutput> =
        serde_json::from_value(manifest.result.get("outputs").cloned().ok_or_else(|| {
            CliError(format!(
                "selected local generation result omits output proofs for `{coordinate}`"
            ))
        })?)
        .map_err(|error| {
            CliError(format!(
                "selected local generation result output proofs are invalid: {error}"
            ))
        })?;
    if result_outputs != manifest.outputs {
        return Err(CliError(format!(
            "selected local generation result and manifest output proofs differ for `{coordinate}`"
        )));
    }
    require_output_kind(&output.path, output.kind, coordinate)?;
    reprove_output(root, coordinate, declared, output)?;
    Ok(ResolvedArtifact {
        path: output.path.clone(),
        _lease: Some(lease),
    })
}

fn reprove_output(
    root: &Path,
    coordinate: &str,
    declared: &DeclaredGenerationOutput,
    output: &GenerationOutput,
) -> Result<()> {
    validate_argv(
        &declared.proof.argv_prefix,
        "generation output proof argvPrefix",
        true,
    )?;
    let (program, arguments) = declared
        .proof
        .argv_prefix
        .split_first()
        .expect("validated proof argvPrefix");
    let proof = Command::new(program)
        .args(arguments)
        .arg(&output.path)
        .current_dir(root)
        .output()
        .map_err(|error| {
            CliError(format!(
                "cannot re-prove selected local artifact `{coordinate}` with Nix-declared `{program}`: {error}"
            ))
        })?;
    if !proof.status.success() {
        return Err(CliError(format!(
            "proof for selected local artifact `{coordinate}` failed with {}: {}",
            proof.status,
            String::from_utf8_lossy(&proof.stderr).trim()
        )));
    }
    let stdout = String::from_utf8(proof.stdout).map_err(|error| {
        CliError(format!(
            "proof for selected local artifact `{coordinate}` returned non-UTF-8 output: {error}"
        ))
    })?;
    let lines: Vec<_> = stdout.lines().filter(|line| !line.is_empty()).collect();
    if lines.len() != 1 || lines[0].trim() != lines[0] || lines[0] != output.proof.digest {
        return Err(CliError(format!(
            "selected local artifact `{coordinate}` no longer matches its published generation proof; locked fallback is forbidden"
        )));
    }
    Ok(())
}

fn validate_manifest(
    manifest: &GenerationManifest,
    pointer: &GenerationPointer,
    coordinate: &str,
) -> Result<()> {
    if manifest.api_version != SUPPORTED_API_VERSION
        || manifest.kind != GENERATION_KIND
        || manifest.interface_version != GENERATION_INTERFACE_VERSION
        || manifest.generation != pointer.generation
        || !valid_action_identity(&pointer.identity)
        || manifest.identity.declared != pointer.identity
        || manifest.execution.exit_status != 0
        || manifest.execution.finished_unix_millis < manifest.execution.started_unix_millis
        || manifest.execution.duration_millis
            > manifest
                .execution
                .finished_unix_millis
                .saturating_sub(manifest.execution.started_unix_millis)
                .saturating_add(60_000)
        || !manifest.result.is_object()
    {
        return Err(CliError(format!(
            "selected local artifact `{coordinate}` generation manifest does not match the exact successful ActionGeneration v1 pointer identity"
        )));
    }
    // Parsing these strict fields is part of the contract even when resolution
    // only needs the declared identity and output records.
    let _ = (
        &manifest.identity.cwd,
        &manifest.identity.argv,
        &manifest.identity.environment,
        &manifest.identity.environment_paths,
        &manifest.identity.path_prefixes,
        &manifest.identity.locks,
        &manifest.identity.outputs,
    );
    for output in &manifest.outputs {
        if output.coordinate.is_empty()
            || output.contract.name.is_empty()
            || output.contract.version == 0
            || output.proof.kind.is_empty()
            || output.proof.digest.is_empty()
        {
            return Err(CliError(format!(
                "selected local artifact `{coordinate}` generation contains an invalid output proof"
            )));
        }
        let _ = (output.mode, &output.symlink_target);
    }
    for output in &manifest.identity.outputs {
        if output.coordinate.is_empty()
            || output.contract.name.is_empty()
            || output.contract.version == 0
            || output.proof.kind.is_empty()
        {
            return Err(CliError(format!(
                "selected local artifact `{coordinate}` generation identity contains an invalid output declaration"
            )));
        }
        validate_argv(
            &output.proof.argv_prefix,
            "generation identity output proof argvPrefix",
            true,
        )?;
    }
    Ok(())
}

fn inspect_local_provenance<'a>(
    root: &Path,
    provenance: &'a LocalProvenance,
) -> Result<InspectedProvenance<'a>> {
    let cwd = resolve_workspace_path(
        root,
        &provenance.inspection.working_directory,
        "provenance inspection workingDirectory",
    )?;
    let dirty = run_inspection(
        &cwd,
        &provenance.inspection.dirty.argv,
        &provenance.source_input,
        "dirty-state",
    )?;
    let revision = run_inspection(
        &cwd,
        &provenance.inspection.revision.argv,
        &provenance.source_input,
        "revision",
    )?;
    let revision = String::from_utf8(revision).map_err(|error| {
        CliError(format!(
            "Nix-declared revision inspection for `{}` returned non-UTF-8 output: {error}",
            provenance.source_input
        ))
    })?;
    let lines: Vec<_> = revision.lines().collect();
    if lines.len() != 1 || lines[0].is_empty() || lines[0].trim() != lines[0] {
        return Err(CliError(format!(
            "Nix-declared revision inspection for `{}` must return exactly one nonempty trimmed line",
            provenance.source_input
        )));
    }
    Ok(InspectedProvenance {
        label: if dirty.is_empty() {
            &provenance.clean_label
        } else {
            &provenance.dirty_label
        },
        revision: lines[0].to_owned(),
    })
}

fn run_inspection(
    cwd: &Path,
    argv: &[String],
    source_input: &str,
    purpose: &str,
) -> Result<Vec<u8>> {
    let (program, arguments) = argv
        .split_first()
        .expect("validated provenance inspection argv");
    let output = Command::new(program)
        .args(arguments)
        .current_dir(cwd)
        .output()
        .map_err(|error| {
            CliError(format!(
                "cannot execute Nix-declared {purpose} inspection for `{source_input}`: {error}"
            ))
        })?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(CliError(format!(
            "Nix-declared {purpose} inspection for `{source_input}` failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn locked_path(candidate: &LockedCandidate, description: &str) -> Result<PathBuf> {
    validate_locked_candidate(candidate, description)?;
    if candidate.relative_path == Path::new(".") {
        Ok(candidate.store_path.clone())
    } else {
        Ok(candidate.store_path.join(&candidate.relative_path))
    }
}

fn resolve_workspace_path(root: &Path, path: &Path, description: &str) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    validate_runtime_path(path, description)?;
    Ok(root.join(path))
}

fn validate_runtime_path(path: &Path, description: &str) -> Result<()> {
    if path.is_absolute() {
        let rendered = path
            .to_str()
            .ok_or_else(|| CliError(format!("{description} must contain valid Unicode")))?;
        if rendered.contains('\0')
            || path
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(CliError(format!(
                "{description} must be an absolute path without parent traversal"
            )));
        }
        Ok(())
    } else {
        validate_portable_relative(path, description, true)
    }
}

fn join_relative(base: &Path, relative: &Path) -> PathBuf {
    if relative == Path::new(".") {
        base.to_path_buf()
    } else {
        base.join(relative)
    }
}

fn lexically_below(base: &Path, path: &Path) -> bool {
    if base.is_absolute() != path.is_absolute() {
        return false;
    }
    let components = |value: &Path| {
        value
            .components()
            .filter_map(|component| match component {
                Component::Prefix(prefix) => Some(prefix.as_os_str().to_owned()),
                Component::RootDir => Some(std::ffi::OsString::from("/")),
                Component::Normal(segment) => Some(segment.to_owned()),
                Component::CurDir => None,
                Component::ParentDir => Some(std::ffi::OsString::from("..")),
            })
            .collect::<Vec<_>>()
    };
    let base = components(base);
    let path = components(path);
    path.starts_with(&base)
}

fn source_path_allowed(workspace_root: &Path, path: &Path) -> bool {
    path.is_absolute() || lexically_below(workspace_root, path)
}

fn validate_portable_relative(path: &Path, description: &str, allow_dot: bool) -> Result<()> {
    if path.is_absolute() || path.as_os_str().is_empty() {
        return Err(CliError(format!(
            "{description} must be a nonempty portable relative path"
        )));
    }
    if allow_dot && path == Path::new(".") {
        return Ok(());
    }
    let rendered = path
        .to_str()
        .ok_or_else(|| CliError(format!("{description} must contain valid Unicode")))?;
    if rendered.contains('\\')
        || path.components().any(|component| {
            !matches!(component, Component::Normal(segment) if segment.to_str().is_some_and(portable_segment))
        })
    {
        return Err(CliError(format!(
            "{description} must remain lexically contained and use portable path segments"
        )));
    }
    Ok(())
}

fn portable_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && segment
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
}

fn validate_unique(description: &str, owner: &str, values: &[String]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        if value.is_empty() || !seen.insert(value) {
            return Err(CliError(format!(
                "`{owner}` {description} must contain unique nonempty values"
            )));
        }
    }
    Ok(())
}

fn validate_contract(contract: &Contract, coordinate: &str) -> Result<()> {
    if contract.name.is_empty() || contract.version == 0 {
        return Err(CliError(format!(
            "artifact `{coordinate}` contract must have a nonempty name and positive version"
        )));
    }
    Ok(())
}

fn valid_action_identity(identity: &ActionTaskIdentity) -> bool {
    identity.api_version == SUPPORTED_API_VERSION
        && identity.kind == "ActionTaskIdentity"
        && identity.interface_version == 1
        && identity
            .declaration
            .as_object()
            .is_some_and(|declaration| !declaration.is_empty())
}

fn validate_identity(
    coordinate: &str,
    package: &str,
    target: &str,
    item: &str,
    separator: char,
    kind: &str,
) -> Result<()> {
    if coordinate != format!("{package}{separator}{target}{separator}{item}") {
        return Err(CliError(format!(
            "{kind} key `{coordinate}` must equal packageId{separator}targetId{separator}{kind}Id"
        )));
    }
    Ok(())
}

fn require_package(
    plan: &ResolutionPlan,
    package: &str,
    kind: &str,
    coordinate: &str,
) -> Result<()> {
    if plan.packages.contains_key(package) {
        Ok(())
    } else {
        Err(CliError(format!(
            "{kind} `{coordinate}` refers to absent package `{package}`"
        )))
    }
}

fn validate_argv(argv: &[String], description: &str, require_program: bool) -> Result<()> {
    if (require_program && argv.is_empty())
        || argv.iter().any(|argument| argument.contains('\0'))
        || (require_program && argv.first().is_some_and(String::is_empty))
    {
        return Err(CliError(format!(
            "{description} must {}contain no NUL bytes",
            if require_program {
                "start with a nonempty program and "
            } else {
                ""
            }
        )));
    }
    Ok(())
}

fn validate_environment_name(name: &str, coordinate: &str) -> Result<()> {
    if name.is_empty()
        || name.contains('=')
        || name.contains('\0')
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(CliError(format!(
            "action binding `{coordinate}` environment name `{name}` is invalid"
        )));
    }
    Ok(())
}

fn require_present(path: &Path, description: &str) -> Result<()> {
    fs::symlink_metadata(path).map(|_| ()).map_err(|error| {
        CliError(format!(
            "{description} is unavailable at {}: {error}; selected-candidate fallback is forbidden",
            path.display()
        ))
    })
}

fn require_directory(path: &Path, description: &str) -> Result<()> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(CliError(format!(
            "{description} is not an available directory at {}; selected-candidate fallback is forbidden",
            path.display()
        )))
    }
}

fn require_executable(path: &Path, coordinate: &str) -> Result<()> {
    let metadata = fs::metadata(path).map_err(|error| {
        CliError(format!(
            "selected executable artifact `{coordinate}` is unavailable at {}: {error}; fallback is forbidden",
            path.display()
        ))
    })?;
    if metadata.is_file() && executable(&metadata) {
        Ok(())
    } else {
        Err(CliError(format!(
            "selected executable artifact `{coordinate}` is not executable at {}; fallback is forbidden",
            path.display()
        )))
    }
}

fn require_output_kind(path: &Path, kind: OutputKind, coordinate: &str) -> Result<()> {
    let link_metadata = fs::symlink_metadata(path).map_err(|error| {
        CliError(format!(
            "selected generation output `{coordinate}` is unavailable at {}: {error}",
            path.display()
        ))
    })?;
    let followed;
    let metadata = match kind {
        OutputKind::Symlink => &link_metadata,
        _ => {
            followed = fs::metadata(path).map_err(|error| {
                CliError(format!(
                    "selected generation output `{coordinate}` target is unavailable: {error}"
                ))
            })?;
            &followed
        }
    };
    let valid = match kind {
        OutputKind::File => metadata.is_file(),
        OutputKind::Executable => metadata.is_file() && executable(metadata),
        OutputKind::Directory => metadata.is_dir(),
        OutputKind::Symlink => metadata.file_type().is_symlink(),
    };
    if valid {
        Ok(())
    } else {
        Err(CliError(format!(
            "selected generation output `{coordinate}` does not match its proved kind"
        )))
    }
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

fn display_list(values: &[String]) -> String {
    if values.is_empty() {
        "-".into()
    } else {
        values.join(",")
    }
}
