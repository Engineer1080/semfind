use anyhow::{Context, Result};
use std::{
    env, fs,
    io::{self, Write},
    num::NonZeroU32,
    path::{Path, PathBuf},
};

pub(super) fn daemon_idle_path() -> Option<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|home| home.join(".config"))
        })?;
    Some(base.join("semfind/daemon-idle-seconds"))
}

pub(super) fn read_daemon_idle(path: &Path) -> Result<Option<NonZeroU32>> {
    match fs::read_to_string(path) {
        Ok(value) => Ok(Some(value.trim().parse().with_context(|| {
            format!("read a positive timeout in seconds from {}", path.display())
        })?)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("read {}", path.display())),
    }
}

pub(super) fn save_daemon_idle(path: &Path, seconds: NonZeroU32) -> Result<()> {
    let directory = path.parent().context("missing configuration directory")?;
    fs::create_dir_all(directory).with_context(|| format!("create {}", directory.display()))?;
    let mut file = tempfile::NamedTempFile::new_in(directory)
        .with_context(|| format!("create a temporary file in {}", directory.display()))?;
    writeln!(file, "{seconds}").with_context(|| format!("write {}", path.display()))?;
    file.as_file()
        .sync_all()
        .with_context(|| format!("sync {}", path.display()))?;
    file.persist(path)
        .with_context(|| format!("save {}", path.display()))?;
    Ok(())
}

pub(super) fn reset_daemon_idle(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("remove {}", path.display())),
    }
}
