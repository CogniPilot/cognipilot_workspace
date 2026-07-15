use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{decode_index, CliError, Result};
use clap::Subcommand;

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Subcommand)]
pub(crate) enum IndexCommand {
    /// Build and atomically cache the Nix-generated workspace index.
    Refresh {
        /// Complete opaque Nix installable that produces the workspace index.
        #[arg(long, value_name = "INSTALLABLE")]
        installable: Option<String>,

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
            installable,
            index_file,
        } => refresh(root, path, &refresh_configuration(installable, index_file)?),
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
    installable: String,
    index_file: PathBuf,
}

fn refresh_configuration(
    installable: Option<String>,
    index_file: Option<PathBuf>,
) -> Result<RefreshConfiguration> {
    let installable = installable
        .or_else(|| env::var("NIXSPACE_INDEX_INSTALLABLE").ok())
        .ok_or_else(|| {
            CliError(
                "index refresh requires an explicit complete Nix installable; pass --installable or set NIXSPACE_INDEX_INSTALLABLE"
                    .into(),
            )
        })?;
    if installable.is_empty() {
        return Err(CliError("index installable must be nonempty".into()));
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
        installable,
        index_file,
    })
}

fn refresh(root: &Path, destination: &Path, configuration: &RefreshConfiguration) -> Result<()> {
    let output = Command::new("nix")
        .current_dir(root)
        .args(["build", "--no-link", "--print-out-paths", "--"])
        .arg(&configuration.installable)
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
