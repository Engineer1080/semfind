pub(crate) struct Setting {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub synonyms: &'static str,
    pub group: &'static str,
}

pub(crate) const SETTINGS: &[Setting] = &[
    Setting {
        id: "audio.output.volume",
        title: "Sound / Ausgabelautstärke",
        description: "Adjust volume and select speakers or headphones. Lautstärke und Audioausgabe für Lautsprecher und Kopfhörer einstellen.",
        synonyms: "Lautstärke ändern, Ton lauter machen, Ton stumm schalten, sound too quiet, change volume, mute speakers",
        group: "audio",
    },
    Setting {
        id: "display.brightness",
        title: "Display / Bildschirmhelligkeit",
        description: "Adjust the brightness of the built-in display. Helligkeit des integrierten Bildschirms regeln.",
        synonyms: "Mein Bildschirm ist zu dunkel, Monitor heller machen, dim screen, display is too dark, backlight",
        group: "display",
    },
    Setting {
        id: "network.wifi",
        title: "Network & Wireless / WLAN",
        description: "Connect to wireless networks and manage Wi-Fi connections. WLAN-Netzwerke verbinden und verwalten.",
        synonyms: "WLAN funktioniert nicht, kein Internet, join a hotspot, wireless connection, Wi-Fi not working",
        group: "network",
    },
    Setting {
        id: "input.keyboard",
        title: "Keyboard / Tastatur",
        description: "Choose keyboard layouts and manage typing preferences. Tastaturbelegung und Eingabeeinstellungen auswählen.",
        synonyms: "Tastaturlayout ändern, deutsche Tastatur, QWERTZ, change input language, switch keyboard layout",
        group: "input",
    },
    Setting {
        id: "power.battery",
        title: "Power & Battery / Energie und Akku",
        description: "Manage power profiles and inspect battery settings. Energieprofil und Akkueinstellungen verwalten.",
        synonyms: "Akku sparen, Energiesparmodus, Laptop entlädt sich, conserve battery, power saving",
        group: "power",
    },
];
