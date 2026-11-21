#!/usr/bin/env python3
"""Build the native Linux COSMIC Launcher plugin and write a user-local install bundle."""

import io
import os
from pathlib import Path
import platform
import subprocess
import tarfile
from tempfile import TemporaryDirectory
import tomllib


def main():
    if platform.system() != "Linux":
        raise SystemExit("Build this COSMIC package on Linux.")

    source = Path(__file__).resolve().parent
    root = source.parents[1]
    with (root / "Cargo.toml").open("rb") as manifest:
        version = tomllib.load(manifest)["workspace"]["package"]["version"]
    subprocess.run(
        ["cargo", "build", "--locked", "--release", "-p", "semfind-cosmic"],
        cwd=root,
        check=True,
    )

    target = root / os.environ.get("CARGO_TARGET_DIR", "target")
    bundle = f"semfind-cosmic-{version}-linux-{platform.machine()}"
    output = target / "cosmic" / f"{bundle}.tar.gz"
    output.parent.mkdir(parents=True, exist_ok=True)
    with TemporaryDirectory(dir=output.parent) as stage:
        staged = Path(stage) / output.name
        with tarfile.open(staged, "w:gz") as archive:
            for path, mode in (
                (target / "release/semfind-cosmic", 0o755),
                (source / "plugin.ron", 0o644),
                (source / "install.py", 0o644),
                (source / "README.md", 0o644),
                (root / "LICENSE", 0o644),
            ):
                content = path.read_bytes()
                info = tarfile.TarInfo(f"{bundle}/{path.name}")
                info.size = len(content)
                info.mode = mode
                archive.addfile(info, io.BytesIO(content))
        staged.replace(output)
    print(output)


if __name__ == "__main__":
    main()
