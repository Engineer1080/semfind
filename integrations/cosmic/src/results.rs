use anyhow::{Context, Result};
use semfind_core::{Category, FileKind, Hit};
use serde::Serialize;
use std::{
    ffi::OsStr,
    path::Path,
    process::{Command, Stdio},
    thread,
};

#[derive(Serialize)]
pub(crate) struct LauncherResult {
    id: u32,
    name: String,
    description: String,
    icon: Icon,
}

#[derive(Serialize)]
enum Icon {
    Name(&'static str),
}

impl LauncherResult {
    pub fn from_hit(id: u32, hit: &Hit) -> Self {
        let details = match (&hit.example, &hit.path, hit.snippet.is_empty()) {
            (Some(example), _, false) => format!("{example} · {}", hit.snippet),
            (Some(example), _, true) => example.clone(),
            (None, Some(path), true) => path.display().to_string(),
            _ => hit.snippet.clone(),
        };
        let icon = match hit.category {
            Category::File => match hit.extension.as_deref().and_then(FileKind::from_extension) {
                Some(FileKind::Image) => "image-x-generic",
                Some(FileKind::Video) => "video-x-generic",
                Some(FileKind::Audio) => "audio-x-generic",
                _ => "text-x-generic",
            },
            Category::Application => "applications-other",
            Category::Setting => "preferences-system",
            Category::Command => "utilities-terminal",
            Category::Action => "system-search",
        };
        Self {
            id,
            name: display_text(&hit.title, 120),
            description: display_text(&details, 180),
            icon: Icon::Name(icon),
        }
    }
}

fn display_text(text: &str, limit: usize) -> String {
    let cleaned: String = text
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    let line = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let Some((end, _)) = line.char_indices().nth(limit) else {
        return line;
    };
    format!("{}…", &line[..end])
}

pub(crate) fn activate(hit: &Hit) -> Result<bool> {
    let mut command = match hit.category {
        Category::File => {
            let Some(path) = hit.path.as_deref().filter(|path| {
                path.is_absolute() && path.symlink_metadata().is_ok_and(|meta| meta.is_file())
            }) else {
                return Ok(false);
            };
            let mut launcher = Command::new("xdg-open");
            launcher.arg(path);
            launcher
        }
        Category::Application => {
            if !hit.id.ends_with(".desktop")
                || Path::new(&hit.id).file_name() != Some(OsStr::new(&hit.id))
            {
                return Ok(false);
            }
            let mut launcher = Command::new("gtk-launch");
            launcher.args(["--", &hit.id]);
            launcher
        }
        Category::Setting => {
            if let Some(id) = hit.id.strip_prefix("setting.local.cosmic.") {
                if !id.ends_with(".desktop") || Path::new(id).file_name() != Some(OsStr::new(id)) {
                    return Ok(false);
                }
                let mut launcher = Command::new("gtk-launch");
                launcher.args(["--", id]);
                launcher
            } else {
                let page = match hit.id.as_str() {
                    "audio.output.volume" => "sound",
                    "display.brightness" | "display.mode" => "displays",
                    "system.language" => "region-language",
                    "network.wifi" => "wireless",
                    "input.keyboard" => "keyboard",
                    "power.battery" => "power",
                    _ => return Ok(false),
                };
                let mut launcher = Command::new("cosmic-settings");
                launcher.arg(page);
                launcher
            }
        }
        Category::Command | Category::Action => return Ok(false),
    };

    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .with_context(|| format!("start {program}"))?;
    thread::spawn(move || match child.wait() {
        Ok(status) if !status.success() => {
            eprintln!("semfind-cosmic: {program} exited with {status}")
        }
        Err(error) => eprintln!("semfind-cosmic: wait for {program}: {error}"),
        _ => (),
    });
    Ok(true)
}
