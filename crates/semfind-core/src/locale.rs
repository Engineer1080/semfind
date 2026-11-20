use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Language {
    En,
    De,
    Ru,
    Pt,
    Fr,
    Es,
    It,
}

impl Language {
    /// Resolve a language code or regional Linux locale.
    pub fn from_locale(locale: &str) -> Option<Self> {
        match locale.split(['_', '-', '.', '@']).next()? {
            "C" | "POSIX" | "en" => Some(Self::En),
            "de" => Some(Self::De),
            "ru" => Some(Self::Ru),
            "pt" => Some(Self::Pt),
            "fr" => Some(Self::Fr),
            "es" => Some(Self::Es),
            "it" => Some(Self::It),
            _ => None,
        }
    }

    /// Select the language for terminal messages from the process environment.
    pub fn current() -> Self {
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .filter_map(|key| env::var(key).ok())
            .find(|value| !value.is_empty());
        if locale
            .as_deref()
            .is_some_and(|value| matches!(value.split('.').next(), Some("C" | "POSIX")))
        {
            return Self::En;
        }
        env::var("LANGUAGE")
            .ok()
            .and_then(|value| value.split(':').find_map(Self::from_locale))
            .or_else(|| locale.as_deref().and_then(Self::from_locale))
            .unwrap_or(Self::En)
    }
}
