use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use clap::Subcommand;
use serde_json::Value;

use crate::{decode_index, CliError, Result};

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Subcommand)]
pub(crate) enum IndexCommand {
    /// Build and atomically cache the Nix-generated workspace index.
    Refresh {
        /// Flake reference that provides the index installable.
        #[arg(long, value_name = "REF")]
        flake: Option<String>,

        /// Flake output attribute built to obtain the workspace index.
        #[arg(long, value_name = "ATTRIBUTE")]
        index_installable: Option<String>,

        /// Index path relative to the built output root.
        #[arg(long, value_name = "PATH")]
        index_file: Option<PathBuf>,
    },
    /// Report whether the cached workspace index exists.
    Status,
    /// Print the cached workspace index path.
    Path,
}

pub(crate) fn run(root: &Path, path: &Path, command: IndexCommand) -> Result<()> {
    match command {
        IndexCommand::Refresh {
            flake,
            index_installable,
            index_file,
        } => refresh(
            root,
            path,
            &refresh_configuration(flake, index_installable, index_file)?,
        ),
        IndexCommand::Status if path.is_file() => {
            println!("ready: {}", path.display());
            Ok(())
        }
        IndexCommand::Status => {
            println!("missing: {}", path.display());
            Err(CliError("cached workspace index is missing".into()))
        }
        IndexCommand::Path => {
            println!("{}", path.display());
            Ok(())
        }
    }
}

struct RefreshConfiguration {
    flake: String,
    installable: String,
    index_file: PathBuf,
}

fn refresh_configuration(
    flake: Option<String>,
    installable: Option<String>,
    index_file: Option<PathBuf>,
) -> Result<RefreshConfiguration> {
    let flake = flake
        .or_else(|| env::var("NIXSPACE_FLAKE").ok())
        .ok_or_else(|| {
            CliError(
                "index refresh requires an explicit flake reference; pass --flake or set NIXSPACE_FLAKE"
                    .into(),
            )
        })?;
    if flake.trim() != flake || flake.is_empty() || flake.contains('#') {
        return Err(CliError(
            "flake reference must be nonempty, have no surrounding whitespace, and omit `#`".into(),
        ));
    }
    let installable = installable
        .or_else(|| env::var("NIXSPACE_INDEX_INSTALLABLE").ok())
        .ok_or_else(|| {
            CliError(
                "index refresh requires an explicit output attribute; pass --index-installable or set NIXSPACE_INDEX_INSTALLABLE"
                    .into(),
            )
        })?;
    if installable.trim() != installable || installable.is_empty() || installable.contains('#') {
        return Err(CliError(
            "index installable must be a nonempty flake output attribute without `#`".into(),
        ));
    }
    let index_file = index_file
        .or_else(|| env::var_os("NIXSPACE_INDEX_FILE").map(PathBuf::from))
        .ok_or_else(|| {
            CliError(
                "index refresh requires an explicit output-relative index file; pass --index-file or set NIXSPACE_INDEX_FILE"
                    .into(),
            )
        })?;
    if !safe_relative(&index_file) {
        return Err(CliError(
            "index file must be a nonempty relative path without parent traversal".into(),
        ));
    }
    Ok(RefreshConfiguration {
        flake,
        installable,
        index_file,
    })
}

fn refresh(root: &Path, destination: &Path, configuration: &RefreshConfiguration) -> Result<()> {
    let flake = prepared_flake_reference(root, &configuration.flake)?;
    let installable = format!("{}#{}", flake.reference, configuration.installable);

    let output = Command::new("nix")
        .current_dir(root)
        .args(["build", "--no-link", "--print-out-paths", &installable])
        .output()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                CliError("index refresh requires Nix on PATH".into())
            } else {
                CliError(format!("cannot start Nix for index refresh: {error}"))
            }
        })?;
    if !output.status.success() {
        return Err(CliError(with_command_stderr(
            "workspace index build failed; existing cache was not changed",
            &output,
        )));
    }

    let stdout = String::from_utf8(output.stdout).map_err(|error| {
        CliError(format!(
            "workspace index build returned non-UTF-8 output: {error}; existing cache was not changed"
        ))
    })?;
    let output_paths: Vec<_> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if output_paths.len() != 1 {
        return Err(CliError(format!(
            "workspace index build returned {} output paths; expected exactly one; existing cache was not changed",
            output_paths.len()
        )));
    }

    let generated = Path::new(output_paths[0]).join(&configuration.index_file);
    let bytes = fs::read(&generated).map_err(|error| {
        CliError(format!(
            "workspace index output is missing or unreadable at {}: {error}; existing cache was not changed",
            generated.display()
        ))
    })?;
    decode_index(&bytes, &generated.display().to_string()).map_err(|error| {
        CliError(format!(
            "workspace index validation failed; existing cache was not changed: {error}"
        ))
    })?;

    atomic_replace(destination, &bytes)?;
    println!("refreshed: {}", destination.display());
    Ok(())
}

struct PreparedFlake {
    reference: String,
    _snapshot: Option<TemporaryDirectory>,
}

fn prepared_flake_reference(workspace_root: &Path, flake_reference: &str) -> Result<PreparedFlake> {
    let Some(root) = local_flake_root(workspace_root, flake_reference) else {
        return Ok(PreparedFlake {
            reference: flake_reference.to_owned(),
            _snapshot: None,
        });
    };
    let root = fs::canonicalize(&root).map_err(|error| {
        CliError(format!(
            "cannot resolve local flake root {}: {error}; existing cache was not changed",
            root.display()
        ))
    })?;
    let worktree = git_output(&root, ["rev-parse", "--show-toplevel"])?;
    ensure_status_succeeded(&worktree, &root)?;
    let worktree = output_line(&worktree.stdout, "Git worktree root")?;
    let worktree = fs::canonicalize(worktree).map_err(|error| {
        CliError(format!(
            "cannot resolve Git worktree root for local flake {}: {error}",
            root.display()
        ))
    })?;
    let flake_directory = root.strip_prefix(&worktree).map_err(|_| {
        CliError(format!(
            "refusing index refresh: local flake {} is outside its reported Git worktree {}; existing cache was not changed",
            root.display(),
            worktree.display()
        ))
    })?;
    // Capture first, then validate only bytes and tree entries from that exact
    // immutable commit. The private repository and its advertised ref remain
    // alive through `nix build`; no user index, worktree, or ref is changed.
    let snapshot = snapshot_repository(&worktree)?;
    let tracked_flake = flake_directory.join("flake.nix");
    let tracked_lock = flake_directory.join("flake.lock");
    snapshot_regular_file(&snapshot, &tracked_flake, "flake.nix")?;
    let lock_bytes = snapshot_regular_file(&snapshot, &tracked_lock, "flake.lock")?;
    for locked_path in local_path_inputs(&lock_bytes, &tracked_lock)? {
        let rendered = locked_path.to_string_lossy();
        if locked_path.is_absolute()
            || rendered
                .split('/')
                .next()
                .is_some_and(|segment| segment.ends_with(':'))
        {
            return Err(CliError(format!(
                "refusing index refresh: pinned local path input {} is absolute and cannot be bound to the immutable snapshot; use a relative path inside the Git worktree",
                locked_path.display()
            )));
        }
        let local_input = lexical_normalize(&root.join(&locked_path)).ok_or_else(|| {
            CliError(format!(
                "refusing index refresh: local path input {} escapes the immutable Git worktree",
                locked_path.display()
            ))
        })?;
        let relative = local_input.strip_prefix(&worktree).map_err(|_| {
            CliError(format!(
                "refusing index refresh: local path input {} is outside the immutable Git worktree {}; use a locked Git flake input instead; existing cache was not changed",
                locked_path.display(), worktree.display()
            ))
        })?;
        snapshot_tree(&snapshot, relative, "local path input")?;
    }

    let mut reference = format!(
        "git+{}?rev={}",
        file_url(&snapshot.repository)?,
        snapshot.commit
    );
    if !flake_directory.as_os_str().is_empty() {
        let directory = flake_directory.to_str().ok_or_else(|| {
            CliError(
                "local flake directory is not valid UTF-8 and cannot be encoded as a Git flake URL"
                    .into(),
            )
        })?;
        reference.push_str("&dir=");
        reference.push_str(&percent_encode(
            directory.replace('\\', "/").as_bytes(),
            true,
        ));
    }
    Ok(PreparedFlake {
        reference,
        _snapshot: Some(snapshot.temporary),
    })
}

struct SnapshotRepository {
    temporary: TemporaryDirectory,
    repository: PathBuf,
    commit: String,
}

fn snapshot_repository(worktree: &Path) -> Result<SnapshotRepository> {
    let temporary = create_snapshot_directory()?;
    let guard = TemporaryDirectory(temporary.clone());
    let repository = temporary.join("snapshot.git");
    let index = temporary.join("index");

    let clone = Command::new("git")
        .current_dir(worktree)
        .args(["clone", "--quiet", "--shared", "--bare", "--"])
        .arg(worktree)
        .arg(&repository)
        .output()
        .map_err(|error| git_start_error(worktree, error))?;
    ensure_status_succeeded(&clone, worktree)?;

    let index_path = git_output(worktree, ["rev-parse", "--git-path", "index"])?;
    ensure_status_succeeded(&index_path, worktree)?;
    let index_path = PathBuf::from(output_line(&index_path.stdout, "Git index path")?);
    let index_path = if index_path.is_absolute() {
        index_path
    } else {
        worktree.join(index_path)
    };
    fs::copy(&index_path, &index).map_err(|error| {
        CliError(format!(
            "cannot snapshot Git index {} without modifying it: {error}; existing cache was not changed",
            index_path.display()
        ))
    })?;

    let add = git_snapshot_output(&repository, worktree, &index, ["add", "-u", "--", "."])?;
    ensure_status_succeeded(&add, worktree)?;
    let tree = git_snapshot_output(&repository, worktree, &index, ["write-tree"])?;
    ensure_status_succeeded(&tree, worktree)?;
    let tree = output_object_id(&tree.stdout, "Git snapshot tree")?;

    let commit = Command::new("git")
        .current_dir(worktree)
        .env("GIT_DIR", &repository)
        .env("GIT_WORK_TREE", worktree)
        .env("GIT_INDEX_FILE", &index)
        .env("GIT_AUTHOR_NAME", "nixspace")
        .env("GIT_AUTHOR_EMAIL", "nixspace@invalid")
        .env("GIT_AUTHOR_DATE", "1970-01-01T00:00:00Z")
        .env("GIT_COMMITTER_NAME", "nixspace")
        .env("GIT_COMMITTER_EMAIL", "nixspace@invalid")
        .env("GIT_COMMITTER_DATE", "1970-01-01T00:00:00Z")
        .args(["commit-tree", tree, "-m", "nixspace index snapshot"])
        .output()
        .map_err(|error| git_start_error(worktree, error))?;
    ensure_status_succeeded(&commit, worktree)?;
    let commit = output_object_id(&commit.stdout, "Git snapshot commit")?.to_owned();
    let update_ref = bare_git_output(
        &repository,
        ["update-ref", "refs/heads/nixspace-index-snapshot", &commit],
    )?;
    ensure_status_succeeded(&update_ref, &repository)?;
    Ok(SnapshotRepository {
        temporary: guard,
        repository,
        commit,
    })
}

fn git_snapshot_output<I, S>(
    repository: &Path,
    worktree: &Path,
    index: &Path,
    arguments: I,
) -> Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new("git")
        .current_dir(worktree)
        .env("GIT_DIR", repository)
        .env("GIT_WORK_TREE", worktree)
        .env("GIT_INDEX_FILE", index)
        .args(arguments)
        .output()
        .map_err(|error| git_start_error(worktree, error))
}

fn bare_git_output<I, S>(repository: &Path, arguments: I) -> Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new("git")
        .env("GIT_DIR", repository)
        .args(arguments)
        .output()
        .map_err(|error| git_start_error(repository, error))
}

fn snapshot_regular_file(
    snapshot: &SnapshotRepository,
    path: &Path,
    description: &str,
) -> Result<Vec<u8>> {
    let mode = snapshot_entry(snapshot, path, "blob", description)?;
    if !mode.starts_with("100") {
        return Err(CliError(format!(
            "refusing index refresh: immutable snapshot {description} must be a regular tracked file"
        )));
    }
    snapshot_contents(snapshot, path, description)
}

fn snapshot_tree(snapshot: &SnapshotRepository, path: &Path, description: &str) -> Result<()> {
    snapshot_entry(snapshot, path, "tree", description).map(|_| ())
}

fn snapshot_entry(
    snapshot: &SnapshotRepository,
    path: &Path,
    expected_type: &str,
    description: &str,
) -> Result<String> {
    let path = git_path(path)?;
    let output = bare_git_output(
        &snapshot.repository,
        ["ls-tree", "-z", &snapshot.commit, "--", &path],
    )?;
    ensure_status_succeeded(&output, &snapshot.repository)?;
    let entries = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>();
    if entries.len() != 1 {
        return Err(CliError(format!(
            "refusing index refresh: immutable snapshot is missing exact tracked {description} `{path}`"
        )));
    }
    let entry = std::str::from_utf8(entries[0]).map_err(|error| {
        CliError(format!(
            "immutable snapshot tree entry is not UTF-8: {error}"
        ))
    })?;
    let (metadata, entry_path) = entry
        .split_once('\t')
        .ok_or_else(|| CliError("immutable snapshot returned a malformed tree entry".into()))?;
    let mut metadata = metadata.split_whitespace();
    let mode = metadata.next().unwrap_or_default();
    let object_type = metadata.next().unwrap_or_default();
    let object = metadata.next().unwrap_or_default();
    if metadata.next().is_some()
        || entry_path != path
        || object_type != expected_type
        || !valid_object_id(object)
    {
        return Err(CliError(format!(
            "refusing index refresh: immutable snapshot {description} `{path}` is not the expected tracked {expected_type}"
        )));
    }
    Ok(mode.to_owned())
}

fn snapshot_contents(
    snapshot: &SnapshotRepository,
    path: &Path,
    description: &str,
) -> Result<Vec<u8>> {
    let path = git_path(path)?;
    let object = format!("{}:{path}", snapshot.commit);
    let output = bare_git_output(&snapshot.repository, ["cat-file", "-p", &object])?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(CliError(with_command_stderr(
            &format!("cannot read {description} from immutable Git snapshot"),
            &output,
        )))
    }
}

fn git_path(path: &Path) -> Result<String> {
    let path = path
        .to_str()
        .ok_or_else(|| CliError("immutable Git snapshot path is not valid UTF-8".into()))?;
    if path.is_empty() {
        Ok(".".into())
    } else {
        Ok(path.replace('\\', "/"))
    }
}

fn git_start_error(directory: &Path, error: io::Error) -> CliError {
    if error.kind() == io::ErrorKind::NotFound {
        CliError("index refresh of local path flakes requires Git on PATH".into())
    } else {
        CliError(format!(
            "cannot inspect Git state in {}: {error}",
            directory.display()
        ))
    }
}

fn output_object_id<'a>(bytes: &'a [u8], description: &str) -> Result<&'a str> {
    let object = output_line(bytes, description)?;
    if !valid_object_id(object) {
        return Err(CliError(format!(
            "{description} returned invalid object ID `{object}`"
        )));
    }
    Ok(object)
}

fn valid_object_id(object: &str) -> bool {
    (40..=64).contains(&object.len()) && object.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn create_snapshot_directory() -> Result<PathBuf> {
    let parent = env::temp_dir();
    for _ in 0..128 {
        let sequence = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            "nixspace-index-snapshot.{}.{}",
            std::process::id(),
            sequence
        ));
        match fs::create_dir(&path) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).map_err(
                        |error| {
                            let _ = fs::remove_dir(&path);
                            CliError(format!(
                                "cannot secure temporary Git snapshot directory {}: {error}",
                                path.display()
                            ))
                        },
                    )?;
                }
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(CliError(format!(
                    "cannot create temporary Git snapshot directory in {}: {error}",
                    parent.display()
                )))
            }
        }
    }
    Err(CliError(format!(
        "cannot allocate a temporary Git snapshot directory in {}",
        parent.display()
    )))
}

struct TemporaryDirectory(PathBuf);

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn git_output<I, S>(directory: &Path, arguments: I) -> Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new("git")
        .current_dir(directory)
        .args(arguments)
        .output()
        .map_err(|error| git_start_error(directory, error))
}

fn ensure_status_succeeded(output: &Output, directory: &Path) -> Result<()> {
    if output.status.success() {
        Ok(())
    } else {
        Err(CliError(with_command_stderr(
            &format!("cannot inspect Git state in {}", directory.display()),
            output,
        )))
    }
}

fn output_line<'a>(bytes: &'a [u8], description: &str) -> Result<&'a str> {
    let value = std::str::from_utf8(bytes)
        .map_err(|error| CliError(format!("{description} is not valid UTF-8: {error}")))?
        .trim_end_matches(['\r', '\n']);
    if value.is_empty() || value.contains(['\r', '\n']) {
        return Err(CliError(format!(
            "{description} must contain exactly one nonempty line"
        )));
    }
    Ok(value)
}

fn file_url(path: &Path) -> Result<String> {
    let path = path.to_str().ok_or_else(|| {
        CliError("Git worktree path is not valid UTF-8 and cannot be encoded as a flake URL".into())
    })?;
    let normalized = path.replace('\\', "/");
    let encoded = percent_encode(normalized.as_bytes(), false);
    if encoded.starts_with('/') {
        Ok(format!("file://{encoded}"))
    } else {
        Ok(format!("file:///{encoded}"))
    }
}

fn percent_encode(bytes: &[u8], encode_slash: bool) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(bytes.len());
    for &byte in bytes {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~' | b':')
            || (!encode_slash && byte == b'/')
        {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    encoded
}

fn local_path_inputs(bytes: &[u8], lock: &Path) -> Result<Vec<PathBuf>> {
    let document: Value = serde_json::from_slice(bytes).map_err(|error| {
        CliError(format!(
            "cannot read pinned flake lock at {} as JSON: {error}",
            lock.display()
        ))
    })?;
    let nodes = document
        .get("nodes")
        .and_then(Value::as_object)
        .ok_or_else(|| CliError("pinned flake lock is missing its `nodes` object".into()))?;
    let mut paths = BTreeSet::new();
    for node in nodes.values() {
        let Some(locked) = node.get("locked") else {
            continue;
        };
        if locked.get("type").and_then(Value::as_str) != Some("path") {
            continue;
        }
        let path = locked
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| CliError("pinned path input is missing its `path` string".into()))?;
        if path.is_empty()
            || path.contains('\\')
            || path.chars().any(|character| character.is_control())
        {
            return Err(CliError(
                "pinned path input must be a nonempty portable relative path without alternate separators or control characters"
                    .into(),
            ));
        }
        let path = PathBuf::from(path);
        paths.insert(path);
    }
    Ok(paths.into_iter().collect())
}

fn lexical_normalize(path: &Path) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
        }
    }
    Some(normalized)
}

fn local_flake_root(workspace_root: &Path, reference: &str) -> Option<PathBuf> {
    let path = if reference == "." {
        PathBuf::from(".")
    } else if let Some(path) = reference.strip_prefix("path:") {
        PathBuf::from(path)
    } else if reference.starts_with("./")
        || reference.starts_with("../")
        || Path::new(reference).is_absolute()
    {
        PathBuf::from(reference)
    } else {
        return None;
    };
    Some(if path.is_absolute() {
        path
    } else {
        workspace_root.join(path)
    })
}

fn safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn atomic_replace(destination: &Path, bytes: &[u8]) -> Result<()> {
    let parent = destination.parent().ok_or_else(|| {
        CliError(format!(
            "cached index path has no parent directory: {}",
            destination.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        CliError(format!(
            "cannot create index state directory {}: {error}; existing cache was not changed",
            parent.display()
        ))
    })?;

    let (temporary, mut file) = create_temporary(parent)?;
    let mut guard = TemporaryFile(Some(temporary.clone()));
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            CliError(format!(
                "cannot write temporary index {}: {error}; existing cache was not changed",
                temporary.display()
            ))
        })?;
    set_public_read_permissions(&file).map_err(|error| {
        CliError(format!(
            "cannot set permissions on temporary index {}: {error}; existing cache was not changed",
            temporary.display()
        ))
    })?;
    drop(file);

    fs::rename(&temporary, destination).map_err(|error| {
        CliError(format!(
            "cannot atomically replace cached index {}: {error}; existing cache was not changed",
            destination.display()
        ))
    })?;
    guard.0 = None;
    Ok(())
}

fn create_temporary(parent: &Path) -> Result<(PathBuf, File)> {
    for _ in 0..128 {
        let sequence = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".nixspace-index.{}.{}.tmp",
            std::process::id(),
            sequence
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(CliError(format!(
                    "cannot create temporary index in {}: {error}; existing cache was not changed",
                    parent.display()
                )))
            }
        }
    }
    Err(CliError(format!(
        "cannot allocate a unique temporary index in {}; existing cache was not changed",
        parent.display()
    )))
}

#[cfg(unix)]
fn set_public_read_permissions(file: &File) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    file.set_permissions(fs::Permissions::from_mode(0o644))
}

#[cfg(not(unix))]
fn set_public_read_permissions(_file: &File) -> io::Result<()> {
    Ok(())
}

fn with_command_stderr(message: &str, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        message.into()
    } else {
        format!("{message}: {stderr}")
    }
}

struct TemporaryFile(Option<PathBuf>);

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_file(path);
        }
    }
}
