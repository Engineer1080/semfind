use crate::Language;
use anyhow::{Context, Result};
use std::{
    collections::{BTreeMap, HashSet},
    env,
    fs::{self, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

pub(crate) struct Application {
    pub id: String,
    pub name: String,
    pub texts: Vec<String>,
}

pub(crate) fn discover(home: &Path) -> Result<Vec<Application>> {
    let user_data = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"));
    let system_data = env::var_os("XDG_DATA_DIRS")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    let desktops = env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let mut seen = HashSet::new();
    let mut applications = Vec::new();
    for base in std::iter::once(user_data).chain(env::split_paths(&system_data)) {
        if !base.is_absolute() {
            continue;
        }
        let root = base.join("applications");
        if !root.try_exists()? {
            continue;
        }
        for entry in WalkDir::new(&root).sort_by_file_name() {
            let entry =
                entry.with_context(|| format!("scan desktop entries in {}", root.display()))?;
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "desktop")
                || !(entry.file_type().is_file() || entry.file_type().is_symlink())
            {
                continue;
            }
            let Some(relative) = path.strip_prefix(&root)?.to_str() else {
                continue;
            };
            let id = relative.replace('/', "-");
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(contents) = read_entry(path)?
                && let Some((name, texts)) = parse_entry(&contents, &desktops)
            {
                applications.push(Application { id, name, texts });
            }
        }
    }
    Ok(applications)
}

fn read_entry(path: &Path) -> Result<Option<String>> {
    const MAX_BYTES: u64 = 128 * 1024;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    if !file.metadata()?.is_file() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    Ok((bytes.len() as u64 <= MAX_BYTES && !bytes.contains(&0))
        .then(|| String::from_utf8(bytes).ok())
        .flatten())
}

fn parse_entry(text: &str, desktops: &str) -> Option<(String, Vec<String>)> {
    let mut lines = text.lines().map(str::trim);
    lines.find(|line| *line == "[Desktop Entry]")?;
    let entries: Vec<_> = lines
        .take_while(|line| !line.starts_with('['))
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .collect();
    let get = |name: &str| {
        entries
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| *value)
    };
    let matches_desktop = |list: &str| {
        desktops
            .split(':')
            .filter(|name| !name.is_empty())
            .any(|name| list.split(';').any(|item| item == name))
    };
    if get("Type") != Some("Application")
        || get("Hidden") == Some("true")
        || get("NoDisplay") == Some("true")
        || get("OnlyShowIn").is_some_and(|list| !matches_desktop(list))
        || get("NotShowIn").is_some_and(matches_desktop)
        || get("TryExec").is_some_and(|program| !try_exec(&unescape(program)))
    {
        return None;
    }
    let name = unescape(get("Name")?);
    if name.trim().is_empty() {
        return None;
    }
    let mut localized: BTreeMap<Language, Vec<String>> = BTreeMap::new();
    for (key, value) in entries {
        let (field, locale) = key.split_once('[').unwrap_or((key, "en"));
        if !matches!(field, "Name" | "GenericName" | "Comment" | "Keywords") {
            continue;
        }
        let Some(language) = Language::from_locale(locale.trim_end_matches(']')) else {
            continue;
        };
        let mut text = unescape(value);
        if field == "Keywords" {
            text = text.replace(';', " ");
        }
        let parts = localized.entry(language).or_default();
        if !text.trim().is_empty() && !parts.contains(&text) {
            parts.push(text);
        }
    }
    let texts = localized
        .into_values()
        .map(|parts| parts.join("\n"))
        .collect();
    Some((name, texts))
}

fn try_exec(program: &str) -> bool {
    let path = Path::new(program);
    let executable = |path: &Path| {
        fs::metadata(path).is_ok_and(|metadata| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            }
            #[cfg(not(unix))]
            metadata.is_file()
        })
    };
    if path.is_absolute() {
        executable(path)
    } else {
        path.file_name().is_some_and(|name| name == program)
            && env::var_os("PATH")
                .iter()
                .flat_map(env::split_paths)
                .any(|dir| executable(&dir.join(path)))
    }
}

fn unescape(value: &str) -> String {
    let mut result = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        let decoded = match chars.next() {
            Some('s') => ' ',
            Some('n') => '\n',
            Some('t') => '\t',
            Some('r') => '\r',
            Some('\\') | None => '\\',
            Some(';') => ';',
            Some(other) => {
                result.push('\\');
                other
            }
        };
        result.push(decoded);
    }
    result
}
