"""Load the desktop app's messages using the same locale rules as the CLI."""
import json
import os
from pathlib import Path
import re

LOCALES = Path(__file__).resolve().parent / "locales"


def message_language(env):
    supported = {"en", *(path.stem for path in LOCALES.glob("*.json"))}

    def language(locale):
        code = re.split(r"[_\-.@]", locale, maxsplit=1)[0]
        if code in {"C", "POSIX"}:
            return "en"
        return code if code in supported else None

    locale = next((env[key] for key in ("LC_ALL", "LC_MESSAGES", "LANG") if env.get(key)), "")
    if locale.split(".", 1)[0] in {"C", "POSIX"}:
        return "en"
    preferences = env.get("LANGUAGE", "").split(":")
    return next((match for option in (*preferences, locale) if (match := language(option))), "en")


def load_messages(env=None):
    language = message_language(os.environ if env is None else env)
    if language == "en":
        return {}
    with (LOCALES / f"{language}.json").open(encoding="utf-8") as catalog:
        return json.load(catalog)


MESSAGES = load_messages()


def translate(message, **values):
    localized = MESSAGES.get(message, message)
    return localized.format(**values) if values else localized
