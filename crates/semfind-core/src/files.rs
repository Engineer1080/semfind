use anyhow::{Context, Result, bail, ensure};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    time::Duration,
};

#[cfg(target_os = "linux")]
use rustix::fs::{Mode, OFlags, ResolveFlags, openat2};

const MAX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TEXT: u64 = 512 * 1024;
const CHUNK_SIZE: usize = 640;
const OVERLAP: usize = 80;

/// A group of supported file extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Text,
    Image,
    Video,
}

impl FileKind {
    pub(crate) fn from_extension(extension: &str) -> Option<Self> {
        match extension {
            "txt" | "md" | "markdown" | "log" | "csv" | "rst" | "pdf" => Some(Self::Text),
            "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "tif" | "tiff" | "avif" | "heic"
            | "heif" | "svg" => Some(Self::Image),
            "mp4" | "m4v" | "mov" | "mkv" | "webm" | "avi" | "mpg" | "mpeg" | "wmv" | "3gp" => {
                Some(Self::Video)
            }
            _ => None,
        }
    }
}

pub(crate) fn root(path: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    // Resolve "." without accepting ".." or link components.
    let path: PathBuf = absolute.components().collect();
    ensure!(
        path.parent().is_some() && allowed(&path),
        "excluded or unsupported root: {}",
        path.display()
    );
    #[cfg(target_os = "linux")]
    {
        let dir = directory(&path).context("open root without following symlinks")?;
        let magic = rustix::fs::fstatfs(&dir)?.f_type as u64;
        ensure!(
            ![
                0x9fa0,
                0x6265_6572,
                0x1cd1,
                0x0027_e0eb,
                0x6367_7270,
                0x6462_6720,
                0x7472_6163,
                0x7363_6673,
                0x1980_0202,
                0x6265_6570
            ]
            .contains(&magic),
            "root is a virtual system filesystem"
        );
    }
    #[cfg(not(target_os = "linux"))]
    {
        ensure!(
            no_links(&path)? && path.is_dir(),
            "root must be a real directory"
        );
    }
    Ok(path)
}

pub(crate) fn allowed(path: &Path) -> bool {
    if ["/proc", "/sys", "/dev", "/run"]
        .iter()
        .any(|p| path.starts_with(p))
    {
        return false;
    }
    path.components().all(|part| match part {
        Component::Normal(name) => name.to_str().is_some_and(|name| !name.starts_with('.')),
        Component::ParentDir => false,
        _ => true,
    })
}

#[cfg(target_os = "linux")]
fn directory(path: &Path) -> Result<rustix::fd::OwnedFd> {
    Ok(openat2(
        rustix::fs::CWD,
        path,
        OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
        ResolveFlags::NO_SYMLINKS,
    )?)
}

#[cfg(not(target_os = "linux"))]
fn no_links(path: &Path) -> Result<bool> {
    for ancestor in path.ancestors().filter(|p| !p.as_os_str().is_empty()) {
        let metadata = fs::symlink_metadata(ancestor)?;
        if metadata.file_type().is_symlink() {
            return Ok(false);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

pub(crate) fn open_file(root: &Path, path: &Path, kind: FileKind) -> Result<File> {
    let relative = path.strip_prefix(root)?;
    ensure!(
        allowed(path) && !relative.as_os_str().is_empty(),
        "excluded path"
    );
    #[cfg(target_os = "linux")]
    let file = File::from(openat2(
        directory(root)?,
        relative,
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
        ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_XDEV,
    )?);
    #[cfg(not(target_os = "linux"))]
    let file = {
        ensure!(no_links(path)?, "links are excluded");
        File::open(path)?
    };
    let metadata = file.metadata()?;
    ensure!(metadata.is_file(), "not a regular file");
    ensure!(
        kind != FileKind::Text || metadata.len() <= MAX_BYTES,
        "file exceeds the 16 MiB input limit"
    );
    Ok(file)
}

pub(crate) fn read(file: File) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "file exceeds the 16 MiB input limit"
    );
    Ok(bytes)
}

pub(crate) fn extract(extension: &str, bytes: Vec<u8>) -> Result<String> {
    if extension == "pdf" {
        return pdf_text(bytes);
    }
    let text = String::from_utf8(bytes).context("text must be UTF-8")?;
    ensure!(!text.contains('\0'), "text contains NUL bytes");
    Ok(text.chars().take(MAX_TEXT as usize).collect())
}

fn pdf_text(bytes: Vec<u8>) -> Result<String> {
    let mut child = Command::new("pdftotext")
        .args(["-enc", "UTF-8", "-nopgbrk", "-", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("start pdftotext (install poppler-utils)")?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let writer = std::thread::spawn(move || stdin.write_all(&bytes));
    let stdout = child.stdout.take().expect("piped stdout");
    let (sender, receiver) = mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        let read = stdout.take(MAX_TEXT + 1).read_to_end(&mut output);
        let _ = sender.send(read.map(|_| output));
    });
    let response = receiver.recv_timeout(Duration::from_secs(15));
    if !matches!(&response, Ok(Ok(output)) if output.len() as u64 <= MAX_TEXT) {
        let _ = child.kill();
    }
    let status = child.wait();
    let _ = reader.join();
    let written = writer
        .join()
        .map_err(|_| anyhow::anyhow!("PDF input thread stopped"))?;
    let mut output = response.context("wait for PDF extraction (15 second limit)")??;
    if output.len() as u64 > MAX_TEXT {
        output.truncate(MAX_TEXT as usize);
    } else {
        ensure!(status?.success(), "pdftotext could not extract text");
        written?;
    }
    let text = String::from_utf8_lossy(&output).trim().to_owned();
    if text.is_empty() {
        bail!("PDF has no extractable text");
    }
    Ok(text)
}

pub(crate) fn chunks(text: &str) -> impl Iterator<Item = &str> {
    let mut start = 0;
    std::iter::from_fn(move || {
        while start < text.len() {
            let rest = &text[start..];
            let end = rest
                .char_indices()
                .nth(CHUNK_SIZE)
                .map_or(rest.len(), |(i, _)| i);
            let end = if end < rest.len() {
                let window = &rest[..end];
                window
                    .rfind(char::is_whitespace)
                    .filter(|i| *i >= window.len() / 2)
                    .unwrap_or(end)
            } else {
                end
            };
            let chunk = rest[..end].trim();
            start += if end == rest.len() {
                end
            } else {
                rest[..end]
                    .char_indices()
                    .rev()
                    .nth(OVERLAP - 1)
                    .map_or(end, |(i, _)| i.max(1))
            };
            if !chunk.is_empty() {
                return Some(chunk);
            }
        }
        None
    })
}

pub(crate) fn index_directory(path: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    if let Some(parent) = absolute.parent() {
        fs::create_dir_all(parent)?;
    }
    let builder = fs::DirBuilder::new();
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = builder;
        builder.mode(0o700);
        builder
    };
    match builder.create(&absolute) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    let metadata = fs::symlink_metadata(&absolute)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "index directory must be a real directory"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            metadata.mode() & 0o077 == 0,
            "index directory must be private (chmod 700)"
        );
        #[cfg(target_os = "linux")]
        ensure!(
            metadata.uid() == rustix::process::geteuid().as_raw(),
            "index directory must belong to the current user"
        );
    }
    Ok(absolute)
}

pub(crate) fn index_file(directory: &Path) -> Result<PathBuf> {
    let path = directory.join("index.sqlite3");
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(&path) {
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    let metadata = fs::symlink_metadata(&path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "index must be a regular file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            metadata.nlink() == 1 && metadata.mode() & 0o077 == 0,
            "index file must be private (chmod 600) and have one link"
        );
        #[cfg(target_os = "linux")]
        ensure!(
            metadata.uid() == rustix::process::geteuid().as_raw(),
            "index file must belong to the current user"
        );
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitting_overlaps_and_handles_unicode() {
        let body = format!("{} {}", "ä".repeat(900), "privacy");
        let parts: Vec<_> = chunks(&body).collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].chars().count(), CHUNK_SIZE);
        assert!(parts[1].ends_with("privacy"));
        assert_eq!(parts[1].chars().count(), 900 - CHUNK_SIZE + OVERLAP + 8);
    }
}
