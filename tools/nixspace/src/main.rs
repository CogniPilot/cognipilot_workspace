mod action;
mod benchmark;
mod closure_materialization;
mod host;
mod index;
mod launch;
mod launch_exec;
mod model;
mod resolution;
mod source;
mod west;

use std::env;
use std::error::Error;
use std::fmt::{self, Display};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use miette::Diagnostic;
use model::{GraphDocument, Index, PackageDocument};
use serde::Serialize;
use serde_json::json;

const SUPPORTED_INTERFACE_VERSION: u64 = 1;
const SUPPORTED_API_VERSION: &str = "nixspace/v1";
const SUPPORTED_KIND: &str = "Workspace";
const TOP_LEVEL_COMPLETIONS: &[&str] = &[
    "package",
    "env",
    "target",
    "artifact",
    "resource",
    "executable",
    "launch",
    "run",
    "graph",
    "build",
    "test",
    "benchmark",
    "cache",
    "doctor",
    "setup",
    "sync",
    "status",
    "update",
    "index",
    "west",
    "completion",
];

type Result<T> = std::result::Result<T, CliError>;

#[derive(Debug, Diagnostic)]
struct CliError(String);

impl Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for CliError {}

#[derive(Debug, Parser)]
#[command(
    name = "nixspace",
    version,
    about = "Query and operate a versioned Nix workspace interface"
)]
struct Cli {
    /// Resolve workspace-relative state and plans from this directory.
    #[arg(long, global = true, value_name = "PATH")]
    workspace_root: Option<PathBuf>,

    /// Read and cache the generated workspace index at this path.
    #[arg(long, global = true, value_name = "PATH")]
    index: Option<PathBuf>,

    /// Read a versioned Nix-generated West execution plan from this path.
    #[arg(long, global = true, value_name = "PATH")]
    west_plan: Option<PathBuf>,

    /// Read a versioned Nix-generated host expectation plan from this path.
    #[arg(long, global = true, value_name = "PATH")]
    host_plan: Option<PathBuf>,

    /// Read or update this Nix configuration file.
    #[arg(long, global = true, value_name = "PATH")]
    nix_config: Option<PathBuf>,

    /// Select single-user or daemon Nix configuration behavior.
    #[arg(long, global = true, value_enum, value_name = "MODE")]
    nix_mode: Option<host::NixMode>,

    /// Read a versioned Nix-generated editable source plan from this path.
    #[arg(long, global = true, value_name = "PATH")]
    source_plan: Option<PathBuf>,

    /// Read a versioned Nix-generated launch execution plan from this path.
    #[arg(long, global = true, value_name = "PATH")]
    launch_plan: Option<PathBuf>,

    /// Read the versioned Nix-generated local/locked resolution plan from this path.
    #[arg(long, global = true, value_name = "PATH")]
    resolution_plan: Option<PathBuf>,

    /// Read a versioned Nix-generated benchmark plan from this path.
    #[arg(long, global = true, value_name = "PATH")]
    benchmark_plan: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the exact Nix-emitted build task roots through the declared runner.
    Build(action::ActionArgs),
    /// Run the exact Nix-emitted test task roots through the declared runner.
    Test(action::ActionArgs),
    /// Time exact Nix-generated commands and enforce their p50/p95 gates.
    Benchmark(benchmark::BenchmarkArgs),
    /// Inspect declared realized roots against declared public Nix caches.
    Cache {
        #[command(subcommand)]
        command: CacheCommand,
    },
    /// Diagnose the host against the exact Nix-generated expectations.
    Doctor(host::DoctorArgs),
    /// Apply or verify the Nix-generated host expectations.
    Setup(host::SetupArgs),
    /// Clone only missing repositories from an exact Nix-emitted selection.
    Sync(source::SelectionArgs),
    /// Stream Git status for an exact Nix-emitted selection.
    Status(source::SelectionArgs),
    /// Fetch and fast-forward an exact Nix-emitted selection transactionally.
    Update(source::SelectionArgs),
    Package {
        #[command(subcommand)]
        command: PackageCommand,
    },
    /// Explain one package's exact Nix-selected command environment.
    Env(resolution::EnvArgs),
    Target {
        #[command(subcommand)]
        command: CollectionCommand,
    },
    Artifact {
        #[command(subcommand)]
        command: CollectionCommand,
    },
    /// Resolve one exact Nix-selected resource path.
    Resource(resolution::ResourceArgs),
    Executable {
        #[command(subcommand)]
        command: CollectionCommand,
    },
    Launch {
        #[command(subcommand)]
        command: LaunchCommand,
    },
    /// Execute one exact workspace executable through its declared artifact.
    Run(resolution::RunArgs),
    Graph(GraphArgs),
    Index {
        #[command(subcommand)]
        command: index::IndexCommand,
    },
    West {
        #[command(subcommand)]
        command: west::WestCommand,
    },
    /// Generate a completion script from the authoritative Clap command tree.
    Completion {
        #[arg(value_enum)]
        shell: Shell,
    },
    #[command(name = "_complete", hide = true)]
    Complete {
        #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
        words: Vec<String>,
    },
    /// Execute one typed Nix-generated editable action task.
    #[command(name = "_run-task", hide = true)]
    RunTask(action::RunTaskArgs),
    /// Materialize one generic Nix closure document from structured attributes.
    #[command(name = "_materialize-closure", hide = true)]
    MaterializeClosure(closure_materialization::MaterializeArgs),
    /// Run one exact readiness probe without polling or supervision.
    #[command(name = "_probe", hide = true)]
    Probe(launch_exec::ProbeArgs),
}

#[derive(Debug, Subcommand)]
enum PackageCommand {
    List {
        #[arg(long)]
        json: bool,
    },
    Show {
        coordinate: String,
        #[arg(long)]
        json: bool,
    },
    /// Resolve the exact selected local or locked package prefix.
    Prefix(resolution::PrefixArgs),
}

#[derive(Debug, Subcommand)]
enum CacheCommand {
    /// Report exact path and byte coverage without building or substituting.
    Coverage(host::CacheCoverageArgs),
}

#[derive(Debug, Subcommand)]
enum CollectionCommand {
    List {
        package: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Show {
        coordinate: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum LaunchCommand {
    List {
        package: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Show {
        coordinate: String,
        #[arg(long)]
        json: bool,
    },
    Plan {
        coordinate: String,
        #[arg(long = "set", value_name = "NAME=VALUE", action = clap::ArgAction::Append)]
        assignments: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    /// Start one exact Nix-emitted launch execution.
    Up(launch_exec::UpArgs),
    /// List recorded launch sessions.
    Sessions,
    /// Attach to a recorded launch session.
    Attach(launch_exec::SessionArgs),
    /// Query a recorded launch session through its manager.
    Status(launch_exec::SessionArgs),
    /// Stream logs from a recorded launch session.
    Logs(launch_exec::SessionArgs),
    /// Stop a recorded launch session and remove its owned state.
    Down(launch_exec::SessionArgs),
}

#[derive(Debug, clap::Args)]
struct GraphArgs {
    package: Option<String>,
    #[arg(long, conflicts_with = "dot")]
    json: bool,
    #[arg(long, conflicts_with = "json")]
    dot: bool,
    #[arg(long)]
    reverse: bool,
}

fn main() {
    let cli = Cli::parse();
    match run(cli) {
        Ok(action::Outcome::Success) => {}
        Ok(action::Outcome::Exit(code)) => std::process::exit(code),
        Err(error) => {
            let report = miette::Report::new(error);
            eprintln!("error: {report}");
            std::process::exit(1);
        }
    }
}

fn run(cli: Cli) -> Result<action::Outcome> {
    let root = workspace_root(cli.workspace_root)?;
    let index_path = index_path(cli.index, &root);
    let west_plan = cli.west_plan;
    let host = host::Options {
        plan: cli.host_plan,
        nix_config: cli.nix_config,
        mode: cli.nix_mode,
        index: index_path.clone(),
        source_plan: cli.source_plan.clone(),
        launch_plan: cli.launch_plan.clone(),
        resolution_plan: cli.resolution_plan.clone(),
    };
    let source_plan = cli.source_plan;
    let launch_plan = cli.launch_plan;
    let resolution_plan = cli.resolution_plan;
    let benchmark_plan = cli.benchmark_plan;
    match cli.command {
        Command::Build(arguments) => {
            let index = load_index(required_index_path(&index_path)?)?;
            let package = arguments
                .package()
                .map(|name| {
                    canonical_package(&index, name).map(|package| package.package_id.as_str())
                })
                .transpose()?;
            resolution::authorize_command(&root, resolution_plan, "build", package)?;
            return action::run_action(&root, &index, "build", arguments);
        }
        Command::Test(arguments) => {
            let index = load_index(required_index_path(&index_path)?)?;
            let package = arguments
                .package()
                .map(|name| {
                    canonical_package(&index, name).map(|package| package.package_id.as_str())
                })
                .transpose()?;
            resolution::authorize_command(&root, resolution_plan, "test", package)?;
            return action::run_action(&root, &index, "test", arguments);
        }
        Command::Benchmark(arguments) => return benchmark::run(&root, benchmark_plan, arguments),
        Command::Cache {
            command: CacheCommand::Coverage(arguments),
        } => return host::cache_coverage(&root, &host, arguments),
        Command::Doctor(arguments) => return host::doctor(&root, &host, arguments),
        Command::Setup(arguments) => return host::setup(&root, &host, arguments),
        Command::Sync(arguments) => return source::sync(&root, source_plan, arguments),
        Command::Status(arguments) => return source::status(&root, source_plan, arguments),
        Command::Update(arguments) => return source::update(&root, source_plan, arguments),
        Command::Package {
            command: PackageCommand::Prefix(arguments),
        } => resolution::package_prefix(&root, resolution_plan, arguments),
        Command::Package { command } => {
            package(&load_index(required_index_path(&index_path)?)?, command)
        }
        Command::Env(arguments) => {
            resolution::explain_environment(&root, resolution_plan, arguments)
        }
        Command::Target { command } => {
            target(&load_index(required_index_path(&index_path)?)?, command)
        }
        Command::Artifact { command } => {
            artifact(&load_index(required_index_path(&index_path)?)?, command)
        }
        Command::Resource(arguments) => resolution::resource(&root, resolution_plan, arguments),
        Command::Executable { command } => {
            executable(&load_index(required_index_path(&index_path)?)?, command)
        }
        Command::Launch { command } => match command {
            LaunchCommand::Up(arguments) => {
                return launch_exec::up(
                    &root,
                    launch_plan,
                    &load_index(required_index_path(&index_path)?)?,
                    arguments,
                )
            }
            LaunchCommand::Sessions => return launch_exec::sessions(&root, launch_plan),
            LaunchCommand::Attach(arguments) => {
                return launch_exec::session_command(&root, launch_plan, "attach", arguments)
            }
            LaunchCommand::Status(arguments) => {
                return launch_exec::session_command(&root, launch_plan, "status", arguments)
            }
            LaunchCommand::Logs(arguments) => {
                return launch_exec::session_command(&root, launch_plan, "logs", arguments)
            }
            LaunchCommand::Down(arguments) => {
                return launch_exec::session_command(&root, launch_plan, "down", arguments)
            }
            query => launch(&load_index(required_index_path(&index_path)?)?, query),
        },
        Command::Graph(arguments) => {
            graph(&load_index(required_index_path(&index_path)?)?, arguments)
        }
        Command::Index { command } => index::run(&root, required_index_path(&index_path)?, command),
        Command::West { command } => west::run(&root, west_plan, command),
        Command::Completion { shell } => completion(shell),
        Command::Complete { words } => complete(required_index_path(&index_path)?, &words),
        Command::RunTask(arguments) => return action::run_task(&root, arguments),
        Command::MaterializeClosure(arguments) => closure_materialization::materialize(arguments),
        Command::Run(arguments) => return resolution::run(&root, resolution_plan, arguments),
        Command::Probe(arguments) => return launch_exec::probe(arguments),
    }
    .map(|()| action::Outcome::Success)
}

fn completion(shell: Shell) -> Result<()> {
    let mut command = Cli::command();
    generate(shell, &mut command, "nixspace", &mut io::stdout());
    Ok(())
}

fn workspace_root(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(root) =
        explicit.or_else(|| env::var_os("NIXSPACE_WORKSPACE_ROOT").map(PathBuf::from))
    {
        return absolute_path(root);
    }
    let root = env::current_dir().map_err(|error| {
        CliError(format!(
            "cannot determine current workspace directory: {error}"
        ))
    })?;
    absolute_path(root)
}

fn absolute_path(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        env::current_dir()
            .map(|current| current.join(path))
            .map_err(|error| CliError(format!("cannot resolve workspace path: {error}")))
    }
}

fn index_path(explicit: Option<PathBuf>, root: &Path) -> Option<PathBuf> {
    explicit
        .or_else(|| env::var_os("NIXSPACE_INDEX").map(PathBuf::from))
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        })
}

fn required_index_path(path: &Option<PathBuf>) -> Result<&Path> {
    path.as_deref().ok_or_else(|| {
        CliError(
            "workspace index location is not configured; pass --index or set NIXSPACE_INDEX to a Nix-generated index path"
                .into(),
        )
    })
}

fn load_index(path: &Path) -> Result<Index> {
    let contents = fs::read(path).map_err(|error| {
        CliError(format!(
            "workspace index is unavailable at {}: {error}; run `nixspace index refresh` or set NIXSPACE_INDEX",
            path.display()
        ))
    })?;
    decode_index(&contents, &path.display().to_string())
}

fn decode_index(contents: &[u8], source: &str) -> Result<Index> {
    let index: Index = serde_json::from_slice(contents).map_err(|error| {
        CliError(format!(
            "workspace index at {source} is unreadable: {error}"
        ))
    })?;
    if index.api_version != SUPPORTED_API_VERSION {
        return Err(CliError(format!(
            "workspace API `{}` is unsupported; this nixspace requires `{SUPPORTED_API_VERSION}`",
            index.api_version
        )));
    }
    if index.kind != SUPPORTED_KIND {
        return Err(CliError(format!(
            "workspace kind `{}` is unsupported; this nixspace requires `{SUPPORTED_KIND}`",
            index.kind
        )));
    }
    if index.interface_version != SUPPORTED_INTERFACE_VERSION {
        return Err(CliError(format!(
            "workspace interface version {} is unsupported; this nixspace supports version {}",
            index.interface_version, SUPPORTED_INTERFACE_VERSION
        )));
    }
    if index.graph.schema_version != 1 {
        return Err(CliError(format!(
            "graph interface version {} is unsupported; this nixspace supports version 1",
            index.graph.schema_version
        )));
    }
    if index.action_plans.schema_version != 1 {
        return Err(CliError(format!(
            "action-plan interface version {} is unsupported; this nixspace supports version 1",
            index.action_plans.schema_version
        )));
    }
    Ok(index)
}

fn canonical_package<'a>(index: &'a Index, name: &str) -> Result<&'a PackageDocument> {
    index
        .catalog
        .packages
        .iter()
        .find(|package| {
            package.package_id == name || package.aliases.iter().any(|alias| alias == name)
        })
        .ok_or_else(|| {
            CliError(format!(
                "package `{name}` is not in the cached workspace index"
            ))
        })
}

fn package(index: &Index, command: PackageCommand) -> Result<()> {
    match command {
        PackageCommand::Prefix(_) => {
            unreachable!("package prefix is dispatched through the resolution plan")
        }
        PackageCommand::List { json: true } => write_json(&json!({
            "interfaceVersion": index.interface_version,
            "count": index.catalog.packages.len(),
            "packages": index.catalog.packages,
        })),
        PackageCommand::List { json: false } => {
            println!(
                "PACKAGE\tALIASES\tVERSION\tLIFECYCLE\tDEPLOYABILITY\tOWNER\tLICENSE\tWARNINGS"
            );
            for package in &index.catalog.packages {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    package.package_id,
                    display_list(&package.aliases),
                    software_version(package),
                    package.lifecycle,
                    package.deployability,
                    package.owner.as_deref().unwrap_or("-"),
                    package.license.spdx.as_deref().unwrap_or("-"),
                    display_list(&package.compliance.warnings),
                );
            }
            Ok(())
        }
        PackageCommand::Show { coordinate, json } => {
            let package = canonical_package(index, &coordinate)?;
            if json {
                return write_json(package);
            }
            println!("Package: {}", package.package_id);
            println!("Aliases: {}", display_words(&package.aliases));
            println!("Version: {}", software_version(package));
            println!("Lifecycle: {}", package.lifecycle);
            println!("Deployability: {}", package.deployability);
            println!("Owner: {}", package.owner.as_deref().unwrap_or("none"));
            println!(
                "License: {}",
                package.license.spdx.as_deref().unwrap_or("none")
            );
            println!("Project: {}", package.id);
            println!("Repository: {}", package.repository_id);
            println!("Preset: {}", package.preset);
            println!("Source: {}:{}", package.source.input, package.source.root);
            println!(
                "Bespoke adapters: {}",
                package.compliance.bespoke_adapter_count
            );
            Ok(())
        }
    }
}

fn target(index: &Index, command: CollectionCommand) -> Result<()> {
    match command {
        CollectionCommand::List { package, json } => {
            let canonical = canonical_filter(index, package.as_deref())?;
            let documents: Vec<_> = index
                .catalog
                .targets
                .iter()
                .filter(|document| canonical.is_none_or(|id| document.package_id == id))
                .collect();
            if json {
                return write_json(
                    &json!({"interfaceVersion": index.interface_version, "count": documents.len(), "targets": documents}),
                );
            }
            println!("TARGET\tACTIONS\tOUTPUTS\tINPUTS");
            for document in documents {
                println!(
                    "{}\t{}\t{}\t{}",
                    document.coordinate,
                    document.target.actions.len(),
                    document.target.artifacts.outputs.len(),
                    document.target.artifacts.inputs.len()
                );
            }
            Ok(())
        }
        CollectionCommand::Show { coordinate, json } => {
            let document = index
                .catalog
                .targets
                .iter()
                .find(|document| document.coordinate == coordinate)
                .ok_or_else(|| {
                    CliError(format!(
                        "target `{coordinate}` is not in the cached workspace index"
                    ))
                })?;
            if json {
                return write_json(document);
            }
            println!("Target: {}", document.coordinate);
            println!("Actions: {}", display_keys(&document.target.actions));
            println!(
                "Artifact outputs: {}",
                display_keys(&document.target.artifacts.outputs)
            );
            println!(
                "Artifact inputs: {}",
                display_keys(&document.target.artifacts.inputs)
            );
            Ok(())
        }
    }
}

fn artifact(index: &Index, command: CollectionCommand) -> Result<()> {
    match command {
        CollectionCommand::List { package, json } => {
            let canonical = canonical_filter(index, package.as_deref())?;
            let documents: Vec<_> = index
                .catalog
                .artifacts
                .iter()
                .filter(|document| canonical.is_none_or(|id| document.package_id == id))
                .collect();
            if json {
                return write_json(
                    &json!({"interfaceVersion": index.interface_version, "count": documents.len(), "artifacts": documents}),
                );
            }
            println!("ARTIFACT\tKIND\tCONTRACT\tPATH");
            for document in documents {
                println!(
                    "{}\t{}\t{}@{}\t{}",
                    document.coordinate,
                    document.kind,
                    document.contract.name,
                    document.contract.version,
                    document.path
                );
            }
            Ok(())
        }
        CollectionCommand::Show { coordinate, json } => {
            let document = index
                .catalog
                .artifacts
                .iter()
                .find(|document| document.coordinate == coordinate)
                .ok_or_else(|| {
                    CliError(format!(
                        "artifact `{coordinate}` is not in the cached workspace index"
                    ))
                })?;
            if json {
                return write_json(document);
            }
            println!("Artifact: {}", document.coordinate);
            println!("Kind: {}", document.kind);
            println!("Path: {}", document.path);
            println!(
                "Contract: {}@{}",
                document.contract.name, document.contract.version
            );
            Ok(())
        }
    }
}

fn executable(index: &Index, command: CollectionCommand) -> Result<()> {
    match command {
        CollectionCommand::List { package, json } => {
            let canonical = canonical_filter(index, package.as_deref())?;
            let documents: Vec<_> = index
                .catalog
                .executables
                .iter()
                .filter(|document| canonical.is_none_or(|id| document.package_id == id))
                .collect();
            if json {
                return write_json(
                    &json!({"interfaceVersion": index.interface_version, "count": documents.len(), "executables": documents}),
                );
            }
            println!("EXECUTABLE\tARTIFACT\tARGV");
            for document in documents {
                println!(
                    "{}\t{}\t{}",
                    document.coordinate,
                    document.from,
                    serde_json::to_string(&document.argv).expect("argv is serializable")
                );
            }
            Ok(())
        }
        CollectionCommand::Show { coordinate, json } => {
            let document = index
                .catalog
                .executables
                .iter()
                .find(|document| document.coordinate == coordinate)
                .ok_or_else(|| {
                    CliError(format!(
                        "executable `{coordinate}` is not in the cached workspace index"
                    ))
                })?;
            if json {
                return write_json(document);
            }
            println!("Executable: {}", document.coordinate);
            println!("Artifact: {}", document.from);
            println!(
                "Argv: {}",
                serde_json::to_string(&document.argv).expect("argv is serializable")
            );
            Ok(())
        }
    }
}

fn launch(index: &Index, command: LaunchCommand) -> Result<()> {
    match command {
        LaunchCommand::List { package, json } => {
            let canonical = canonical_filter(index, package.as_deref())?;
            let documents: Vec<_> = index
                .catalog
                .launches
                .iter()
                .filter(|document| canonical.is_none_or(|id| document.package_id == id))
                .collect();
            if json {
                return write_json(
                    &json!({"interfaceVersion": index.interface_version, "count": documents.len(), "launches": documents}),
                );
            }
            println!("LAUNCH\tPROCESSES\tINCLUDES\tDESCRIPTION");
            for document in documents {
                println!(
                    "{}\t{}\t{}\t{}",
                    document.coordinate,
                    document.launch.processes.len(),
                    document.launch.includes.len(),
                    document.launch.description
                );
            }
            Ok(())
        }
        LaunchCommand::Show { coordinate, json } => {
            let document = index
                .catalog
                .launches
                .iter()
                .find(|document| document.coordinate == coordinate)
                .ok_or_else(|| CliError(format!("launch `{coordinate}` is not in the cached workspace index; use package/launch coordinates")))?;
            if json {
                return write_json(document);
            }
            println!("Launch: {}", document.coordinate);
            println!("Description: {}", document.launch.description);
            println!(
                "Required artifacts: {}",
                display_words(&document.launch.required_artifacts)
            );
            println!(
                "Required resources: {}",
                display_words(&document.launch.required_resources)
            );
            Ok(())
        }
        LaunchCommand::Plan {
            coordinate,
            assignments,
            json,
        } => {
            let template = index.launch_plans.get(&coordinate).ok_or_else(|| {
                CliError(format!(
                    "launch `{coordinate}` is not in the cached workspace index; use package/launch coordinates"
                ))
            })?;
            let plan = launch::build_plan(index.interface_version, template, &assignments)
                .map_err(CliError)?;
            if json {
                write_json(&plan)
            } else {
                launch::emit_human(&plan);
                Ok(())
            }
        }
        LaunchCommand::Up(_)
        | LaunchCommand::Sessions
        | LaunchCommand::Attach(_)
        | LaunchCommand::Status(_)
        | LaunchCommand::Logs(_)
        | LaunchCommand::Down(_) => {
            unreachable!("launch execution is dispatched before query handling")
        }
    }
}

fn graph(index: &Index, arguments: GraphArgs) -> Result<()> {
    if arguments.reverse && arguments.package.is_none() {
        return Err(CliError(
            "reverse dependency graph requires a package; use `nixspace graph PACKAGE --reverse`"
                .into(),
        ));
    }
    let document = if let Some(package) = arguments.package.as_deref() {
        let canonical = &canonical_package(index, package)?.package_id;
        let collection = if arguments.reverse {
            &index.graph.reverse
        } else {
            &index.graph.packages
        };
        collection.get(canonical).ok_or_else(|| {
            CliError(format!(
                "package `{canonical}` has no normalized graph document"
            ))
        })?
    } else {
        &index.graph.all
    };
    if arguments.json {
        write_json(document)
    } else if arguments.dot {
        emit_dot(document);
        Ok(())
    } else {
        emit_graph(document);
        Ok(())
    }
}

fn emit_dot(document: &GraphDocument) {
    println!("digraph nixspace {{");
    for node in &document.nodes {
        let shape = if node.node_type == "action" {
            "ellipse"
        } else {
            "box"
        };
        println!("  \"{}\" [shape={shape}];", node.id);
    }
    for edge in &document.edges {
        let label = edge.producer_artifact.as_deref().unwrap_or(&edge.kind);
        println!(
            "  \"{}\" -> \"{}\" [label=\"{label}\"];",
            edge.from, edge.to
        );
    }
    println!("}}");
}

fn emit_graph(document: &GraphDocument) {
    for node in &document.nodes {
        println!("{}", node.id);
    }
    for edge in &document.edges {
        if let Some(artifact) = &edge.producer_artifact {
            println!(
                "{} -> {} [artifact {} -> {}]",
                edge.from,
                edge.to,
                artifact,
                edge.consumer_input.as_deref().unwrap_or("")
            );
        } else {
            println!("{} -> {} [action]", edge.from, edge.to);
        }
    }
}

fn complete(index_path: &Path, words: &[String]) -> Result<()> {
    let completed = words
        .get(..words.len().saturating_sub(1))
        .unwrap_or_default();
    if completed.is_empty() {
        print_lines(TOP_LEVEL_COMPLETIONS);
        return Ok(());
    }

    let command = completed[0].as_str();
    if matches!(command, "build" | "test") {
        let index = load_index(index_path)?;
        for option in ["--plan"] {
            if !completed.iter().any(|word| word == option) {
                println!("{option}");
            }
        }
        if completed.iter().any(|word| word == "--plan")
            && !completed.iter().any(|word| word == "--json")
        {
            println!("--json");
        }
        if !completed[1..].iter().any(|word| !word.starts_with('-')) {
            print_owned_lines(package_identities(&index));
        }
        return Ok(());
    }
    if command == "doctor" {
        println!("--json");
        return Ok(());
    }
    if command == "setup" {
        println!("--check");
        return Ok(());
    }
    if completed.len() == 1 {
        match command {
            "package" => print_lines(&["list", "show", "prefix"]),
            "target" | "artifact" | "executable" => print_lines(&["list", "show"]),
            "resource" => {
                let index = load_index(index_path)?;
                println!("--json");
                print_owned_lines(
                    index
                        .catalog
                        .resources
                        .iter()
                        .map(|item| item.coordinate.clone()),
                );
            }
            "env" => {
                let index = load_index(index_path)?;
                println!("--explain");
                print_owned_lines(package_identities(&index));
            }
            "run" => {
                let index = load_index(index_path)?;
                println!("--selection-root");
                print_owned_lines(
                    index
                        .catalog
                        .executables
                        .iter()
                        .map(|item| item.coordinate.clone()),
                );
            }
            "launch" => print_lines(&["list", "show", "plan"]),
            "index" => print_lines(&["refresh", "status", "path"]),
            "west" => print_lines(&[
                "validate",
                "ensure",
                "sync",
                "status",
                "path",
                "extra-modules",
                "exec",
            ]),
            _ => {}
        }
        return Ok(());
    }

    let subcommand = completed[1].as_str();
    if command == "package" && subcommand == "prefix" {
        if completed.len() == 2 {
            let index = load_index(index_path)?;
            print_owned_lines(package_identities(&index));
        } else {
            println!("--json");
        }
        return Ok(());
    }
    if command == "resource" || command == "env" {
        println!("--json");
        if command == "env" {
            println!("--explain");
        }
        return Ok(());
    }
    if command == "run" {
        println!("--selection-root");
        return Ok(());
    }
    if subcommand == "list" && completed.len() == 2 {
        println!("--json");
        if command != "package" {
            let index = load_index(index_path)?;
            for identity in package_identities(&index) {
                println!("{identity}");
            }
        }
        return Ok(());
    }
    if subcommand == "show" && completed.len() == 2 {
        let index = load_index(index_path)?;
        match command {
            "package" => print_owned_lines(package_identities(&index)),
            "target" => print_owned_lines(
                index
                    .catalog
                    .targets
                    .iter()
                    .map(|item| item.coordinate.clone()),
            ),
            "artifact" => print_owned_lines(
                index
                    .catalog
                    .artifacts
                    .iter()
                    .map(|item| item.coordinate.clone()),
            ),
            "executable" => print_owned_lines(
                index
                    .catalog
                    .executables
                    .iter()
                    .map(|item| item.coordinate.clone()),
            ),
            "launch" => print_owned_lines(
                index
                    .catalog
                    .launches
                    .iter()
                    .map(|item| item.coordinate.clone()),
            ),
            _ => {}
        }
        return Ok(());
    }
    if command == "launch" && subcommand == "plan" {
        let index = load_index(index_path)?;
        if completed.len() == 2 {
            print_owned_lines(
                index
                    .catalog
                    .launches
                    .iter()
                    .map(|item| item.coordinate.clone()),
            );
            return Ok(());
        }
        if completed.len() >= 4 && completed.last().is_some_and(|word| word == "--set") {
            let template = index.launch_plans.get(&completed[2]).ok_or_else(|| {
                CliError(format!(
                    "launch `{}` is not in the cached workspace index",
                    completed[2]
                ))
            })?;
            let assigned: std::collections::BTreeSet<_> = completed[3..]
                .iter()
                .filter_map(|argument| argument.split_once('=').map(|(name, _)| name))
                .collect();
            for (name, definition) in &template.parameters {
                if definition.parameter_type != model::ParameterType::Secret
                    && !assigned.contains(name.as_str())
                {
                    println!("{name}=");
                }
            }
            return Ok(());
        }
        if !completed[3..].iter().any(|word| word == "--json") {
            println!("--json");
        }
        println!("--set");
        return Ok(());
    }
    if command == "graph" {
        let index = load_index(index_path)?;
        for option in ["--json", "--dot", "--reverse"] {
            if !completed.iter().any(|word| word == option) {
                println!("{option}");
            }
        }
        if !completed[1..].iter().any(|word| !word.starts_with('-')) {
            print_owned_lines(package_identities(&index));
        }
        return Ok(());
    }
    println!("--json");
    Ok(())
}

fn canonical_filter<'a>(index: &'a Index, package: Option<&str>) -> Result<Option<&'a str>> {
    package
        .map(|name| canonical_package(index, name).map(|package| package.package_id.as_str()))
        .transpose()
}

fn package_identities(index: &Index) -> Vec<String> {
    let mut identities: Vec<_> = index
        .catalog
        .packages
        .iter()
        .flat_map(|package| {
            std::iter::once(package.package_id.clone()).chain(package.aliases.iter().cloned())
        })
        .collect();
    identities.sort();
    identities.dedup();
    identities
}

fn software_version(package: &PackageDocument) -> String {
    match package.software_version.source.as_str() {
        "literal" => package.software_version.value.clone().unwrap_or_default(),
        "file" => format!(
            "file:{}",
            package.software_version.file.as_deref().unwrap_or_default()
        ),
        _ => "native".into(),
    }
}

fn display_list(values: &[String]) -> String {
    if values.is_empty() {
        "-".into()
    } else {
        values.join(",")
    }
}

fn display_words(values: &[String]) -> String {
    if values.is_empty() {
        "none".into()
    } else {
        values.join(", ")
    }
}

fn display_keys<T>(values: &std::collections::BTreeMap<String, T>) -> String {
    display_words(&values.keys().cloned().collect::<Vec<_>>())
}

fn write_json(value: &impl Serialize) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    serde_json::to_writer(&mut handle, value)
        .map_err(|error| CliError(format!("cannot serialize normalized document: {error}")))?;
    writeln!(handle).map_err(|error| CliError(format!("cannot write output: {error}")))
}

fn print_lines(values: &[&str]) {
    for value in values {
        println!("{value}");
    }
}

fn print_owned_lines(values: impl IntoIterator<Item = String>) {
    for value in values {
        println!("{value}");
    }
}
