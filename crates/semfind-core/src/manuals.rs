use crate::Language;
use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs::{self, OpenOptions},
    io::{self, Read},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

pub(crate) const KEY_PREFIX: &str = "command.local.";
const COMPRESSION: &[(&str, &str)] = &[
    (".gz", "gzip"),
    (".xz", "xz"),
    (".bz2", "bzip2"),
    (".zst", "zstd"),
];
const MAX_TEXT: u64 = 256 * 1024;

pub(crate) struct LocalCommand {
    pub key: String,
    pub name: String,
    pub text: Option<String>,
    pub example: Option<String>,
}

pub(crate) fn discover() -> Result<Vec<LocalCommand>> {
    let language = Language::current();
    let mut pages = BTreeMap::new();
    for root in manual_roots()? {
        let Some(entries) = read_dir(&root)? else {
            continue;
        };
        let root = root.canonicalize()?;
        collect_pages(&root, &root, 0, &mut pages)?;
        for entry in entries {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let code = name.split(['_', '.', '@']).next().unwrap_or_default();
            if !(2..=3).contains(&code.len())
                || !code.bytes().all(|byte| byte.is_ascii_lowercase())
                || !entry.file_type()?.is_dir()
            {
                continue;
            }
            let priority = match Language::from_locale(name) {
                Some(locale) if locale == language => 1,
                Some(Language::De | Language::En) => 2,
                Some(_) => 3,
                None => 4,
            };
            collect_pages(&root, &root.join(name), priority, &mut pages)?;
        }
    }

    let mut documented = BTreeSet::new();
    let mut commands = Vec::new();
    for ((name, section), mut files) in pages {
        files.sort_by_key(|page| page.priority);
        let mut excerpts = Vec::new();
        for page in files {
            let Ok(contents) = read_page(&page.root, &page.path, 0) else {
                continue;
            };
            if let Some(text) = excerpt(&contents)
                && !excerpts.contains(&text)
            {
                excerpts.push(text);
            }
            if excerpts.len() == 2 {
                break;
            }
        }
        let text = (!excerpts.is_empty()).then(|| excerpts.join("\n"));
        documented.insert(name.clone());
        commands.push(LocalCommand {
            key: format!("{KEY_PREFIX}man.{section}.{name}"),
            example: Some(format!("man -- {section} {}", shell_argument(&name))),
            name,
            text,
        });
    }
    for name in executable_names()? {
        if !documented.contains(&name) {
            commands.push(LocalCommand {
                key: format!("{KEY_PREFIX}path.{name}"),
                name,
                text: None,
                example: None,
            });
        }
    }
    Ok(commands)
}

fn manual_roots() -> Result<Vec<PathBuf>> {
    match Command::new("manpath").arg("-q").output() {
        Ok(output) => {
            ensure!(
                output.status.success(),
                "manpath failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
            Ok(
                env::split_paths(String::from_utf8_lossy(&output.stdout).trim())
                    .filter(|path| path.is_absolute())
                    .collect(),
            )
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if let Some(paths) = env::var_os("MANPATH").filter(|value| !value.is_empty()) {
                Ok(env::split_paths(&paths)
                    .filter(|path| path.is_absolute())
                    .collect())
            } else {
                Ok(["/usr/local/share/man", "/usr/share/man"]
                    .map(PathBuf::from)
                    .to_vec())
            }
        }
        Err(error) => Err(error).context("discover manual page directories"),
    }
}

fn read_dir(path: &Path) -> Result<Option<Vec<fs::DirEntry>>> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    let mut entries: Vec<_> = entries.collect::<io::Result<_>>()?;
    entries.sort_unstable_by_key(fs::DirEntry::file_name);
    Ok(Some(entries))
}

struct Page {
    root: PathBuf,
    path: PathBuf,
    priority: u8,
}

fn collect_pages(
    root: &Path,
    directory: &Path,
    priority: u8,
    pages: &mut BTreeMap<(String, String), Vec<Page>>,
) -> Result<()> {
    for dir in read_dir(directory)?.into_iter().flatten() {
        if !dir.file_name().to_str().is_some_and(section_directory) || !dir.path().is_dir() {
            continue;
        }
        for file in read_dir(&dir.path())?.into_iter().flatten() {
            let kind = file.file_type()?;
            if !(kind.is_file() || kind.is_symlink()) {
                continue;
            }
            let filename = file.file_name();
            let Some((name, section)) = filename.to_str().and_then(page_name) else {
                continue;
            };
            pages
                .entry((name.to_owned(), section.to_owned()))
                .or_default()
                .push(Page {
                    root: root.to_path_buf(),
                    path: file.path(),
                    priority,
                });
        }
    }
    Ok(())
}

fn section_directory(name: &str) -> bool {
    name.strip_prefix("man").is_some_and(command_section)
}

fn command_section(section: &str) -> bool {
    section.starts_with(['1', '6', '8'])
        && section.bytes().all(|byte| byte.is_ascii_alphanumeric())
        && !section.as_bytes().get(1).is_some_and(u8::is_ascii_digit)
}

fn page_name(filename: &str) -> Option<(&str, &str)> {
    let filename = COMPRESSION
        .iter()
        .find_map(|(suffix, _)| filename.strip_suffix(suffix))
        .unwrap_or(filename);
    let (name, section) = filename.rsplit_once('.')?;
    (!name.is_empty() && command_section(section)).then_some((name, section))
}

fn executable_names() -> Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    for directory in env::var_os("PATH")
        .iter()
        .flat_map(env::split_paths)
        .filter(|path| path.is_absolute())
    {
        for entry in read_dir(&directory)?.into_iter().flatten() {
            let Ok(metadata) = fs::metadata(entry.path()) else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o111 == 0 {
                    continue;
                }
            }
            if let Ok(name) = entry.file_name().into_string() {
                names.insert(name);
            }
        }
    }
    Ok(names)
}

fn read_page(root: &Path, path: &Path, depth: usize) -> Result<String> {
    ensure!(depth < 8, "manual page aliases nested too deeply");
    let resolved = path.canonicalize()?;
    ensure!(
        resolved.starts_with(root),
        "manual page points outside its hierarchy"
    );
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32);
    }
    let file = options.open(&resolved)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= 4 * 1024 * 1024,
        "unsupported manual page"
    );
    let mut bytes = Vec::new();
    let decoder = resolved.to_str().and_then(|path| {
        COMPRESSION
            .iter()
            .find_map(|(suffix, program)| path.ends_with(suffix).then_some(program))
    });
    if let Some(decoder) = decoder {
        let mut child = Command::new(decoder)
            .arg("-dc")
            .stdin(file)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let result = child
            .stdout
            .take()
            .context("manual decompressor output")?
            .take(MAX_TEXT + 1)
            .read_to_end(&mut bytes);
        let truncated = bytes.len() as u64 > MAX_TEXT;
        if result.is_err() || truncated {
            let _ = child.kill();
        }
        let status = child.wait()?;
        result?;
        ensure!(
            truncated || status.success(),
            "could not decompress manual page"
        );
    } else {
        file.take(MAX_TEXT).read_to_end(&mut bytes)?;
    }
    bytes.truncate(MAX_TEXT as usize);
    let text = String::from_utf8_lossy(&bytes);
    let alias = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with(".\\\""))
        .and_then(|line| line.strip_prefix(".so "));
    if let Some(alias) = alias {
        let relative = Path::new(alias.trim());
        ensure!(
            relative
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
            "invalid manual alias"
        );
        // A localized alias first resolves relative to the language hierarchy.
        let local = path.parent().and_then(Path::parent).unwrap_or(root);
        for base in [local, root] {
            for suffix in std::iter::once("").chain(COMPRESSION.iter().map(|(suffix, _)| *suffix)) {
                let target = base.join(format!("{}{suffix}", relative.display()));
                if target.is_file() {
                    return read_page(root, &target, depth + 1);
                }
            }
        }
        anyhow::bail!("manual alias target is missing");
    }
    Ok(text.into_owned())
}

fn shell_argument(name: &str) -> String {
    if name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.+".contains(&byte))
    {
        name.to_owned()
    } else {
        format!("'{}'", name.replace('\'', "'\\''"))
    }
}

fn excerpt(source: &str) -> Option<String> {
    let mut section = String::new();
    let mut name = Vec::new();
    let mut synopsis = Vec::new();
    let mut description = Vec::new();
    for row in source.lines() {
        let row = row.trim();
        if let Some(heading) = row
            .strip_prefix(".SH ")
            .or_else(|| row.strip_prefix(".Sh "))
        {
            section = heading.trim_matches('"').to_uppercase();
            if !description.is_empty() {
                break;
            }
            continue;
        }
        let target = match section.as_str() {
            "NAME" | "BEZEICHNUNG" | "NOM" | "NOME" | "NOMBRE" | "ИМЯ" | "НАЗВАНИЕ" => {
                &mut name
            }
            "SYNOPSIS" | "ÜBERSICHT" | "SYNTAX" | "AUFRUF" | "SINTAXIS" | "SINOPSIS"
            | "SINTASSI" | "SINOPSE" | "SINTAXE" | "SYNTAXE" | "СИНТАКСИС" | "ОБЗОР" => {
                &mut synopsis
            }
            "DESCRIPTION" | "BESCHREIBUNG" | "DESCRIPCIÓN" | "DESCRIÇÃO" | "DESCRIZIONE"
            | "ОПИСАНИЕ"
                if description.len() < 4 =>
            {
                &mut description
            }
            _ => continue,
        };
        if target.len() < 12
            && let Some(line) = clean_line(row)
        {
            target.push(line);
        }
    }
    let name: String = name.join(" ").chars().take(180).collect();
    let synopsis: String = synopsis.join(" ").chars().take(180).collect();
    let description: String = description.join(" ").chars().take(180).collect();
    let pieces = [name, synopsis, description];
    let excerpt = pieces
        .into_iter()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    (!excerpt.is_empty()).then_some(excerpt)
}

fn clean_line(line: &str) -> Option<String> {
    let text = if let Some(request) = line.strip_prefix('.') {
        let (name, args) = request.split_once(char::is_whitespace)?;
        match name {
            "B" | "BI" | "BR" | "I" | "IB" | "IR" | "RB" | "RI" | "SM" | "SY" | "Nm" | "Nd"
            | "Ar" | "Pa" | "Cm" | "Ic" | "Em" | "Li" | "No" => args.trim(),
            "Fl" => return Some(format!("-{}", inline_text(args.trim()))),
            "Op" => {
                return Some(format!(
                    "[{}]",
                    inline_text(args.trim()).replace("Fl ", "-")
                ));
            }
            _ => return None,
        }
    } else {
        line
    };
    let cleaned = inline_text(text);
    (!cleaned.is_empty()).then_some(cleaned)
}

fn inline_text(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('f') => match chars.next() {
                Some('(') => {
                    chars.by_ref().take(2).for_each(drop);
                }
                Some('[') => {
                    chars.by_ref().take_while(|c| *c != ']').for_each(drop);
                }
                _ => (),
            },
            Some('(') => {
                let name: String = chars.by_ref().take(2).collect();
                if matches!(name.as_str(), "em" | "en" | "mi") {
                    out.push('-');
                }
            }
            Some('[') => {
                let name: String = chars.by_ref().take_while(|c| *c != ']').collect();
                if matches!(name.as_str(), "em" | "en" | "mi") {
                    out.push('-');
                }
            }
            Some('"') | Some('#') => break,
            Some('&' | '/' | ',' | 'c' | '|') => (),
            Some('~' | ' ' | '0') => out.push(' '),
            Some('e') => out.push('\\'),
            Some(other) => out.push(other),
            None => (),
        }
    }
    out.split_whitespace()
        .map(|word| word.trim_matches('"'))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::excerpt;

    #[test]
    fn extracts_localized_manual_sections() {
        for (name, synopsis, description) in [
            ("NOM", "SYNTAXE", "DESCRIPTION"),
            ("NOME", "SINOPSE", "DESCRIÇÃO"),
            ("NOMBRE", "SINOPSIS", "DESCRIPCIÓN"),
            ("NOME", "SINTASSI", "DESCRIZIONE"),
            ("ИМЯ", "СИНТАКСИС", "ОПИСАНИЕ"),
        ] {
            let source = format!(
                ".SH \"{name}\"\nprobe \\- name\n.SH \"{synopsis}\"\n.B probe\nFILE\n.SH \"{description}\"\nDetail"
            );
            assert_eq!(
                excerpt(&source).as_deref(),
                Some("probe - name\nprobe FILE\nDetail")
            );
        }
    }
}
