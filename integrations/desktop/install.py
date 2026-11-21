"""Install the Semfind D-Bus adapter for Plasma or GNOME."""

import argparse
import json
import os
from pathlib import Path
import shlex
import shutil
import sys
from tempfile import NamedTemporaryFile

BUS = "io.github.Engineer1080.Semfind"
CINNAMON_ID = "semfind@engineer1080"


def write(path, contents, mode=0o644):
    path.parent.mkdir(parents=True, exist_ok=True)
    with NamedTemporaryFile(dir=path.parent, delete=False) as stream:
        temporary = Path(stream.name)
        try:
            stream.write(contents)
            os.fchmod(stream.fileno(), mode)
            stream.close()
            temporary.replace(path)
        finally:
            temporary.unlink(missing_ok=True)


def executable(value):
    path = shutil.which(os.path.expanduser(value))
    if path is None:
        raise ValueError(f"executable not found: {value}")
    return str(Path(path).absolute())


def quoted(value):
    escaped = "".join(f"\\{char}" if char in '\\"$' or char == chr(96) else char for char in value)
    return f'"{escaped}"'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("desktop", choices=["plasma", "gnome", "both", "cinnamon", "rofi"])
    parser.add_argument("--semfind", default=os.environ.get("SEMFIND_CLI", "semfind"))
    args = parser.parse_args()
    try:
        import gi
        gi.require_version("Gio", "2.0")
        from gi.repository import Gio
        cli = executable(args.semfind)
        environment = {name: os.environ[name] for name in ("SEMFIND_MODEL", "SEMFIND_DEVICE")
                       if os.environ.get(name)}
        for name in ("SEMFIND_INDEX_DIR", "SEMFIND_MODEL_DIR"):
            if os.environ.get(name):
                environment[name] = str(Path(os.environ[name]).expanduser().absolute())
        if os.environ.get("SEMFIND_PYTHON"):
            environment["SEMFIND_PYTHON"] = executable(os.environ["SEMFIND_PYTHON"])
    except (ImportError, ValueError) as error:
        parser.error(str(error))

    source = Path(__file__).resolve().parent
    data = Path(os.environ.get("XDG_DATA_HOME", ""))
    if not data.is_absolute():
        data = Path.home() / ".local/share"
    app = data / "semfind/desktop"
    provider = app / "provider.py"
    command = f"{quoted(sys.executable)} {quoted(str(provider))}"
    write(provider, (source / "provider.py").read_bytes())
    write(app / "config.json",
          (json.dumps({"semfind": cli, "environment": environment}, indent=2) + "\n").encode(),
          0o600)
    write(data / f"dbus-1/services/{BUS}.service",
          f"[D-BUS Service]\nName={BUS}\nExec={command}\n".encode())
    if args.desktop in ("plasma", "both"):
        write(data / "krunner/dbusplugins/semfind-runner.desktop",
              (source / "semfind-runner.desktop").read_bytes())
        print("Installed Semfind for KRunner.")
    if args.desktop in ("gnome", "both"):
        desktop_exec = command.replace("%", "%%")
        write(data / f"applications/{BUS}.desktop",
              ("[Desktop Entry]\nType=Application\nName=Semfind\nIcon=system-search\n"
               f"OnlyShowIn=GNOME;\nTerminal=true\nExec={desktop_exec} --show\n").encode())
        print("Installed the user service for GNOME. Register the provider as shown in the README.")
    if args.desktop == "cinnamon":
        extension = data / "cinnamon/search_providers" / CINNAMON_ID
        for name in ("metadata.json", "search_provider.js"):
            write(extension / name, (source / "cinnamon" / name).read_bytes())
        schema = Gio.SettingsSchemaSource.get_default().lookup("org.cinnamon", True)
        key = "enabled-search-providers"
        if schema and schema.has_key(key):
            settings = Gio.Settings.new_full(schema, None, None)
            enabled = settings.get_strv(key)
            if CINNAMON_ID in enabled or settings.set_strv(key, [*enabled, CINNAMON_ID]):
                Gio.Settings.sync()
                print("Installed and enabled Semfind for the Cinnamon menu.")
            else:
                print("Installed Semfind for Cinnamon, but its settings are locked.")
        else:
            print("Installed Semfind for Cinnamon. Rerun this installer in a Cinnamon session to enable it.")
    if args.desktop == "rofi":
        write(app / "rofi.py", (source / "rofi.py").read_bytes())
        command = shlex.join([sys.executable, str(app / "rofi.py")])
        write(app / "semfind-rofi", f'#!/bin/sh\nexec {command} "$@"\n'.encode(), 0o755)
        print(f"Installed Semfind for Rofi: {app / 'semfind-rofi'}")


if __name__ == "__main__":
    main()
