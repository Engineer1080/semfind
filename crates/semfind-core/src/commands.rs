use serde::{Deserialize, Serialize};
use std::{fs, io};

/// Distribution family for Linux command examples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Distro {
    Generic,
    Debian,
    Arch,
}

impl Distro {
    /// Detect the installed Linux distribution family using os-release.
    pub fn current() -> Self {
        let release = fs::read_to_string("/etc/os-release").or_else(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                fs::read_to_string("/usr/lib/os-release")
            } else {
                Err(error)
            }
        });
        release
            .as_deref()
            .map_or(Self::Generic, Self::from_os_release)
    }

    fn from_os_release(release: &str) -> Self {
        let mut id = "";
        let mut id_like = "";
        for line in release.lines() {
            let Some((key, value)) = line.trim().split_once('=') else {
                continue;
            };
            let value = value.trim().trim_matches(['"', '\'']);
            match key {
                "ID" => id = value,
                "ID_LIKE" => id_like = value,
                _ => (),
            }
        }
        std::iter::once(id)
            .chain(id_like.split_whitespace())
            .find_map(|id| match id {
                "debian" | "ubuntu" | "pop" => Some(Self::Debian),
                "arch" | "manjaro" | "endeavouros" => Some(Self::Arch),
                _ => None,
            })
            .unwrap_or(Self::Generic)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::Debian => "debian",
            Self::Arch => "arch",
        }
    }

    pub(crate) fn from_str(value: &str) -> Option<Self> {
        match value {
            "generic" => Some(Self::Generic),
            "debian" => Some(Self::Debian),
            "arch" => Some(Self::Arch),
            _ => None,
        }
    }
}

pub(crate) struct CommandEntry {
    pub id: &'static str,
    pub title: &'static str,
    pub distro: Distro,
    pub description: &'static str,
    pub example: &'static str,
}

const fn entry(
    id: &'static str,
    title: &'static str,
    distro: Distro,
    description: &'static str,
    example: &'static str,
) -> CommandEntry {
    CommandEntry {
        id,
        title,
        distro,
        description,
        example,
    }
}

use Distro::{Arch, Debian, Generic};

pub(crate) const COMMANDS: &[CommandEntry] = &[
    entry(
        "command.disk.free",
        "df",
        Generic,
        "Freien und belegten Speicherplatz auf eingehängten Dateisystemen und Festplatten anzeigen. Show available disk space and usage for mounted filesystems.",
        "df -h",
    ),
    entry(
        "command.disk.directory-usage",
        "du",
        Generic,
        "Ordner nach belegtem Speicher sortieren. Herausfinden, welches Verzeichnis am meisten Platz verbraucht. Show disk usage by directory and identify the largest folders.",
        "du -h --max-depth=1 . | sort -hr",
    ),
    entry(
        "command.files.find-name",
        "find -name",
        Generic,
        "Dateien anhand des Dateinamens oder der Endung in allen Unterordnern suchen. Recursively find files by name or extension.",
        "find . -type f -name '*.pdf'",
    ),
    entry(
        "command.files.find-large",
        "find -size",
        Generic,
        "Große Dateien oberhalb einer Mindestgröße suchen, etwa Dateien größer als 100 MB. Find files larger than a given size recursively.",
        "find . -type f -size +100M",
    ),
    entry(
        "command.files.list",
        "ls",
        Generic,
        "Dateien im aktuellen Ordner mit Größe, Datum und Rechten auflisten, auch versteckte Dateien. List directory contents including hidden entries and permissions.",
        "ls -lah",
    ),
    entry(
        "command.files.current-directory",
        "pwd",
        Generic,
        "Aktuellen Ordner und seinen vollständigen Pfad anzeigen. Print the absolute path of the working directory.",
        "pwd",
    ),
    entry(
        "command.text.find",
        "grep",
        Generic,
        "Nach Text oder einem Fehlercode in Dateien und Unterverzeichnissen suchen, mit Zeilennummern. Search file contents recursively for matching text and show line numbers.",
        "grep -rni -- 'error' .",
    ),
    entry(
        "command.text.follow-log",
        "tail -f",
        Generic,
        "Eine Logdatei live mitlesen, während neue Zeilen angehängt werden. Follow a log file as new lines are appended.",
        "tail -f -- application.log",
    ),
    entry(
        "command.text.line-count",
        "wc -l",
        Generic,
        "Die Anzahl der Zeilen in einer Textdatei zählen. Count the lines in a text file.",
        "wc -l < datei.txt",
    ),
    entry(
        "command.archive.inspect",
        "tar -tf",
        Generic,
        "Inhalt eines tar.gz-Archivs auflisten und prüfen. List the contents of a compressed tar archive.",
        "tar -tzf archiv.tar.gz",
    ),
    entry(
        "command.archive.create",
        "tar -czf",
        Generic,
        "Einen Ordner als komprimiertes tar.gz-Archiv packen. Create a gzip-compressed tar archive from a directory.",
        "tar -czf archiv.tar.gz ordner/",
    ),
    entry(
        "command.system.memory",
        "free",
        Generic,
        "Arbeitsspeicher und Swap anzeigen. Wie viel RAM ist noch verfügbar? Show used and available RAM and swap.",
        "free -h",
    ),
    entry(
        "command.system.processes",
        "ps",
        Generic,
        "Laufende Prozesse nach CPU-Verbrauch sortieren und ihre Prozess-ID anzeigen. List running processes with PID and sort by CPU usage.",
        "ps -eo pid,comm,%cpu --sort=-%cpu | head -n 11",
    ),
    entry(
        "command.system.disks",
        "lsblk",
        Generic,
        "Festplatten, SSDs, USB-Laufwerke, Partitionen und Einhängepunkte anzeigen. List block devices, partitions and mount points.",
        "lsblk -f",
    ),
    entry(
        "command.system.service-status",
        "systemctl status",
        Generic,
        "Status eines systemd-Dienstes anzeigen und prüfen, ob er läuft. Inspect a systemd service and see whether it is running.",
        "systemctl status NetworkManager.service",
    ),
    entry(
        "command.system.boot-logs",
        "journalctl",
        Generic,
        "Systemprotokoll oder Journal des aktuellen Systemstarts anzeigen. Read systemd journal logs for the current boot.",
        "journalctl -b",
    ),
    entry(
        "command.network.addresses",
        "ip addr",
        Generic,
        "Eigene IP-Adressen und Netzwerkschnittstellen anzeigen. Show local IP addresses and network interfaces.",
        "ip -brief address",
    ),
    entry(
        "command.network.ports",
        "ss",
        Generic,
        "Offene lokale TCP- und UDP-Ports anzeigen, auf denen Dienste lauschen. List listening TCP and UDP sockets and their ports.",
        "ss -lntu",
    ),
    entry(
        "command.help.manual",
        "man",
        Generic,
        "Handbuch und Optionen zu einem Linux-Befehl nachschlagen. Open the manual page for a Linux command.",
        "man find",
    ),
    entry(
        "command.packages.search.debian",
        "apt search",
        Debian,
        "Suche nach verfügbaren Paketen. Ein Paket in den Paketquellen anhand von Name oder Stichwort finden. Search the configured repositories for packages by name or description.",
        "apt search ripgrep",
    ),
    entry(
        "command.packages.search.arch",
        "pacman -Ss",
        Arch,
        "Suche nach verfügbaren Paketen. Ein Paket in den Paketquellen anhand von Name oder Stichwort finden. Search the configured repositories for packages by name or description.",
        "pacman -Ss ripgrep",
    ),
    entry(
        "command.packages.install.debian",
        "apt install",
        Debian,
        "Software installieren. Ein neues Paket und seine Abhängigkeiten aus den Paketquellen installieren. Install a package and its dependencies.",
        "sudo apt install ripgrep",
    ),
    entry(
        "command.packages.install.arch",
        "pacman -S",
        Arch,
        "Software installieren. Ein neues Paket und seine Abhängigkeiten auf einem aktuellen System aus den Paketquellen installieren. Install a package and its dependencies on an up-to-date system.",
        "sudo pacman -S ripgrep",
    ),
    entry(
        "command.packages.upgrade.debian",
        "apt upgrade",
        Debian,
        "Das Betriebssystem aktualisieren und verfügbare Updates für installierte Pakete einspielen. Refresh package lists and upgrade installed packages.",
        "sudo apt update && sudo apt upgrade",
    ),
    entry(
        "command.packages.upgrade.arch",
        "pacman -Syu",
        Arch,
        "Das Betriebssystem aktualisieren und verfügbare Updates für installierte Pakete einspielen. Refresh package databases and perform a full system upgrade.",
        "sudo pacman -Syu",
    ),
    entry(
        "command.packages.list.debian",
        "apt list --installed",
        Debian,
        "Alle aktuell installierten Systempakete auflisten. List all installed packages.",
        "apt list --installed",
    ),
    entry(
        "command.packages.list.arch",
        "pacman -Q",
        Arch,
        "Alle aktuell installierten Systempakete auflisten. List all installed packages.",
        "pacman -Q",
    ),
    entry(
        "command.packages.details.debian",
        "apt show",
        Debian,
        "Version, Beschreibung und Abhängigkeiten für ein bekanntes Paket anzeigen. Show version, description, and dependencies for a known package.",
        "apt show ripgrep",
    ),
    entry(
        "command.packages.details.arch",
        "pacman -Si",
        Arch,
        "Version, Beschreibung und Abhängigkeiten für ein bekanntes Paket anzeigen. Show version, description, and dependencies for a known package.",
        "pacman -Si ripgrep",
    ),
    entry(
        "command.packages.owner.debian",
        "dpkg -S",
        Debian,
        "Herausfinden, zu welchem installierten Paket eine Datei oder ein Programm gehört. Identify the installed package that owns a file.",
        "dpkg -S /usr/bin/grep",
    ),
    entry(
        "command.packages.owner.arch",
        "pacman -Qo",
        Arch,
        "Herausfinden, zu welchem installierten Paket eine Datei oder ein Programm gehört. Identify the installed package that owns a file.",
        "pacman -Qo /usr/bin/grep",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_debian_arch_and_derivatives_from_os_release() {
        assert_eq!(Distro::from_os_release("ID=debian"), Distro::Debian);
        assert_eq!(Distro::from_os_release("ID=arch"), Distro::Arch);
        assert_eq!(
            Distro::from_os_release("ID=pop\nID_LIKE=\"ubuntu debian\""),
            Distro::Debian
        );
        assert_eq!(
            Distro::from_os_release("ID=derivative\nID_LIKE='arch debian'"),
            Distro::Arch
        );
        assert_eq!(
            Distro::from_os_release("ID=debian\nID_LIKE=arch"),
            Distro::Debian
        );
        assert_eq!(Distro::from_os_release("ID=fedora"), Distro::Generic);
    }
}
