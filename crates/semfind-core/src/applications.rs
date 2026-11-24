use crate::Language;
use anyhow::{Context, Result};
use std::{
    collections::{BTreeMap, HashSet},
    env,
    fs::{self, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
    process::Command,
};
use walkdir::WalkDir;

pub(crate) struct Application {
    pub id: String,
    pub name: String,
    pub texts: Vec<String>,
}

pub(crate) struct DesktopEntries {
    pub applications: Vec<Application>,
    pub settings: Vec<Application>,
    pub desktop: Option<&'static str>,
}

pub(crate) const SETTING_PREFIX: &str = "setting.local.";

fn supported_desktop(session: &str) -> Option<&'static str> {
    session
        .split(':')
        .find_map(|name| match name.to_ascii_lowercase().as_str() {
            "cosmic" => Some("cosmic"),
            "gnome" | "gnome-classic" => Some("gnome"),
            "kde" | "plasma" | "plasmawayland" => Some("kde"),
            "cinnamon" | "x-cinnamon" | "cinnamon-wayland" => Some("cinnamon"),
            "xfce" | "xfce4" => Some("xfce"),
            "mate" => Some("mate"),
            "lxqt" => Some("lxqt"),
            _ => None,
        })
}

pub(crate) fn discover(home: &Path, include_applications: bool) -> Result<DesktopEntries> {
    let user_data = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"));
    let system_data = env::var_os("XDG_DATA_DIRS")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    let desktops = [
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_DESKTOP",
        "DESKTOP_SESSION",
    ]
    .into_iter()
    .filter_map(|key| env::var(key).ok())
    .find(|value| !value.is_empty())
    .unwrap_or_default();
    let desktop = supported_desktop(&desktops);
    let mut seen = HashSet::new();
    let mut applications = Vec::new();
    let mut settings = Vec::new();
    if desktop.is_none() && !include_applications {
        return Ok(DesktopEntries {
            applications,
            settings,
            desktop,
        });
    }
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
                && let Some((name, texts, setting)) =
                    parse_entry(&contents, &id, &desktops, desktop)
            {
                let entry = Application { id, name, texts };
                if setting {
                    settings.push(entry);
                } else if include_applications {
                    applications.push(entry);
                }
            }
        }
    }
    if desktop == Some("kde") {
        // Plasma 6 also registers settings that exist only as Qt plugins.
        if let Ok(output) = Command::new("kcmshell6")
            .arg("--list")
            .env("QT_QPA_PLATFORM", "offscreen")
            .output()
            && output.status.success()
        {
            settings.extend(parse_kde_modules(&String::from_utf8_lossy(&output.stdout)));
        }
    }
    Ok(DesktopEntries {
        applications,
        settings,
        desktop,
    })
}

fn parse_kde_modules(text: &str) -> impl Iterator<Item = Application> {
    let mut seen = HashSet::new();
    text.lines().filter_map(move |line| {
        let (module, description) = line.split_once(" - ")?;
        let module = module.trim();
        let description = description.trim();
        if !module.starts_with(|c: char| c.is_ascii_alphanumeric())
            || !module
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            || !seen.insert(module)
        {
            return None;
        }
        let name = if description.is_empty() {
            module
        } else {
            description
        };
        Some(Application {
            id: module.to_owned(),
            name: name.to_owned(),
            texts: vec![format!("{module} {description}")],
        })
    })
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

fn parse_entry(
    text: &str,
    id: &str,
    desktops: &str,
    desktop: Option<&str>,
) -> Option<(String, Vec<String>, bool)> {
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
            .any(|name| {
                list.split(';').any(|item| {
                    item.eq_ignore_ascii_case(name)
                        || desktop.is_some_and(|value| supported_desktop(item) == Some(value))
                })
            })
    };
    let exec = get("Exec").filter(|value| !value.trim().is_empty());
    let setting = exec
        .is_some_and(|exec| is_setting(desktop, id, exec, get("Categories").unwrap_or_default()));
    if get("Type") != Some("Application")
        || get("Hidden") == Some("true")
        || (get("NoDisplay") == Some("true") && !setting)
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
    Some((name, texts, setting))
}

fn is_setting(desktop: Option<&str>, id: &str, exec: &str, categories: &str) -> bool {
    let has_category = |name| categories.split(';').any(|category| category == name);
    let mut arguments = exec.split_whitespace();
    let program = arguments
        .next()
        .unwrap_or_default()
        .rsplit('/')
        .next()
        .unwrap_or_default();
    let panel = arguments
        .next()
        .is_some_and(|arg| !arg.starts_with(['-', '%']));
    let dedicated = match desktop {
        Some("cosmic") => {
            id.starts_with("com.system76.CosmicSettings.") && program == "cosmic-settings" && panel
        }
        Some("gnome") => program == "gnome-control-center" && panel,
        Some("cinnamon") => program == "cinnamon-settings" && panel,
        Some("xfce") => has_category("X-XFCE-SettingsDialog"),
        Some("mate") => program.starts_with("mate-") && has_category("Settings"),
        Some("lxqt") => {
            has_category("Settings")
                && (program.starts_with("lxqt-config-") || has_category("X-LXQt"))
        }
        _ => false,
    };
    dedicated
        || desktop.is_some()
            && has_category("Settings")
            && !has_category("PackageManager")
            && ["DesktopSettings", "HardwareSettings", "PersonalSettings"]
                .into_iter()
                .any(has_category)
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

#[cfg(test)]
mod tests {
    use super::{parse_entry, parse_kde_modules, supported_desktop};

    #[test]
    fn recognizes_native_settings_for_supported_desktops() {
        for (session, expected, id, exec, categories) in [
            (
                "COSMIC",
                "cosmic",
                "com.system76.CosmicSettings.Displays.desktop",
                "cosmic-settings displays",
                "COSMIC;",
            ),
            (
                "ubuntu:GNOME",
                "gnome",
                "gnome-display-panel.desktop",
                "gnome-control-center display",
                "GNOME;Settings;X-GNOME-Settings-Panel;",
            ),
            (
                "Plasma",
                "kde",
                "kde-display.desktop",
                "systemsettings kcm_kscreen",
                "Qt;KDE;Settings;HardwareSettings;",
            ),
            (
                "X-Cinnamon",
                "cinnamon",
                "cinnamon-display-panel.desktop",
                "cinnamon-settings display",
                "X-Cinnamon;",
            ),
            (
                "XFCE",
                "xfce",
                "xfce-display-settings.desktop",
                "xfce4-display-settings",
                "Settings;X-XFCE-SettingsDialog;",
            ),
            (
                "MATE",
                "mate",
                "mate-display-properties.desktop",
                "mate-display-properties",
                "GTK;Settings;HardwareSettings;",
            ),
            (
                "LXQt",
                "lxqt",
                "lxqt-config-monitor.desktop",
                "lxqt-config-monitor",
                "Qt;Settings;X-LXQt;",
            ),
        ] {
            let desktop = supported_desktop(session);
            assert_eq!(desktop, Some(expected));
            let entry = format!(
                "[Desktop Entry]\nType=Application\nName=Displays\nKeywords[de]=Auflösung;Hertz;\nNoDisplay=true\nExec={exec}\nCategories={categories}"
            );
            let (name, texts, setting) = parse_entry(&entry, id, session, desktop).unwrap();
            assert!(setting, "{id}");
            assert_eq!(name, "Displays");
            assert!(
                texts.iter().any(|text| text.contains("Auflösung Hertz")),
                "{id}"
            );
            for flags in [
                "Hidden=true",
                "OnlyShowIn=OtherDesktop;",
                "NotShowIn=cosmic;gnome;kde;cinnamon;xfce;mate;lxqt;",
            ] {
                assert!(
                    parse_entry(&format!("{entry}\n{flags}"), id, session, desktop).is_none(),
                    "{id}: {flags}"
                );
            }
        }
        let hidden_app =
            "[Desktop Entry]\nType=Application\nName=Editor\nExec=editor\nNoDisplay=true";
        assert!(parse_entry(hidden_app, "editor.desktop", "COSMIC", Some("cosmic")).is_none());
        let package_manager = "[Desktop Entry]\nType=Application\nName=Packages\nExec=packages\nCategories=Settings;HardwareSettings;PackageManager;";
        assert!(
            !parse_entry(
                package_manager,
                "packages.desktop",
                "COSMIC",
                Some("cosmic")
            )
            .unwrap()
            .2
        );
    }

    #[test]
    fn reads_distinct_plasma_modules_and_skips_non_module_output() {
        let output = "Available modules:\nkcm_kscreen  - Configure displays\nkcm_kscreen - Duplicate\n--help - Option\nkcm_audio - Configure audio - speakers\n";
        let modules: Vec<_> = parse_kde_modules(output).collect();
        assert_eq!(modules.len(), 2);
        assert_eq!(modules[0].id, "kcm_kscreen");
        assert_eq!(modules[1].name, "Configure audio - speakers");
    }
}
