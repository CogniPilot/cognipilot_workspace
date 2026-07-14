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
    ensure_git_filtered_flake(root, &configuration.flake)?;

    let installable = format!("{}#{}", configuration.flake, configuration.installable);

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

fn ensure_git_filtered_flake(workspace_root: &Path, flake_reference: &str) -> Result<()> {
    let Some(root) = local_flake_root(workspace_root, flake_reference) else {
        return Ok(());
    };
    let flake = root.join("flake.nix");
    if !flake.is_file() {
        return Err(CliError(format!(
            "index refresh requires a local flake at {}",
            flake.display()
        )));
    }
    let lock = root.join("flake.lock");
    if !lock.is_file() {
        return Err(CliError(format!(
            "index refresh requires a pinned local flake lock at {}",
            lock.display()
        )));
    }

    let tracked = git_output(
        &root,
        [
            "ls-files",
            "--error-unmatch",
            "--",
            "flake.nix",
            "flake.lock",
        ],
    )?;
    if !tracked.status.success() {
        return Err(untracked_flake_error(None));
    }

    let root_status = git_output(
        &root,
        [
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignored=matching",
            "--",
            "flake.nix",
            "flake.lock",
        ],
    )?;
    ensure_status_succeeded(&root_status, &root)?;
    let unsafe_paths = untracked_or_ignored(&root_status.stdout)?;
    if !unsafe_paths.is_empty() {
        return Err(untracked_flake_error(Some(&unsafe_paths)));
    }

    for local_input in local_path_inputs(&root, &lock)? {
        if !local_input.exists() {
            return Err(CliError(format!(
                "refusing index refresh: local path input does not exist: {}; existing cache was not changed",
                local_input.display()
            )));
        }
        let worktree = git_output(&local_input, ["rev-parse", "--show-toplevel"])?;
        if !worktree.status.success() {
            return Err(CliError(format!(
                "refusing index refresh: local path input is not in a Git worktree: {}; existing cache was not changed",
                local_input.display()
            )));
        }
        let status = git_output(
            &local_input,
            [
                "status",
                "--porcelain=v1",
                "--untracked-files=all",
                "--ignored=matching",
                "--",
                ".",
            ],
        )?;
        ensure_status_succeeded(&status, &local_input)?;
        let unsafe_paths = untracked_or_ignored(&status.stdout)?;
        if !unsafe_paths.is_empty() {
            return Err(untracked_flake_error(Some(&unsafe_paths)));
        }
    }
    Ok(())
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
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                CliError("index refresh of local path flakes requires Git on PATH".into())
            } else {
                CliError(format!(
                    "cannot inspect Git state in {}: {error}",
                    directory.display()
                ))
            }
        })
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

fn untracked_or_ignored(stdout: &[u8]) -> Result<Vec<String>> {
    let status = std::str::from_utf8(stdout)
        .map_err(|error| CliError(format!("Git returned non-UTF-8 status output: {error}")))?;
    Ok(status
        .lines()
        .filter(|line| line.starts_with("?? ") || line.starts_with("!! "))
        .map(str::to_owned)
        .collect())
}

fn untracked_flake_error(paths: Option<&[String]>) -> CliError {
    let detail = paths
        .filter(|values| !values.is_empty())
        .map(|values| format!("; unsafe paths: {}", values.join(", ")))
        .unwrap_or_default();
    CliError(format!(
        "refusing index refresh: the local flake or one of its local path inputs is untracked/ignored and could be ingested as an unfiltered Nix path{detail}; track the flake, lock, and every local path input before refreshing"
    ))
}

fn local_path_inputs(root: &Path, lock: &Path) -> Result<Vec<PathBuf>> {
    let bytes = fs::read(lock).map_err(|error| {
        CliError(format!(
            "cannot read pinned flake lock at {}: {error}",
            lock.display()
        ))
    })?;
    let document: Value = serde_json::from_slice(&bytes).map_err(|error| {
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
        let path = PathBuf::from(path);
        paths.insert(if path.is_absolute() {
            path
        } else {
            root.join(path)
        });
    }
    Ok(paths.into_iter().collect())
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
