use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::{Args, Subcommand};
use fs4::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::action::Outcome;
use crate::launch::ResolvedLaunchPlan;
use crate::model::{Index, ParameterType, PortAllocation, PortTransport};
use crate::{CliError, Result};

const SUPPORTED_API_VERSION: &str = "nixspace/v1";
const SUPPORTED_KIND: &str = "LaunchExecution";
const SUPPORTED_INTERFACE_VERSION: u64 = 3;
const SESSION_KIND: &str = "LaunchSession";
const METADATA_FILE: &str = "session.json";
const SOCKET_FILE: &str = "process-compose.sock";
const LOG_FILE: &str = "processes.log";
const SESSION_ENVIRONMENT: &str = "NIXSPACE_SESSION_DIR";

static NEXT_SESSION: AtomicU64 = AtomicU64::new(0);
static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Args)]
pub(crate) struct UpArgs {
    /// Exact Nix-emitted workspace launch coordinate.
    pub(crate) coordinate: String,

    /// Set one declared non-secret runtime parameter.
    #[arg(long = "set", value_name = "NAME=VALUE", action = clap::ArgAction::Append)]
    pub(crate) assignments: Vec<String>,

    /// Use this portable session identifier instead of generating one.
    #[arg(long, value_name = "ID")]
    pub(crate) name: Option<String>,

    /// Start through the exact detached manager command.
    #[arg(long)]
    pub(crate) detach: bool,
}

#[derive(Debug, Args)]
pub(crate) struct SessionArgs {
    /// Exact recorded session identifier.
    pub(crate) session: String,
}

#[derive(Debug, Args)]
pub(crate) struct ProbeArgs {
    #[command(subcommand)]
    pub(crate) probe: Probe,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Probe {
    /// Test exact TCP reachability without polling.
    Tcp(NetworkProbe),
    /// Perform one exact HTTP/1.1 GET and require the declared status.
    Http(HttpProbe),
}

#[derive(Debug, Args)]
pub(crate) struct NetworkProbe {
    #[arg(long)]
    host: String,
    #[arg(long)]
    port: u16,
    #[arg(long, default_value_t = 1000)]
    timeout_ms: u64,
}

#[derive(Debug, Args)]
pub(crate) struct HttpProbe {
    #[command(flatten)]
    network: NetworkProbe,
    #[arg(long)]
    path: String,
    #[arg(long)]
    expect_status: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LaunchExecutionPlan {
    api_version: String,
    kind: String,
    interface_version: u64,
    state_root: PathBuf,
    launches: BTreeMap<String, ExecutionLaunch>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExecutionLaunch {
    coordinate: String,
    workspace_launch: String,
    parameters: BTreeMap<String, ExecutionParameter>,
    declared_ports: Vec<DeclaredPort>,
    session_environment: BTreeMap<String, SessionEnvironment>,
    process_policies: Value,
    runner: LaunchRunner,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeclaredPort {
    launch: String,
    process: String,
    endpoint: String,
    protocol: String,
    transport: PortTransport,
    host: Option<String>,
    port: Option<u16>,
    host_environment: Option<String>,
    port_environment: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionEnvironment {
    base: SessionBase,
    path: PathBuf,
    create: SessionCreate,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum SessionBase {
    Session,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum SessionCreate {
    None,
    Parent,
    Directory,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExecutionParameter {
    coordinate: String,
    workspace_coordinate: String,
    parameter_id: String,
    environment: String,
    #[serde(rename = "type")]
    parameter_type: ParameterType,
    required: bool,
    default: Option<Value>,
    enum_values: Vec<String>,
    minimum: Option<f64>,
    maximum: Option<f64>,
    allowed_roots: Vec<String>,
    must_exist: bool,
    access: String,
    secret: bool,
    allocation: PortAllocation,
    allocation_host: String,
    allocation_transport: PortTransport,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LaunchRunner {
    kind: String,
    working_directory: PathBuf,
    commands: LaunchCommands,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LaunchCommands {
    up: ManagerCommand,
    start: ManagerCommand,
    attach: ManagerCommand,
    status: ManagerCommand,
    logs: ManagerCommand,
    down: ManagerCommand,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManagerCommand {
    argv: Vec<ManagerArgument>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum ManagerArgument {
    Literal(String),
    Runtime(RuntimeArgument),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeArgument {
    runtime: RuntimeValue,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum RuntimeValue {
    SessionSocket,
    SessionLog,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionRecord {
    api_version: String,
    kind: String,
    interface_version: u64,
    id: String,
    coordinate: String,
    schema_coordinate: String,
    created_unix_ms: u128,
    working_directory: PathBuf,
    socket: PathBuf,
    log: PathBuf,
    commands: RecordedCommands,
    parameter_environments: Vec<String>,
    process_policies: Value,
    resolved_launch: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RecordedCommands {
    attach: Vec<String>,
    status: Vec<String>,
    logs: Vec<String>,
    down: Vec<String>,
}

struct LoadedPlan {
    plan: LaunchExecutionPlan,
    state_root: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LaunchReadiness {
    pub(crate) state_root: PathBuf,
    pub(crate) satisfied: bool,
    pub(crate) sessions: Vec<SessionReadiness>,
    pub(crate) ports: Vec<PortReadiness>,
    pub(crate) errors: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionReadiness {
    id: String,
    coordinate: String,
    manager_socket: PathBuf,
    manager_socket_present: bool,
    satisfied: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PortReadiness {
    launch: String,
    process: String,
    endpoint: String,
    protocol: String,
    transport: PortTransport,
    host: String,
    port: u16,
    pub(crate) available: bool,
    claimed_by_sessions: Vec<String>,
    satisfied: bool,
    error: Option<String>,
}

pub(crate) fn up(
    root: &Path,
    explicit_plan: Option<PathBuf>,
    index: &Index,
    arguments: UpArgs,
) -> Result<Outcome> {
    let loaded = load(root, explicit_plan)?;
    let launch = loaded
        .plan
        .launches
        .get(&arguments.coordinate)
        .ok_or_else(|| {
            CliError(format!(
                "launch `{}` has no execution record in the Nix-generated plan",
                arguments.coordinate
            ))
        })?;
    let template = index
        .launch_plans
        .get(&launch.workspace_launch)
        .ok_or_else(|| {
            CliError(format!(
                "launch execution record `{}` refers to missing workspace launch `{}`",
                launch.coordinate, launch.workspace_launch
            ))
        })?;
    let mut ports = LaunchPorts::acquire(&loaded.state_root)?;
    let assignments = automatic_port_assignments(launch, &arguments.assignments, &mut ports)?;
    let resolved = crate::launch::build_plan(index.interface_version, template, &assignments)
        .map_err(CliError)?;
    if resolved.launch != launch.workspace_launch {
        return Err(CliError(format!(
            "resolved launch `{}` does not match execution record `{}`",
            resolved.launch, launch.workspace_launch
        )));
    }
    let mut environment = parameter_environment(root, launch, &resolved)?;
    reserve_declared_ports(launch, &environment, &mut ports)?;
    let session = create_session(&loaded.state_root, &arguments.coordinate, arguments.name)?;
    let mut pending_session = PendingSession::new(session.clone());
    if let Err(error) = session_environment(launch, &session, &mut environment) {
        let _ = fs::remove_dir_all(&session);
        return Err(error);
    }
    let socket = session.join(SOCKET_FILE);
    let log = session.join(LOG_FILE);
    let working_directory = resolve_working_directory(root, &launch.runner.working_directory)?;
    let commands = RecordedCommands {
        attach: resolve_command(&launch.runner.commands.attach, &socket, &log)?,
        status: resolve_command(&launch.runner.commands.status, &socket, &log)?,
        logs: resolve_command(&launch.runner.commands.logs, &socket, &log)?,
        down: resolve_command(&launch.runner.commands.down, &socket, &log)?,
    };
    let command = if arguments.detach {
        resolve_command(&launch.runner.commands.start, &socket, &log)?
    } else {
        resolve_command(&launch.runner.commands.up, &socket, &log)?
    };
    let id = session
        .file_name()
        .and_then(|value| value.to_str())
        .expect("validated session identifier")
        .to_owned();
    let record = SessionRecord {
        api_version: SUPPORTED_API_VERSION.into(),
        kind: SESSION_KIND.into(),
        interface_version: SUPPORTED_INTERFACE_VERSION,
        id: id.clone(),
        coordinate: launch.workspace_launch.clone(),
        schema_coordinate: launch.coordinate.clone(),
        created_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        working_directory: working_directory.clone(),
        socket,
        log,
        commands,
        parameter_environments: environment.keys().cloned().collect(),
        process_policies: launch.process_policies.clone(),
        resolved_launch: serde_json::to_value(&resolved).map_err(|error| {
            CliError(format!("cannot encode redacted launch resolution: {error}"))
        })?,
    };
    atomic_json(&session.join(METADATA_FILE), &record)?;

    println!("{id}");
    io::stdout()
        .flush()
        .map_err(|error| CliError(format!("cannot report launch session: {error}")))?;
    let mut manager = exact_command(&command, &working_directory)?;
    manager
        .envs(&environment)
        .env(SESSION_ENVIRONMENT, &session);
    ports.release_sockets();
    if arguments.detach {
        let status = manager
            .status()
            .map_err(|error| command_start_error(&command, error))?;
        let outcome = status_outcome(status)?;
        if matches!(outcome, Outcome::Success) {
            pending_session.commit();
        }
        Ok(outcome)
    } else {
        replace_or_wait(manager, &command)
    }
}

pub(crate) fn sessions(root: &Path, explicit_plan: Option<PathBuf>) -> Result<Outcome> {
    let loaded = load(root, explicit_plan)?;
    if !loaded.state_root.exists() {
        return Ok(Outcome::Success);
    }
    let mut records = fs::read_dir(&loaded.state_root)
        .map_err(|error| {
            CliError(format!(
                "cannot list launch sessions at {}: {error}",
                loaded.state_root.display()
            ))
        })?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| read_session_at(&entry.path()).ok())
        .collect::<Vec<_>>();
    records.sort_by(|left, right| left.id.cmp(&right.id));
    for record in records {
        println!("{}\t{}", record.id, record.coordinate);
    }
    Ok(Outcome::Success)
}

pub(crate) fn readiness(
    root: &Path,
    explicit_plan: Option<PathBuf>,
    allow_active_sessions: bool,
    require_manager_socket: bool,
    require_available_ports: bool,
) -> Result<LaunchReadiness> {
    let loaded = load(root, explicit_plan)?;
    let mut records = Vec::new();
    let mut errors = Vec::new();
    if loaded.state_root.exists() {
        let entries = fs::read_dir(&loaded.state_root).map_err(|error| {
            CliError(format!(
                "cannot inspect launch sessions at {}: {error}",
                loaded.state_root.display()
            ))
        })?;
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    errors.push(format!(
                        "cannot inspect a launch session directory: {error}"
                    ));
                    continue;
                }
            };
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            match read_session_at(&entry.path()) {
                Ok(record) => records.push(record),
                Err(error) => errors.push(error.0),
            }
        }
    }
    records.sort_by(|left, right| left.id.cmp(&right.id));

    let sessions = records
        .iter()
        .map(|record| {
            let manager_socket_present = record.socket.exists();
            SessionReadiness {
                id: record.id.clone(),
                coordinate: record.coordinate.clone(),
                manager_socket: record.socket.clone(),
                manager_socket_present,
                satisfied: allow_active_sessions
                    && (!require_manager_socket || manager_socket_present),
            }
        })
        .collect::<Vec<_>>();

    let claims = records
        .iter()
        .flat_map(session_port_claims)
        .collect::<Vec<_>>();
    let mut ports = Vec::new();
    for launch in loaded.plan.launches.values() {
        for declared in &launch.declared_ports {
            let (Some(host), Some(port)) = (&declared.host, declared.port) else {
                continue;
            };
            let claimed_by_sessions = claims
                .iter()
                .filter(|claim| claim.port == port && claim.host.as_deref().unwrap_or(host) == host)
                .map(|claim| claim.session.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let (available, error) = port_available(declared.transport, host, port);
            let claimed_by_allowed_session = allow_active_sessions
                && claimed_by_sessions.iter().any(|id| {
                    sessions
                        .iter()
                        .any(|session| &session.id == id && session.satisfied)
                });
            let satisfied = !require_available_ports || available || claimed_by_allowed_session;
            ports.push(PortReadiness {
                launch: declared.launch.clone(),
                process: declared.process.clone(),
                endpoint: declared.endpoint.clone(),
                protocol: declared.protocol.clone(),
                transport: declared.transport,
                host: host.clone(),
                port,
                available,
                claimed_by_sessions,
                satisfied,
                error,
            });
        }
    }
    for claim in &claims {
        let host = claim.host.as_deref().unwrap_or("127.0.0.1");
        if ports
            .iter()
            .any(|port| port.host == host && port.port == claim.port)
        {
            continue;
        }
        let claimed_by_sessions = claims
            .iter()
            .filter(|candidate| {
                candidate.port == claim.port && candidate.host.as_deref().unwrap_or(host) == host
            })
            .map(|candidate| candidate.session.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let (available, error) = port_available(claim.transport, host, claim.port);
        let claimed_by_allowed_session = allow_active_sessions
            && claimed_by_sessions.iter().any(|id| {
                sessions
                    .iter()
                    .any(|session| &session.id == id && session.satisfied)
            });
        ports.push(PortReadiness {
            launch: claim.launch.clone(),
            process: claim.process.clone(),
            endpoint: claim.endpoint.clone(),
            protocol: claim.protocol.clone(),
            transport: claim.transport,
            host: host.into(),
            port: claim.port,
            available,
            claimed_by_sessions,
            satisfied: !require_available_ports || available || claimed_by_allowed_session,
            error,
        });
    }
    ports.sort_by(|left, right| {
        (&left.launch, &left.process, &left.endpoint).cmp(&(
            &right.launch,
            &right.process,
            &right.endpoint,
        ))
    });
    let satisfied = errors.is_empty()
        && sessions.iter().all(|session| session.satisfied)
        && ports.iter().all(|port| port.satisfied);
    Ok(LaunchReadiness {
        state_root: loaded.state_root,
        satisfied,
        sessions,
        ports,
        errors,
    })
}

struct SessionPortClaim {
    session: String,
    launch: String,
    process: String,
    endpoint: String,
    protocol: String,
    host: Option<String>,
    port: u16,
    transport: PortTransport,
}

fn session_port_claims(record: &SessionRecord) -> Vec<SessionPortClaim> {
    record
        .resolved_launch
        .get("processes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|process| {
            let process_id = process
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned();
            process
                .get("endpoints")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
                .map(move |(endpoint, value)| (process_id.clone(), endpoint.clone(), value))
        })
        .filter_map(|(process, endpoint_id, endpoint)| {
            let port = endpoint.get("port")?.as_u64()?;
            let port = u16::try_from(port).ok()?;
            let protocol = endpoint.get("protocol")?.as_str()?.to_owned();
            let transport = match protocol.as_str() {
                "udp" => PortTransport::Udp,
                "tcp" | "http" | "https" => PortTransport::Tcp,
                _ => return None,
            };
            let host = endpoint
                .get("host")
                .and_then(Value::as_str)
                .map(str::to_owned);
            Some(SessionPortClaim {
                session: record.id.clone(),
                launch: record.coordinate.clone(),
                process,
                endpoint: endpoint_id,
                protocol,
                host,
                port,
                transport,
            })
        })
        .collect()
}

fn port_available(transport: PortTransport, host: &str, port: u16) -> (bool, Option<String>) {
    let result = match transport {
        PortTransport::Udp => UdpSocket::bind((host, port)).map(drop),
        PortTransport::Tcp => TcpListener::bind((host, port)).map(drop),
    };
    match result {
        Ok(()) => (true, None),
        Err(error) => (false, Some(error.to_string())),
    }
}

enum PortSocket {
    Tcp { _listener: TcpListener },
    Udp { _socket: UdpSocket },
}

struct PortReservation {
    transport: PortTransport,
    host: String,
    port: u16,
    _socket: PortSocket,
}

struct LaunchPorts {
    _lock: File,
    claimed: BTreeSet<(String, u16)>,
    reservations: Vec<PortReservation>,
}

struct PendingSession {
    path: PathBuf,
    committed: bool,
}

impl PendingSession {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            committed: false,
        }
    }

    fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for PendingSession {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

impl LaunchPorts {
    fn acquire(state_root: &Path) -> Result<Self> {
        fs::create_dir_all(state_root).map_err(|error| {
            CliError(format!(
                "cannot create Nix-selected launch state root at {}: {error}",
                state_root.display()
            ))
        })?;
        let lock_path = state_root.join(".port-allocation.lock");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| {
                CliError(format!(
                    "cannot open launch port-allocation lock at {}: {error}",
                    lock_path.display()
                ))
            })?;
        FileExt::lock(&lock).map_err(|error| {
            CliError(format!(
                "cannot acquire launch port-allocation lock at {}: {error}",
                lock_path.display()
            ))
        })?;
        let mut claimed = BTreeSet::new();
        for entry in fs::read_dir(state_root).map_err(|error| {
            CliError(format!(
                "cannot inspect launch sessions at {}: {error}",
                state_root.display()
            ))
        })? {
            let entry = entry.map_err(|error| {
                CliError(format!(
                    "cannot inspect a launch session directory: {error}"
                ))
            })?;
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let metadata = entry.path().join(METADATA_FILE);
            if !metadata.exists() {
                continue;
            }
            let record = read_session_at(&entry.path())?;
            for claim in session_port_claims(&record) {
                claimed.insert((claim.host.unwrap_or_else(|| "127.0.0.1".into()), claim.port));
            }
        }
        Ok(Self {
            _lock: lock,
            claimed,
            reservations: Vec::new(),
        })
    }

    fn automatic_port(
        &mut self,
        transport: PortTransport,
        host: &str,
        coordinate: &str,
        preferred: Option<u16>,
    ) -> Result<u16> {
        if let Some(port) = preferred {
            if !self.claimed.contains(&(host.to_owned(), port)) {
                if let Ok(reservation) = bind_port(transport, host, port, coordinate) {
                    self.claimed.insert((host.to_owned(), port));
                    self.reservations.push(reservation);
                    return Ok(port);
                }
            }
        }
        let mut rejected = Vec::new();
        for _ in 0..128 {
            let reservation = bind_port(transport, host, 0, coordinate)?;
            if !self.claimed.contains(&(host.to_owned(), reservation.port)) {
                let port = reservation.port;
                self.claimed.insert((host.to_owned(), port));
                self.reservations.push(reservation);
                return Ok(port);
            }
            rejected.push(reservation);
        }
        Err(CliError(format!(
            "cannot allocate an unclaimed automatic port for `{coordinate}` on {host}"
        )))
    }

    fn reserve(
        &mut self,
        transport: PortTransport,
        host: &str,
        port: u16,
        coordinate: &str,
    ) -> Result<()> {
        if self.reservations.iter().any(|reservation| {
            reservation.transport == transport
                && reservation.host == host
                && reservation.port == port
        }) {
            return Ok(());
        }
        if self.claimed.contains(&(host.to_owned(), port)) {
            return Err(CliError(format!(
                "launch port `{coordinate}` requires {host}:{port}, which is claimed by another recorded session"
            )));
        }
        let reservation = bind_port(transport, host, port, coordinate)?;
        self.claimed.insert((host.to_owned(), port));
        self.reservations.push(reservation);
        Ok(())
    }

    fn release_sockets(&mut self) {
        self.reservations.clear();
    }
}

impl Drop for LaunchPorts {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self._lock);
    }
}

fn bind_port(
    transport: PortTransport,
    host: &str,
    port: u16,
    coordinate: &str,
) -> Result<PortReservation> {
    let (port, socket) = match transport {
        PortTransport::Tcp => {
            let listener = TcpListener::bind((host, port)).map_err(|error| {
                CliError(format!(
                    "launch port `{coordinate}` cannot reserve TCP {host}:{port}: {error}"
                ))
            })?;
            let selected = listener.local_addr().map_err(|error| {
                CliError(format!(
                    "launch port `{coordinate}` cannot inspect its TCP reservation: {error}"
                ))
            })?;
            (
                selected.port(),
                PortSocket::Tcp {
                    _listener: listener,
                },
            )
        }
        PortTransport::Udp => {
            let socket = UdpSocket::bind((host, port)).map_err(|error| {
                CliError(format!(
                    "launch port `{coordinate}` cannot reserve UDP {host}:{port}: {error}"
                ))
            })?;
            let selected = socket.local_addr().map_err(|error| {
                CliError(format!(
                    "launch port `{coordinate}` cannot inspect its UDP reservation: {error}"
                ))
            })?;
            (selected.port(), PortSocket::Udp { _socket: socket })
        }
    };
    Ok(PortReservation {
        transport,
        host: host.into(),
        port,
        _socket: socket,
    })
}

fn automatic_port_assignments(
    launch: &ExecutionLaunch,
    requested: &[String],
    ports: &mut LaunchPorts,
) -> Result<Vec<String>> {
    let provided = requested
        .iter()
        .filter_map(|assignment| assignment.split_once('=').map(|(name, _)| name))
        .collect::<BTreeSet<_>>();
    let mut assignments = requested.to_vec();
    for parameter in launch.parameters.values() {
        if parameter.allocation != PortAllocation::Automatic
            || provided.contains(parameter.parameter_id.as_str())
        {
            continue;
        }
        if parameter.workspace_coordinate != launch.workspace_launch {
            return Err(CliError(format!(
                "automatic port parameter `{}:{}` is not forwarded to the root launch",
                parameter.workspace_coordinate, parameter.parameter_id
            )));
        }
        let coordinate = format!(
            "{}:{}",
            parameter.workspace_coordinate, parameter.parameter_id
        );
        let port = ports.automatic_port(
            parameter.allocation_transport,
            &parameter.allocation_host,
            &coordinate,
            parameter
                .default
                .as_ref()
                .and_then(Value::as_u64)
                .and_then(|port| u16::try_from(port).ok()),
        )?;
        assignments.push(format!("{}={port}", parameter.parameter_id));
    }
    Ok(assignments)
}

fn reserve_declared_ports(
    launch: &ExecutionLaunch,
    environment: &BTreeMap<String, String>,
    ports: &mut LaunchPorts,
) -> Result<()> {
    for declared in &launch.declared_ports {
        let host = match &declared.host_environment {
            Some(name) => environment.get(name).cloned().ok_or_else(|| {
                CliError(format!(
                    "launch port `{}:{}` has no resolved host environment `{name}`",
                    declared.process, declared.endpoint
                ))
            })?,
            None => declared.host.clone().ok_or_else(|| {
                CliError(format!(
                    "launch port `{}:{}` has neither a host nor a host parameter",
                    declared.process, declared.endpoint
                ))
            })?,
        };
        let port = if let Some(value) = environment.get(&declared.port_environment) {
            value.parse::<u16>().map_err(|_| {
                CliError(format!(
                    "launch port `{}:{}` resolved to an invalid port",
                    declared.process, declared.endpoint
                ))
            })?
        } else {
            declared.port.ok_or_else(|| {
                CliError(format!(
                    "launch port `{}:{}` has no resolved port environment `{}`",
                    declared.process, declared.endpoint, declared.port_environment
                ))
            })?
        };
        ports.reserve(
            declared.transport,
            &host,
            port,
            &format!("{}:{}", declared.process, declared.endpoint),
        )?;
    }
    Ok(())
}

pub(crate) fn session_command(
    root: &Path,
    explicit_plan: Option<PathBuf>,
    operation: &str,
    arguments: SessionArgs,
) -> Result<Outcome> {
    validate_session_id(&arguments.session)?;
    let loaded = load(root, explicit_plan)?;
    let directory = loaded.state_root.join(&arguments.session);
    let record = read_session_at(&directory)?;
    if record.id != arguments.session {
        return Err(CliError(format!(
            "launch session metadata ID `{}` does not match requested session `{}`",
            record.id, arguments.session
        )));
    }
    let argv = match operation {
        "attach" => &record.commands.attach,
        "status" => &record.commands.status,
        "logs" => &record.commands.logs,
        "down" => &record.commands.down,
        _ => {
            return Err(CliError(format!(
                "unsupported launch session operation `{operation}`"
            )))
        }
    };
    let mut command = exact_command(argv, &record.working_directory)?;
    if matches!(operation, "attach" | "logs") {
        return replace_or_wait(command, argv);
    }
    let status = command
        .status()
        .map_err(|error| command_start_error(argv, error))?;
    if operation == "down" && status.success() {
        verify_session_ports_released(&record)?;
        fs::remove_dir_all(&directory).map_err(|error| {
            CliError(format!(
                "launch manager stopped session `{}` but its owned state at {} could not be removed: {error}",
                record.id,
                directory.display()
            ))
        })?;
    }
    status_outcome(status)
}

fn verify_session_ports_released(record: &SessionRecord) -> Result<()> {
    for claim in session_port_claims(record) {
        let host = claim.host.as_deref().unwrap_or("127.0.0.1");
        let (available, error) = port_available(claim.transport, host, claim.port);
        if !available {
            return Err(CliError(format!(
                "launch manager stopped session `{}` but {}:{} is still occupied: {}",
                record.id,
                host,
                claim.port,
                error.unwrap_or_else(|| "unknown port error".into())
            )));
        }
    }
    Ok(())
}

pub(crate) fn probe(arguments: ProbeArgs) -> Result<Outcome> {
    match arguments.probe {
        Probe::Tcp(network) => {
            connect(&network)?;
            Ok(Outcome::Success)
        }
        Probe::Http(arguments) => http_probe(arguments),
    }
}

fn http_probe(arguments: HttpProbe) -> Result<Outcome> {
    if !arguments.path.starts_with('/')
        || arguments.path.contains('\r')
        || arguments.path.contains('\n')
        || arguments.network.host.contains('\r')
        || arguments.network.host.contains('\n')
        || !(100..=599).contains(&arguments.expect_status)
    {
        return Err(CliError(
            "HTTP probe requires a slash-prefixed path without control characters, a safe host, and an expected status from 100 through 599"
                .into(),
        ));
    }
    let mut stream = connect(&arguments.network)?;
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n\r\n",
        arguments.path, arguments.network.host, arguments.network.port
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| CliError(format!("HTTP readiness request failed: {error}")))?;
    let mut status_line = String::new();
    BufReader::new(stream)
        .read_line(&mut status_line)
        .map_err(|error| CliError(format!("HTTP readiness response failed: {error}")))?;
    if status_line.len() > 4096 {
        return Err(CliError("HTTP readiness status line is too large".into()));
    }
    let mut fields = status_line.split_whitespace();
    let protocol = fields.next();
    let status = fields.next().and_then(|value| value.parse::<u16>().ok());
    if !protocol.is_some_and(|value| value.starts_with("HTTP/1."))
        || status != Some(arguments.expect_status)
    {
        return Err(CliError(format!(
            "HTTP readiness expected status {}, received `{}`",
            arguments.expect_status,
            status_line.trim_end()
        )));
    }
    Ok(Outcome::Success)
}

fn connect(arguments: &NetworkProbe) -> Result<TcpStream> {
    if arguments.host.is_empty() || arguments.host.contains('\0') || arguments.timeout_ms == 0 {
        return Err(CliError(
            "TCP probe requires a nonempty host and positive timeout".into(),
        ));
    }
    let timeout = Duration::from_millis(arguments.timeout_ms);
    let addresses = (arguments.host.as_str(), arguments.port)
        .to_socket_addrs()
        .map_err(|error| CliError(format!("cannot resolve readiness endpoint: {error}")))?;
    let mut last_error = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(stream) => {
                stream.set_read_timeout(Some(timeout)).map_err(|error| {
                    CliError(format!("cannot configure readiness read timeout: {error}"))
                })?;
                stream.set_write_timeout(Some(timeout)).map_err(|error| {
                    CliError(format!("cannot configure readiness write timeout: {error}"))
                })?;
                return Ok(stream);
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(CliError(format!(
        "readiness endpoint {}:{} is unavailable: {}",
        arguments.host,
        arguments.port,
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "host resolved to no addresses".into())
    )))
}

fn load(root: &Path, explicit_plan: Option<PathBuf>) -> Result<LoadedPlan> {
    let path = explicit_plan
        .or_else(|| env::var_os("NIXSPACE_LAUNCH_PLAN").map(PathBuf::from))
        .ok_or_else(|| {
            CliError(
                "launch execution requires an explicit Nix-generated plan; pass --launch-plan or set NIXSPACE_LAUNCH_PLAN"
                    .into(),
            )
        })?;
    let path = if path.is_absolute() {
        path
    } else {
        root.join(path)
    };
    let bytes = fs::read(&path).map_err(|error| {
        CliError(format!(
            "launch execution plan is unavailable at {}: {error}",
            path.display()
        ))
    })?;
    let plan: LaunchExecutionPlan = serde_json::from_slice(&bytes).map_err(|error| {
        CliError(format!(
            "Nix-generated launch execution plan at {} is unreadable: {error}",
            path.display()
        ))
    })?;
    validate_plan(&plan)?;
    let state_root = safe_workspace_path(root, &plan.state_root, "launch stateRoot")?;
    Ok(LoadedPlan { plan, state_root })
}

fn validate_plan(plan: &LaunchExecutionPlan) -> Result<()> {
    if plan.api_version != SUPPORTED_API_VERSION {
        return Err(CliError(format!(
            "launch execution API `{}` is unsupported; this nixspace requires `{SUPPORTED_API_VERSION}`",
            plan.api_version
        )));
    }
    if plan.kind != SUPPORTED_KIND {
        return Err(CliError(format!(
            "launch execution kind `{}` is unsupported; this nixspace requires `{SUPPORTED_KIND}`",
            plan.kind
        )));
    }
    if plan.interface_version != SUPPORTED_INTERFACE_VERSION {
        return Err(CliError(format!(
            "launch execution interface version {} is unsupported; this nixspace supports version {SUPPORTED_INTERFACE_VERSION}",
            plan.interface_version
        )));
    }
    if !portable_relative(&plan.state_root) {
        return Err(CliError(
            "launch execution stateRoot must be a nonempty safe workspace-relative path".into(),
        ));
    }
    for (key, launch) in &plan.launches {
        if key != &launch.workspace_launch
            || launch.coordinate.is_empty()
            || launch.workspace_launch.is_empty()
        {
            return Err(CliError(format!(
                "launch execution key `{key}` must equal its nonempty workspaceLaunch and declare a nonempty schema coordinate"
            )));
        }
        if launch.runner.kind != "devenv-process-compose" {
            return Err(CliError(format!(
                "launch `{key}` runner kind `{}` is unsupported; expected `devenv-process-compose`",
                launch.runner.kind
            )));
        }
        let mut declared_ports = BTreeSet::new();
        for declared in &launch.declared_ports {
            if declared.launch.is_empty()
                || declared.process.is_empty()
                || declared.endpoint.is_empty()
                || declared.protocol.is_empty()
                || declared.host.as_deref().is_some_and(str::is_empty)
                || !valid_environment_name(&declared.port_environment)
                || declared
                    .host_environment
                    .as_deref()
                    .is_some_and(|name| !valid_environment_name(name))
                || !declared_ports.insert((
                    declared.launch.as_str(),
                    declared.process.as_str(),
                    declared.endpoint.as_str(),
                ))
            {
                return Err(CliError(format!(
                    "launch `{key}` declared ports must have unique nonempty coordinates and valid host/port bindings"
                )));
            }
        }
        if launch.runner.working_directory.as_os_str().is_empty()
            || launch
                .runner
                .working_directory
                .to_string_lossy()
                .contains('\0')
        {
            return Err(CliError(format!(
                "launch `{key}` runner workingDirectory must be nonempty and contain no NUL"
            )));
        }
        if !launch.runner.working_directory.is_absolute()
            && launch.runner.working_directory != Path::new(".")
            && !portable_relative(&launch.runner.working_directory)
        {
            return Err(CliError(format!(
                "launch `{key}` runner workingDirectory must be absolute or safely workspace-relative"
            )));
        }
        for (name, command) in manager_commands(&launch.runner.commands) {
            if command.argv.is_empty() {
                return Err(CliError(format!(
                    "launch `{key}` manager command `{name}` must declare argv"
                )));
            }
            for argument in &command.argv {
                if matches!(argument, ManagerArgument::Literal(value) if value.contains('\0')) {
                    return Err(CliError(format!(
                        "launch `{key}` manager command `{name}` argv contains NUL"
                    )));
                }
            }
        }
        let mut identities = BTreeSet::new();
        for (environment, parameter) in &launch.parameters {
            if environment != &parameter.environment
                || !valid_environment_name(environment)
                || parameter.coordinate.is_empty()
                || parameter.workspace_coordinate.is_empty()
                || parameter.parameter_id.is_empty()
                || parameter.secret != (parameter.parameter_type == ParameterType::Secret)
                || (parameter.secret && parameter.default.is_some())
                || (parameter.allocation == PortAllocation::Automatic
                    && (parameter.parameter_type != ParameterType::Port
                        || parameter.allocation_host.is_empty()))
                || (parameter.parameter_type != ParameterType::Port
                    && parameter.allocation != PortAllocation::Fixed)
            {
                return Err(CliError(format!(
                    "launch `{key}` parameter environment `{environment}` has an invalid execution contract"
                )));
            }
            if !identities.insert((&parameter.workspace_coordinate, &parameter.parameter_id)) {
                return Err(CliError(format!(
                    "launch `{key}` repeats workspace parameter `{}:{}`",
                    parameter.workspace_coordinate, parameter.parameter_id
                )));
            }
            let _ = (
                &parameter.enum_values,
                parameter.minimum,
                parameter.maximum,
                &parameter.allowed_roots,
                parameter.must_exist,
                &parameter.access,
                parameter.required,
                parameter.allocation_transport,
            );
        }
        for (environment, binding) in &launch.session_environment {
            if !valid_environment_name(environment)
                || environment == SESSION_ENVIRONMENT
                || launch.parameters.contains_key(environment)
                || !portable_relative(&binding.path)
            {
                return Err(CliError(format!(
                    "launch `{key}` session environment `{environment}` has an invalid or colliding execution contract"
                )));
            }
            let _ = (binding.base, binding.create);
        }
    }
    Ok(())
}

fn session_environment(
    launch: &ExecutionLaunch,
    session: &Path,
    environment: &mut BTreeMap<String, String>,
) -> Result<()> {
    for (name, binding) in &launch.session_environment {
        let path = session.join(&binding.path);
        match binding.create {
            SessionCreate::None => {}
            SessionCreate::Parent => {
                let parent = path
                    .parent()
                    .expect("safe relative session path has a parent");
                fs::create_dir_all(parent).map_err(|error| {
                    CliError(format!(
                        "cannot create parent for session environment `{name}` at {}: {error}",
                        parent.display()
                    ))
                })?;
            }
            SessionCreate::Directory => fs::create_dir_all(&path).map_err(|error| {
                CliError(format!(
                    "cannot create directory for session environment `{name}` at {}: {error}",
                    path.display()
                ))
            })?,
        }
        environment.insert(name.clone(), path.to_string_lossy().into_owned());
    }
    Ok(())
}

fn parameter_environment(
    root: &Path,
    launch: &ExecutionLaunch,
    resolved: &ResolvedLaunchPlan,
) -> Result<BTreeMap<String, String>> {
    let mut environment = BTreeMap::new();
    for (name, contract) in &launch.parameters {
        let parameter = resolved
            .parameter(&contract.workspace_coordinate, &contract.parameter_id)
            .map_err(CliError)?;
        if parameter.parameter_type != contract.parameter_type
            || parameter.allocation != contract.allocation
        {
            return Err(CliError(format!(
                "launch execution parameter `{name}` type or allocation does not match the workspace launch plan"
            )));
        }
        if contract.secret {
            match env::var(name) {
                Ok(value) => {
                    if value.contains('\0') {
                        return Err(CliError(format!(
                            "secret provider returned an invalid value for `{name}`"
                        )));
                    }
                    environment.insert(name.clone(), value);
                }
                Err(env::VarError::NotPresent) if !contract.required => {}
                Err(env::VarError::NotPresent) => {
                    return Err(CliError(format!(
                        "launch requires secret environment `{name}`; configure a runtime secret provider"
                    )))
                }
                Err(env::VarError::NotUnicode(_)) => {
                    return Err(CliError(format!(
                        "secret environment `{name}` is not valid Unicode"
                    )))
                }
            }
            continue;
        }
        if parameter.value.is_null() {
            continue;
        }
        let value = scalar_string(&parameter.value)
            .map_err(|error| CliError(format!("launch execution parameter `{name}` {error}")))?;
        if contract.parameter_type == ParameterType::Path && contract.must_exist {
            let path = PathBuf::from(&value);
            let path = if path.is_absolute() {
                path
            } else {
                root.join(path)
            };
            if !path.exists() {
                return Err(CliError(format!(
                    "launch path parameter `{name}` must exist at {}",
                    path.display()
                )));
            }
        }
        environment.insert(name.clone(), value);
    }
    Ok(environment)
}

fn scalar_string(value: &Value) -> std::result::Result<String, &'static str> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Number(value) => Ok(value.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        _ => Err("is not a scalar"),
    }
}

fn create_session(
    state_root: &Path,
    coordinate: &str,
    requested: Option<String>,
) -> Result<PathBuf> {
    fs::create_dir_all(state_root).map_err(|error| {
        CliError(format!(
            "cannot create Nix-selected launch state root at {}: {error}",
            state_root.display()
        ))
    })?;
    if let Some(id) = requested {
        validate_session_id(&id)?;
        let path = state_root.join(&id);
        fs::create_dir(&path).map_err(|error| {
            if error.kind() == io::ErrorKind::AlreadyExists {
                CliError(format!("launch session `{id}` already exists"))
            } else {
                CliError(format!(
                    "cannot create launch session `{id}` at {}: {error}",
                    path.display()
                ))
            }
        })?;
        return Ok(path);
    }
    let stem: String = coordinate
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '-'
            }
        })
        .collect();
    for _ in 0..1000 {
        let sequence = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
        let id = format!("{stem}-{}-{sequence}", std::process::id());
        let path = state_root.join(&id);
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(CliError(format!(
                    "cannot create launch session at {}: {error}",
                    path.display()
                )))
            }
        }
    }
    Err(CliError(
        "cannot allocate a collision-free launch session identifier".into(),
    ))
}

fn validate_session_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || id == "."
        || id == ".."
        || !id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
        || !id
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphanumeric())
    {
        return Err(CliError(
            "launch session ID must start with an ASCII letter or digit and contain only letters, digits, `.`, `_`, or `-`"
                .into(),
        ));
    }
    Ok(())
}

fn read_session_at(directory: &Path) -> Result<SessionRecord> {
    let path = directory.join(METADATA_FILE);
    let bytes = fs::read(&path).map_err(|error| {
        CliError(format!(
            "launch session metadata is unavailable at {}: {error}",
            path.display()
        ))
    })?;
    let record: SessionRecord = serde_json::from_slice(&bytes).map_err(|error| {
        CliError(format!(
            "launch session metadata at {} is unreadable: {error}",
            path.display()
        ))
    })?;
    if record.api_version != SUPPORTED_API_VERSION
        || record.kind != SESSION_KIND
        || record.interface_version != SUPPORTED_INTERFACE_VERSION
    {
        return Err(CliError(format!(
            "launch session metadata at {} uses an unsupported interface",
            path.display()
        )));
    }
    validate_session_id(&record.id)?;
    Ok(record)
}

fn resolve_command(command: &ManagerCommand, socket: &Path, log: &Path) -> Result<Vec<String>> {
    command
        .argv
        .iter()
        .map(|argument| match argument {
            ManagerArgument::Literal(value) => Ok(value.clone()),
            ManagerArgument::Runtime(RuntimeArgument {
                runtime: RuntimeValue::SessionSocket,
            }) => Ok(socket.to_string_lossy().into_owned()),
            ManagerArgument::Runtime(RuntimeArgument {
                runtime: RuntimeValue::SessionLog,
            }) => Ok(log.to_string_lossy().into_owned()),
        })
        .collect()
}

fn manager_commands(commands: &LaunchCommands) -> [(&'static str, &ManagerCommand); 6] {
    [
        ("up", &commands.up),
        ("start", &commands.start),
        ("attach", &commands.attach),
        ("status", &commands.status),
        ("logs", &commands.logs),
        ("down", &commands.down),
    ]
}

fn exact_command(argv: &[String], working_directory: &Path) -> Result<Command> {
    let (program, arguments) = argv
        .split_first()
        .ok_or_else(|| CliError("Nix-generated command must begin with an executable".into()))?;
    if program.is_empty() || argv.iter().any(|argument| argument.contains('\0')) {
        return Err(CliError(
            "Nix-generated command must begin with a nonempty executable and contain no NUL".into(),
        ));
    }
    let mut command = Command::new(program);
    command.args(arguments).current_dir(working_directory);
    Ok(command)
}

fn replace_or_wait(mut command: Command, argv: &[String]) -> Result<Outcome> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = command.exec();
        Err(command_start_error(argv, error))
    }
    #[cfg(not(unix))]
    {
        let status = command
            .status()
            .map_err(|error| command_start_error(argv, error))?;
        status_outcome(status)
    }
}

fn command_start_error(argv: &[String], error: io::Error) -> CliError {
    CliError(format!(
        "cannot start Nix-generated executable `{}`: {error}",
        argv.first().map(String::as_str).unwrap_or("<missing>")
    ))
}

fn status_outcome(status: ExitStatus) -> Result<Outcome> {
    if status.success() {
        Ok(Outcome::Success)
    } else {
        Ok(Outcome::Exit(status.code().unwrap_or(1)))
    }
}

fn resolve_working_directory(root: &Path, path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_owned())
    } else if path == Path::new(".") {
        Ok(root.to_owned())
    } else {
        safe_workspace_path(root, path, "launch workingDirectory")
    }
}

fn safe_workspace_path(root: &Path, path: &Path, description: &str) -> Result<PathBuf> {
    if !portable_relative(path) {
        return Err(CliError(format!(
            "{description} must be a nonempty safe workspace-relative path"
        )));
    }
    Ok(root.join(path))
}

fn portable_relative(path: &Path) -> bool {
    let rendered = path.to_string_lossy();
    !rendered.is_empty()
        && !rendered.contains('\0')
        && !rendered.contains('\\')
        && !rendered
            .split('/')
            .next()
            .is_some_and(|segment| segment.ends_with(':'))
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn valid_environment_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn atomic_json<T: Serialize>(destination: &Path, value: &T) -> Result<()> {
    let mut bytes = serde_json::to_vec(value)
        .map_err(|error| CliError(format!("cannot encode launch session metadata: {error}")))?;
    bytes.push(b'\n');
    let sequence = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
    let temporary = destination.with_extension(format!("tmp-{}-{sequence}", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> Result<()> {
        let mut file = options.open(&temporary).map_err(|error| {
            CliError(format!(
                "cannot create temporary launch metadata at {}: {error}",
                temporary.display()
            ))
        })?;
        file.write_all(&bytes).map_err(|error| {
            CliError(format!(
                "cannot write temporary launch metadata at {}: {error}",
                temporary.display()
            ))
        })?;
        file.sync_all().map_err(|error| {
            CliError(format!(
                "cannot sync temporary launch metadata at {}: {error}",
                temporary.display()
            ))
        })?;
        fs::rename(&temporary, destination).map_err(|error| {
            CliError(format!(
                "cannot publish launch metadata at {}: {error}",
                destination.display()
            ))
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
