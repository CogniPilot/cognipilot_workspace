use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use clap::Args;
use serde::{Deserialize, Serialize};

use crate::action::Outcome;
use crate::{CliError, Result};

const API_VERSION: &str = "nixspace/v1";
const PLAN_KIND: &str = "BenchmarkPlan";
const PLAN_INTERFACE_VERSION: u64 = 3;
const REPORT_INTERFACE_VERSION: u64 = 3;
const MAX_SAMPLES_PER_PHASE: u64 = 1000;

#[derive(Debug, Args)]
pub(crate) struct BenchmarkArgs {
    /// Run only these exact benchmark case IDs. The default is declared by Nix.
    #[arg(value_name = "CASE")]
    cases: Vec<String>,

    /// Run every case declared by the plan instead of its default selection.
    #[arg(long, conflicts_with = "cases")]
    all: bool,

    /// Override the reference host name recorded in this report only.
    #[arg(long, value_name = "NAME", value_parser = parse_host_name)]
    host_name: Option<String>,

    /// Write the versioned report here instead of under the plan's state root.
    #[arg(long, value_name = "PATH")]
    output: Option<PathBuf>,

    /// Emit the complete report on stdout in addition to writing it to disk.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BenchmarkPlan {
    api_version: String,
    kind: String,
    interface_version: u64,
    id: String,
    reference: ReferenceHost,
    context: BTreeMap<String, String>,
    state_root: PathBuf,
    default_cases: Vec<String>,
    cases: BTreeMap<String, BenchmarkCase>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceHost {
    name: String,
    class: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BenchmarkCase {
    description: String,
    context: BTreeMap<String, String>,
    setup: Vec<CommandSpec>,
    before_each: Vec<CommandSpec>,
    measure: Vec<CommandSpec>,
    after_each: Vec<CommandSpec>,
    teardown: Vec<CommandSpec>,
    warmup_samples: u64,
    measured_samples: u64,
    gates: Gates,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommandSpec {
    argv: Vec<String>,
    cwd: PathBuf,
    environment: BTreeMap<String, String>,
    inherit_environment: bool,
    timeout_milliseconds: u64,
    expected_exit_codes: Vec<i32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Gates {
    p50_milliseconds: Option<u64>,
    p95_milliseconds: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchmarkReport {
    api_version: &'static str,
    kind: &'static str,
    interface_version: u64,
    path_base: &'static str,
    plan_id: String,
    plan_path: PathBuf,
    report_path: PathBuf,
    workspace: PathBuf,
    reference: ReferenceHost,
    context: BTreeMap<String, String>,
    started_at_unix_milliseconds: u128,
    duration_nanoseconds: u128,
    passed: bool,
    cases: Vec<CaseReport>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CaseReport {
    id: String,
    description: String,
    context: BTreeMap<String, String>,
    commands: LifecycleCommands,
    setup: Vec<CommandExecution>,
    warmups: Vec<SampleRecord>,
    samples: Vec<SampleRecord>,
    teardown: Vec<CommandExecution>,
    statistics: Statistics,
    gates: Vec<GateResult>,
    passed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LifecycleCommands {
    setup: Vec<CommandSpec>,
    before_each: Vec<CommandSpec>,
    measure: Vec<CommandSpec>,
    after_each: Vec<CommandSpec>,
    teardown: Vec<CommandSpec>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SampleRecord {
    index: u64,
    duration_nanoseconds: Option<u128>,
    before_each: Vec<CommandExecution>,
    measure: Vec<CommandExecution>,
    after_each: Vec<CommandExecution>,
    passed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandExecution {
    duration_nanoseconds: u128,
    status: SampleStatus,
    stdout_log: PathBuf,
    stderr_log: PathBuf,
    infrastructure_errors: Vec<String>,
}

impl CommandExecution {
    fn passed(&self) -> bool {
        self.infrastructure_errors.is_empty() && self.status.passed()
    }
}

#[derive(Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
enum SampleStatus {
    Exited {
        success: bool,
        code: Option<i32>,
        expected: bool,
    },
    SpawnError {
        message: String,
    },
    TimedOut {
        timeout_milliseconds: u64,
    },
}

impl SampleStatus {
    fn passed(&self) -> bool {
        matches!(self, Self::Exited { expected: true, .. })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Statistics {
    p50_nanoseconds: Option<u128>,
    p95_nanoseconds: Option<u128>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GateResult {
    statistic: &'static str,
    actual_nanoseconds: Option<u128>,
    limit_milliseconds: u64,
    passed: bool,
}

pub(crate) fn run(
    workspace_root: &Path,
    explicit_plan: Option<PathBuf>,
    arguments: BenchmarkArgs,
) -> Result<Outcome> {
    let plan_path = plan_path(workspace_root, explicit_plan)?;
    let plan = load_plan(&plan_path)?;
    validate_plan(&plan)?;
    let selected = select_cases(&plan, &arguments.cases, arguments.all)?;
    let mut reference = plan.reference.clone();
    if let Some(host_name) = arguments.host_name {
        reference.name = host_name;
    }
    let started_at_unix_milliseconds = unix_duration()?.as_millis();
    let run_started = Instant::now();
    let report_path = report_path(workspace_root, &plan, arguments.output)?;
    if report_path.exists() {
        return Err(CliError(format!(
            "benchmark report path already exists: {}",
            report_path.display()
        )));
    }
    let log_root = log_root(&report_path)?;
    // Per-command execution records log-directory failures and falls back to
    // null output. Do not let diagnostics suppress lifecycle cleanup.
    let _ = fs::create_dir_all(&log_root);

    let mut case_reports = Vec::with_capacity(selected.len());
    for (id, case) in selected {
        case_reports.push(run_case(workspace_root, &log_root, &id, case));
    }
    let passed = case_reports.iter().all(|case| case.passed);
    make_log_paths_portable(&mut case_reports, workspace_root);
    let report = BenchmarkReport {
        api_version: API_VERSION,
        kind: "BenchmarkReport",
        interface_version: REPORT_INTERFACE_VERSION,
        path_base: "workspace",
        plan_id: plan.id,
        plan_path: portable_path(&plan_path, workspace_root),
        report_path: portable_path(&report_path, workspace_root),
        workspace: PathBuf::from("."),
        reference,
        context: plan.context,
        started_at_unix_milliseconds,
        duration_nanoseconds: run_started.elapsed().as_nanos(),
        passed,
        cases: case_reports,
    };
    write_report(&report_path, &report)?;
    if arguments.json {
        crate::write_json(&report)?;
    } else {
        emit_human(&report);
    }
    Ok(if passed {
        Outcome::Success
    } else {
        Outcome::Exit(1)
    })
}

fn plan_path(root: &Path, explicit: Option<PathBuf>) -> Result<PathBuf> {
    let path = explicit
        .or_else(|| env::var_os("NIXSPACE_BENCHMARK_PLAN").map(PathBuf::from))
        .ok_or_else(|| {
            CliError(
                "benchmark plan location is not configured; pass --benchmark-plan or set NIXSPACE_BENCHMARK_PLAN to a Nix-generated plan"
                    .into(),
            )
        })?;
    Ok(if path.is_absolute() {
        path
    } else {
        root.join(path)
    })
}

fn load_plan(path: &Path) -> Result<BenchmarkPlan> {
    let contents = fs::read(path).map_err(|error| {
        CliError(format!(
            "benchmark plan is unavailable at {}: {error}",
            path.display()
        ))
    })?;
    serde_json::from_slice(&contents).map_err(|error| {
        CliError(format!(
            "benchmark plan at {} is unreadable: {error}",
            path.display()
        ))
    })
}

fn validate_plan(plan: &BenchmarkPlan) -> Result<()> {
    if plan.api_version != API_VERSION {
        return Err(CliError(format!(
            "benchmark API `{}` is unsupported; expected `{API_VERSION}`",
            plan.api_version
        )));
    }
    if plan.kind != PLAN_KIND {
        return Err(CliError(format!(
            "benchmark plan kind `{}` is unsupported; expected `{PLAN_KIND}`",
            plan.kind
        )));
    }
    if plan.interface_version != PLAN_INTERFACE_VERSION {
        return Err(CliError(format!(
            "benchmark interface version {} is unsupported; expected {PLAN_INTERFACE_VERSION}",
            plan.interface_version
        )));
    }
    if plan.id.is_empty() {
        return Err(CliError("benchmark plan id must not be empty".into()));
    }
    if plan.reference.name.is_empty() || plan.reference.class.is_empty() {
        return Err(CliError(
            "benchmark reference host name and class must not be empty".into(),
        ));
    }
    if plan.state_root == Path::new(".") || !safe_relative_path(&plan.state_root) {
        return Err(CliError(format!(
            "benchmark state root must be a safe non-empty relative path: {}",
            plan.state_root.display()
        )));
    }
    if plan.cases.is_empty() {
        return Err(CliError(
            "benchmark plan must declare at least one case".into(),
        ));
    }
    if plan.default_cases.is_empty() {
        return Err(CliError(
            "benchmark plan defaultCases must declare at least one case ID".into(),
        ));
    }
    let mut default_cases = BTreeSet::new();
    for id in &plan.default_cases {
        if !default_cases.insert(id) {
            return Err(CliError(format!(
                "benchmark plan defaultCases contains duplicate case `{id}`"
            )));
        }
        if !plan.cases.contains_key(id) {
            return Err(CliError(format!(
                "benchmark plan defaultCases references unknown case `{id}`"
            )));
        }
    }
    for (id, case) in &plan.cases {
        if !safe_id(id) {
            return Err(CliError(format!(
                "benchmark case id `{id}` must contain only letters, digits, `.`, `_`, or `-`"
            )));
        }
        if case.description.is_empty() {
            return Err(CliError(format!(
                "benchmark case `{id}` description must not be empty"
            )));
        }
        if case.measure.is_empty() {
            return Err(CliError(format!(
                "benchmark case `{id}` measure must declare at least one command"
            )));
        }
        if case.warmup_samples > MAX_SAMPLES_PER_PHASE {
            return Err(CliError(format!(
                "benchmark case `{id}` warmupSamples must not exceed {MAX_SAMPLES_PER_PHASE}"
            )));
        }
        if case.measured_samples == 0 || case.measured_samples > MAX_SAMPLES_PER_PHASE {
            return Err(CliError(format!(
                "benchmark case `{id}` measuredSamples must be between 1 and {MAX_SAMPLES_PER_PHASE}"
            )));
        }
        for (phase, commands) in [
            ("setup", &case.setup),
            ("beforeEach", &case.before_each),
            ("measure", &case.measure),
            ("afterEach", &case.after_each),
            ("teardown", &case.teardown),
        ] {
            for (index, command) in commands.iter().enumerate() {
                validate_command(id, phase, index, command)?;
            }
        }
    }
    Ok(())
}

fn validate_command(id: &str, phase: &str, index: usize, command: &CommandSpec) -> Result<()> {
    let label = format!("benchmark case `{id}` {phase} command {}", index + 1);
    if command.argv.first().is_none_or(String::is_empty)
        || command.argv.iter().any(|argument| argument.contains('\0'))
    {
        return Err(CliError(format!(
            "{label} argv must start with a non-empty executable and contain no NUL"
        )));
    }
    if !command.cwd.is_absolute() && !safe_relative_path(&command.cwd) {
        return Err(CliError(format!(
            "{label} cwd must be absolute or a safe relative path: {}",
            command.cwd.display()
        )));
    }
    if command.timeout_milliseconds == 0 {
        return Err(CliError(format!(
            "{label} timeoutMilliseconds must be positive"
        )));
    }
    if command.expected_exit_codes.is_empty() {
        return Err(CliError(format!(
            "{label} expectedExitCodes must not be empty"
        )));
    }
    let mut expected_exit_codes = BTreeSet::new();
    if command
        .expected_exit_codes
        .iter()
        .any(|code| !expected_exit_codes.insert(*code))
    {
        return Err(CliError(format!(
            "{label} expectedExitCodes must contain unique signed 32-bit values"
        )));
    }
    if command.environment.iter().any(|(name, value)| {
        name.is_empty() || name.contains('=') || name.contains('\0') || value.contains('\0')
    }) {
        return Err(CliError(format!(
            "{label} has an invalid environment variable name or value"
        )));
    }
    Ok(())
}

fn select_cases<'a>(
    plan: &'a BenchmarkPlan,
    requested: &[String],
    all: bool,
) -> Result<Vec<(String, &'a BenchmarkCase)>> {
    if all {
        return Ok(plan
            .cases
            .iter()
            .map(|(id, case)| (id.clone(), case))
            .collect());
    }
    let requested = if requested.is_empty() {
        &plan.default_cases
    } else {
        requested
    };
    let mut seen = BTreeSet::new();
    let mut selected = Vec::with_capacity(requested.len());
    for id in requested {
        if !seen.insert(id) {
            return Err(CliError(format!(
                "benchmark case `{id}` was selected more than once"
            )));
        }
        let case = plan
            .cases
            .get(id)
            .ok_or_else(|| CliError(format!("benchmark plan has no case `{id}`")))?;
        selected.push((id.clone(), case));
    }
    Ok(selected)
}

fn parse_host_name(value: &str) -> std::result::Result<String, String> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err("host name must be nonempty and contain no control characters".into());
    }
    Ok(value.to_owned())
}

fn run_case(workspace: &Path, log_root: &Path, id: &str, case: &BenchmarkCase) -> CaseReport {
    let case_log_root = log_root.join(id);
    let _ = fs::create_dir_all(&case_log_root);
    let setup = run_until_failure(workspace, &case_log_root, &case.setup, "case", 0, "setup");
    let setup_passed =
        setup.len() == case.setup.len() && setup.iter().all(CommandExecution::passed);
    let mut warmups = Vec::with_capacity(case.warmup_samples as usize);
    if setup_passed {
        for index in 1..=case.warmup_samples {
            let sample = run_sample(workspace, &case_log_root, case, "warmup", index);
            let lifecycle_failed = lifecycle_failed(&sample);
            warmups.push(sample);
            if lifecycle_failed {
                break;
            }
        }
    }
    let mut samples = Vec::with_capacity(case.measured_samples as usize);
    if setup_passed
        && warmups.len() == case.warmup_samples as usize
        && !warmups.iter().any(lifecycle_failed)
    {
        for index in 1..=case.measured_samples {
            let sample = run_sample(workspace, &case_log_root, case, "sample", index);
            let lifecycle_failed = lifecycle_failed(&sample);
            samples.push(sample);
            if lifecycle_failed {
                break;
            }
        }
    }
    let teardown = run_all(
        workspace,
        &case_log_root,
        &case.teardown,
        "case",
        0,
        "teardown",
    );
    let mut durations: Vec<_> = samples
        .iter()
        .filter_map(|sample| sample.duration_nanoseconds)
        .collect();
    durations.sort_unstable();
    let statistics = Statistics {
        p50_nanoseconds: percentile(&durations, 50),
        p95_nanoseconds: percentile(&durations, 95),
    };
    let mut gates = Vec::new();
    if let Some(limit) = case.gates.p50_milliseconds {
        gates.push(gate("p50", statistics.p50_nanoseconds, limit));
    }
    if let Some(limit) = case.gates.p95_milliseconds {
        gates.push(gate("p95", statistics.p95_nanoseconds, limit));
    }
    let passed = setup_passed
        && warmups.len() == case.warmup_samples as usize
        && samples.len() == case.measured_samples as usize
        && warmups.iter().all(|sample| sample.passed)
        && samples.iter().all(|sample| sample.passed)
        && teardown.iter().all(CommandExecution::passed)
        && gates.iter().all(|gate| gate.passed);
    CaseReport {
        id: id.to_owned(),
        description: case.description.clone(),
        context: case.context.clone(),
        commands: LifecycleCommands {
            setup: case.setup.clone(),
            before_each: case.before_each.clone(),
            measure: case.measure.clone(),
            after_each: case.after_each.clone(),
            teardown: case.teardown.clone(),
        },
        setup,
        warmups,
        samples,
        teardown,
        statistics,
        gates,
        passed,
    }
}

fn run_sample(
    workspace: &Path,
    log_root: &Path,
    case: &BenchmarkCase,
    phase: &str,
    index: u64,
) -> SampleRecord {
    let before_each = run_until_failure(
        workspace,
        log_root,
        &case.before_each,
        phase,
        index,
        "before",
    );
    let prepared = before_each.len() == case.before_each.len()
        && before_each.iter().all(CommandExecution::passed);
    let measured_started = Instant::now();
    let measure = if prepared {
        run_until_failure(workspace, log_root, &case.measure, phase, index, "measure")
    } else {
        Vec::new()
    };
    let duration_nanoseconds = prepared.then(|| measured_started.elapsed().as_nanos());
    let after_each = run_all(workspace, log_root, &case.after_each, phase, index, "after");
    let passed = prepared
        && measure.len() == case.measure.len()
        && measure.iter().all(CommandExecution::passed)
        && after_each.iter().all(CommandExecution::passed);
    SampleRecord {
        index,
        duration_nanoseconds,
        before_each,
        measure,
        after_each,
        passed,
    }
}

fn lifecycle_failed(sample: &SampleRecord) -> bool {
    sample.before_each.iter().any(|command| !command.passed())
        || sample.after_each.iter().any(|command| !command.passed())
}

fn run_until_failure(
    workspace: &Path,
    log_root: &Path,
    commands: &[CommandSpec],
    sample_phase: &str,
    sample_index: u64,
    command_phase: &str,
) -> Vec<CommandExecution> {
    let mut executions = Vec::with_capacity(commands.len());
    for (command_index, command) in commands.iter().enumerate() {
        let execution = run_command(
            workspace,
            log_root,
            command,
            sample_phase,
            sample_index,
            command_phase,
            command_index + 1,
        );
        let passed = execution.passed();
        executions.push(execution);
        if !passed {
            break;
        }
    }
    executions
}

fn run_all(
    workspace: &Path,
    log_root: &Path,
    commands: &[CommandSpec],
    sample_phase: &str,
    sample_index: u64,
    command_phase: &str,
) -> Vec<CommandExecution> {
    commands
        .iter()
        .enumerate()
        .map(|(command_index, command)| {
            run_command(
                workspace,
                log_root,
                command,
                sample_phase,
                sample_index,
                command_phase,
                command_index + 1,
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn run_command(
    workspace: &Path,
    log_root: &Path,
    spec: &CommandSpec,
    sample_phase: &str,
    sample_index: u64,
    command_phase: &str,
    command_index: usize,
) -> CommandExecution {
    let cwd = if spec.cwd.is_absolute() {
        spec.cwd.clone()
    } else {
        join_safe_relative(workspace, &spec.cwd)
    };
    let log_prefix = format!("{sample_phase}-{sample_index:03}-{command_phase}-{command_index:03}");
    let stdout_log = log_root.join(format!("{log_prefix}.stdout.log"));
    let stderr_log = log_root.join(format!("{log_prefix}.stderr.log"));
    let mut infrastructure_errors = Vec::new();
    let stdout = match File::create(&stdout_log) {
        Ok(stdout) => Stdio::from(stdout),
        Err(error) => {
            infrastructure_errors.push(format!(
                "cannot create benchmark stdout log {}: {error}",
                stdout_log.display()
            ));
            Stdio::null()
        }
    };
    let stderr = match File::create(&stderr_log) {
        Ok(stderr) => Stdio::from(stderr),
        Err(error) => {
            infrastructure_errors.push(format!(
                "cannot create benchmark stderr log {}: {error}",
                stderr_log.display()
            ));
            Stdio::null()
        }
    };
    let program = resolve_program(&cwd, &spec.argv[0]);
    let mut command = Command::new(program);
    command
        .args(&spec.argv[1..])
        .current_dir(&cwd)
        .stdout(stdout)
        .stderr(stderr);
    if !spec.inherit_environment {
        command.env_clear();
    }
    command.envs(&spec.environment);
    configure_process_tree(&mut command);
    let started = Instant::now();
    let status = match command.spawn() {
        Ok(mut child) => loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let code = status.code();
                    break SampleStatus::Exited {
                        success: status.success(),
                        code,
                        expected: code.is_some_and(|code| spec.expected_exit_codes.contains(&code)),
                    };
                }
                Ok(None)
                    if started.elapsed() >= Duration::from_millis(spec.timeout_milliseconds) =>
                {
                    terminate_process_tree(&mut child, &mut infrastructure_errors, "timed-out");
                    break SampleStatus::TimedOut {
                        timeout_milliseconds: spec.timeout_milliseconds,
                    };
                }
                Ok(None) => thread::sleep(Duration::from_millis(1)),
                Err(error) => {
                    terminate_process_tree(
                        &mut child,
                        &mut infrastructure_errors,
                        "command after wait failure",
                    );
                    break SampleStatus::SpawnError {
                        message: format!("cannot wait for command: {error}"),
                    };
                }
            }
        },
        Err(error) => SampleStatus::SpawnError {
            message: error.to_string(),
        },
    };
    let duration_nanoseconds = started.elapsed().as_nanos();
    CommandExecution {
        duration_nanoseconds,
        status,
        stdout_log,
        stderr_log,
        infrastructure_errors,
    }
}

#[cfg(unix)]
fn configure_process_tree(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

#[cfg(windows)]
fn configure_process_tree(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(CREATE_NEW_PROCESS_GROUP);
}

#[cfg(not(any(unix, windows)))]
fn configure_process_tree(_command: &mut Command) {}

fn terminate_process_tree(child: &mut Child, errors: &mut Vec<String>, description: &str) {
    if let Err(error) = kill_process_tree(child) {
        errors.push(format!("cannot kill {description} process tree: {error}"));
        if let Err(error) = child.kill() {
            errors.push(format!("cannot kill {description} command: {error}"));
        }
    }
    if let Err(error) = child.wait() {
        errors.push(format!("cannot reap {description} command: {error}"));
    }
}

#[cfg(unix)]
fn kill_process_tree(child: &mut Child) -> io::Result<()> {
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    const SIGKILL: i32 = 9;
    let pid = i32::try_from(child.id())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "child PID exceeds i32"))?;
    // Every benchmark command is its own process group. A negative PID kills
    // the whole group, including descendants that have not deliberately
    // detached themselves.
    if unsafe { kill(-pid, SIGKILL) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn kill_process_tree(child: &mut Child) -> io::Result<()> {
    // taskkill terminates the process tree visible at invocation time. A child
    // that deliberately detaches before that snapshot can escape; callers
    // needing containment against hostile commands must use an OS sandbox or
    // a Windows Job Object outside this portable client.
    let status = Command::new("taskkill")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "taskkill failed with status {status}"
        )))
    }
}

#[cfg(not(any(unix, windows)))]
fn kill_process_tree(child: &mut Child) -> io::Result<()> {
    child.kill()
}

fn make_log_paths_portable(cases: &mut [CaseReport], workspace_root: &Path) {
    for case in cases {
        for execution in case
            .setup
            .iter_mut()
            .chain(case.teardown.iter_mut())
            .chain(case.warmups.iter_mut().flat_map(sample_executions_mut))
            .chain(case.samples.iter_mut().flat_map(sample_executions_mut))
        {
            execution.stdout_log = portable_path(&execution.stdout_log, workspace_root);
            execution.stderr_log = portable_path(&execution.stderr_log, workspace_root);
        }
    }
}

fn sample_executions_mut(sample: &mut SampleRecord) -> impl Iterator<Item = &mut CommandExecution> {
    sample
        .before_each
        .iter_mut()
        .chain(sample.measure.iter_mut())
        .chain(sample.after_each.iter_mut())
}

fn portable_path(path: &Path, base: &Path) -> PathBuf {
    path.strip_prefix(base)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .unwrap_or(path)
        .to_path_buf()
}

fn resolve_program(cwd: &Path, program: &str) -> PathBuf {
    let path = Path::new(program);
    if path.is_absolute() || path.components().count() == 1 {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn percentile(sorted: &[u128], percentile: usize) -> Option<u128> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (percentile * sorted.len()).div_ceil(100);
    Some(sorted[rank.saturating_sub(1)])
}

fn gate(
    statistic: &'static str,
    actual_nanoseconds: Option<u128>,
    limit_milliseconds: u64,
) -> GateResult {
    GateResult {
        statistic,
        actual_nanoseconds,
        limit_milliseconds,
        passed: actual_nanoseconds
            .is_some_and(|actual| actual <= u128::from(limit_milliseconds) * 1_000_000),
    }
}

fn report_path(root: &Path, plan: &BenchmarkPlan, output: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = output {
        return Ok(if path.is_absolute() {
            path
        } else {
            root.join(path)
        });
    }
    let run_id = format!("run-{}-{}", unix_duration()?.as_nanos(), std::process::id());
    Ok(root.join(&plan.state_root).join(run_id).join("report.json"))
}

fn log_root(report_path: &Path) -> Result<PathBuf> {
    let parent = report_path.parent().ok_or_else(|| {
        CliError(format!(
            "benchmark report path has no parent: {}",
            report_path.display()
        ))
    })?;
    let stem = report_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| CliError("benchmark report path must have a UTF-8 file name".into()))?;
    Ok(parent.join(format!("{stem}.logs")))
}

fn write_report(path: &Path, report: &BenchmarkReport) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CliError(format!(
                "cannot create benchmark report directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    let mut contents = serde_json::to_vec_pretty(report)
        .map_err(|error| CliError(format!("cannot encode benchmark report: {error}")))?;
    contents.push(b'\n');
    let temporary = path.with_file_name(format!(
        ".{}.{}-{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("report"),
        std::process::id(),
        unix_duration()?.as_nanos()
    ));
    let mut file = File::options()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| {
            CliError(format!(
                "cannot create temporary benchmark report {}: {error}",
                temporary.display()
            ))
        })?;
    file.write_all(&contents)
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            CliError(format!(
                "cannot publish benchmark report content {}: {error}",
                temporary.display()
            ))
        })?;
    drop(file);
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        CliError(format!(
            "cannot atomically publish benchmark report {}: {error}",
            path.display()
        ))
    })
}

fn emit_human(report: &BenchmarkReport) {
    println!("CASE\tSAMPLES\tP50_MS\tP95_MS\tSTATUS");
    for case in &report.cases {
        println!(
            "{}\t{}\t{}\t{}\t{}",
            case.id,
            case.samples.len(),
            display_milliseconds(case.statistics.p50_nanoseconds),
            display_milliseconds(case.statistics.p95_nanoseconds),
            if case.passed { "pass" } else { "fail" }
        );
    }
    println!("Report: {}", report.report_path.display());
}

fn display_milliseconds(value: Option<u128>) -> String {
    value
        .map(|value| format!("{:.3}", value as f64 / 1_000_000.0))
        .unwrap_or_else(|| "-".into())
}

fn unix_duration() -> Result<Duration> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            CliError(format!(
                "system clock is earlier than the Unix epoch: {error}"
            ))
        })
}

fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path.components().all(|component| {
            matches!(component, Component::Normal(_) | Component::CurDir)
                && !matches!(component, Component::ParentDir)
        })
}

fn join_safe_relative(base: &Path, relative: &Path) -> PathBuf {
    relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(segment) => Some(segment),
            Component::CurDir => None,
            _ => unreachable!("validated safe relative benchmark path"),
        })
        .fold(base.to_path_buf(), |path, segment| path.join(segment))
}
