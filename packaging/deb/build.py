#!/usr/bin/env python3
"""Build Semfind's amd64 package on Ubuntu or Pop!_OS 24.04."""

import argparse
from datetime import UTC, datetime
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib
import xml.etree.ElementTree as ET

PACKAGING = Path(__file__).resolve().parent
ROOT = PACKAGING.parent.parent
GUI = ROOT / "integrations/gui"
APP_ID = "io.github.engineer1080.semfind"
MAINTAINER = "Semfind contributors <Engineer1080@users.noreply.github.com>"


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def stage_package(build, version, wheel_dir):
    stage = build / "package"
    library = stage / "usr/lib/semfind"
    runtime = library / "python"
    bin_dir = stage / "usr/bin"
    doc = stage / "usr/share/doc/semfind"
    for directory in (runtime / "bin", bin_dir, doc, stage / "DEBIAN", build / "debian"):
        directory.mkdir(parents=True)

    cargo_target = ROOT / "target/deb-build"
    run("cargo", "build", "--locked", "--release", "-p", "semfind-cli",
        "--target-dir", cargo_target, cwd=ROOT)
    shutil.copy2(cargo_target / "release/semfind", library / "semfind")

    python_packages = [
        sys.executable, "-m", "pip", "--disable-pip-version-check", "install",
        "--only-binary=:all:", "--no-compile", "--target",
        runtime / "lib/python3.12/site-packages", "-r", ROOT / "requirements.txt",
    ]
    if wheel_dir:
        python_packages.extend(("--no-index", "--find-links", wheel_dir.resolve()))
    run(*python_packages)
    (runtime / "bin/python3.12").symlink_to("/usr/bin/python3.12")
    (runtime / "pyvenv.cfg").write_text("home = /usr/bin\ninclude-system-site-packages = false\n")
    run(runtime / "bin/python3.12", "-I", "-B", "-c", "import numpy, onnxruntime, tokenizers")

    for source, name in (
        (PACKAGING / "semfind", "semfind"),
        (ROOT / "scripts/download_model.py", "semfind-download-model"),
    ):
        target = bin_dir / name
        shutil.copyfile(source, target)
        target.chmod(0o755)

    shutil.copyfile(ROOT / "LICENSE", doc / "copyright")
    # dpkg-shlibdeps needs source metadata to derive dependencies from the executable.
    (build / "debian/control").write_text(
        "Source: semfind\n\nPackage: semfind\nArchitecture: any\nDescription: Semfind\n"
    )
    dependencies = run("dpkg-shlibdeps", "-O", f"-e{library / 'semfind'}",
                       cwd=build, text=True, stdout=subprocess.PIPE).stdout.strip()
    libraries = dependencies.removeprefix("shlibs:Depends=")
    if not libraries or libraries == dependencies:
        raise SystemExit("Could not detect shared-library dependencies with dpkg-shlibdeps.")

    (stage / "DEBIAN/control").write_text(
        f"Package: semfind\nVersion: {version}\nSection: utils\nPriority: optional\n"
        f"Architecture: amd64\nMaintainer: {MAINTAINER}\n"
        f"Depends: {libraries}, python3.12, libstdc++6, ca-certificates\n"
        "Recommends: python3.12-venv, poppler-utils, man-db\n"
        "Homepage: https://github.com/Engineer1080/semfind\n"
        "Description: Offline semantic search for Linux\n"
        " Find files by content, installed applications by purpose, and Linux\n"
        " commands by task. Search from a terminal or use JSON for automation.\n"
    )
    return stage


def stage_gui(build, version):
    stage = build / "gui"
    library = stage / "usr/lib/semfind-gui"
    bin_dir = stage / "usr/bin"
    doc = stage / "usr/share/doc/semfind-gui"
    apps = stage / "usr/share/applications"
    metainfo = stage / "usr/share/metainfo"
    for directory in (library, bin_dir, doc, apps, metainfo, stage / "DEBIAN"):
        directory.mkdir(parents=True)

    for filename in ("app.py", "style.css"):
        shutil.copyfile(GUI / filename, library / filename)
    (bin_dir / "semfind-gui").symlink_to("../lib/semfind-gui/app.py")
    (library / "app.py").chmod(0o755)
    shutil.copyfile(ROOT / "LICENSE", doc / "copyright")
    shutil.copyfile(PACKAGING / f"{APP_ID}.desktop", apps / f"{APP_ID}.desktop")
    for size in (64, 128):
        theme = stage / f"usr/share/icons/hicolor/{size}x{size}/apps"
        theme.mkdir(parents=True)
        shutil.copyfile(PACKAGING / f"icons/semfind-{size}.png", theme / f"{APP_ID}.png")
    metadata = ET.parse(PACKAGING / f"{APP_ID}.metainfo.xml")
    date = datetime.now(UTC).date().isoformat()
    ET.SubElement(ET.SubElement(metadata.getroot(), "releases"), "release", version=version, date=date)
    metadata.write(metainfo / f"{APP_ID}.metainfo.xml", encoding="utf-8", xml_declaration=True)

    (stage / "DEBIAN/control").write_text(
        f"Package: semfind-gui\nVersion: {version}\nSection: utils\nPriority: optional\n"
        f"Architecture: all\nMaintainer: {MAINTAINER}\n"
        f"Depends: semfind (>= {version}), python3, python3-gi, gir1.2-gtk-4.0 (>= 4.10)\n"
        "Breaks: semfind (<< 0.3.2-1)\nReplaces: semfind (<< 0.3.2-1)\n"
        "Homepage: https://github.com/Engineer1080/semfind\n"
        "Description: Desktop interface for Semfind local search\n"
        " Find files, applications, and Linux commands in the Semfind index.\n"
    )
    return stage


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "target/deb")
    parser.add_argument("--wheel-dir", type=Path, help="Install wheels from this directory without network access.")
    args = parser.parse_args()
    arch = subprocess.check_output(["dpkg", "--print-architecture"], text=True).strip()
    if arch != "amd64" or sys.version_info[:2] != (3, 12):
        parser.error("Build this package on amd64 with Python 3.12 and pip.")
    os.umask(0o022)
    upstream = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    version = f"{upstream}-3"
    args.output.mkdir(parents=True, exist_ok=True)
    output = args.output.resolve()
    with tempfile.TemporaryDirectory(prefix="semfind-deb-", dir=args.output) as directory:
        build = Path(directory).resolve()
        packages = (
            (stage_package(build, version, args.wheel_dir), f"semfind_{version}_amd64.deb"),
            (stage_gui(build, version), f"semfind-gui_{version}_all.deb"),
        )
        for stage, filename in packages:
            run("dpkg-deb", "--root-owner-group", "-Zxz", "-z3", "--build", stage, build / filename)
        for _, filename in packages:
            (build / filename).replace(output / filename)
            print(output / filename)


if __name__ == "__main__":
    main()
