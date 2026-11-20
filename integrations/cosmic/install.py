#!/usr/bin/env python3
"""Install the Semfind plugin into the user-local Pop Launcher directory."""

import argparse
import json
import os
from pathlib import Path
import shutil
import sys
from tempfile import NamedTemporaryFile


def executable(value):
    path = shutil.which(os.path.expanduser(value))
    if path is None:
        raise ValueError(f"executable not found: {value}")
    # Preserve virtual environment symlinks when recording the Python executable.
    return str(Path(path).absolute())


def write_file(destination, data, mode):
    with NamedTemporaryFile(dir=destination.parent, delete=False) as stream:
        temporary = Path(stream.name)
        try:
            stream.write(data)
            os.fchmod(stream.fileno(), mode)
            stream.close()
            temporary.replace(destination)
        finally:
            temporary.unlink(missing_ok=True)


def main():
    source = Path(__file__).resolve().parent
    release = source.parents[1] / "target/release"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plugin-binary", default=str(release / "semfind-cosmic"))
    parser.add_argument(
        "--semfind",
        default=os.environ.get("SEMFIND_CLI") or shutil.which("semfind") or str(release / "semfind"),
        help="semfind executable that the launcher should start",
    )
    args = parser.parse_args()
    try:
        binary = Path(executable(args.plugin_binary))
        semfind = executable(args.semfind)
        environment = {
            name: os.environ[name]
            for name in ("SEMFIND_MODEL", "SEMFIND_DEVICE")
            if os.environ.get(name)
        }
        for name in ("SEMFIND_MODEL_DIR", "SEMFIND_INDEX_DIR"):
            if os.environ.get(name):
                environment[name] = str(Path(os.environ[name]).expanduser().absolute())
        if python := os.environ.get("SEMFIND_PYTHON"):
            environment["SEMFIND_PYTHON"] = executable(python)
        elif sys.prefix != sys.base_prefix:
            environment["SEMFIND_PYTHON"] = sys.executable
    except ValueError as error:
        parser.error(str(error))

    destination = Path.home() / ".local/share/pop-launcher/plugins/semfind"
    destination.mkdir(parents=True, exist_ok=True)
    config = json.dumps({"semfind": semfind, "environment": environment}, indent=2) + "\n"
    write_file(destination / "config.json", config.encode("utf-8"), 0o600)
    write_file(destination / "semfind-cosmic", binary.read_bytes(), 0o755)
    write_file(destination / "plugin.ron", (source / "plugin.ron").read_bytes(), 0o644)
    print(f"Installed Semfind for Pop Launcher in {destination}")


if __name__ == "__main__":
    main()
