#!/usr/bin/env python3
"""Prepare a signed Semfind APT repository for Ubuntu and Pop!_OS 24.04."""

import argparse
import gzip
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import xml.etree.ElementTree as ET

APP_ID = "io.github.engineer1080.semfind"


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def package_identity(package):
    def field(name):
        return subprocess.check_output(["dpkg-deb", "--field", package, name], text=True).strip()

    name = field("Package")
    architecture = field("Architecture")
    if (name, architecture) not in {("semfind", "amd64"), ("semfind-gui", "all")}:
        raise ValueError(f"Expected a Semfind CLI or desktop package: {package}")
    version = field("Version")
    run("dpkg", "--validate-version", version)
    return name, version, architecture


def write_index(path, content):
    path.write_bytes(content)
    path.with_name(path.name + ".gz").write_bytes(gzip.compress(content, mtime=0))


def write_catalog(newest, dep11, build):
    extracted = build / "extracted"
    run("dpkg-deb", "--extract", newest, extracted)
    metadata = extracted / f"usr/share/metainfo/{APP_ID}.metainfo.xml"
    run("appstreamcli", "validate", "--no-net", metadata)
    component = ET.parse(metadata).getroot()
    ET.SubElement(component, "pkgname").text = "semfind-gui"
    ET.SubElement(component, "icon", type="stock").text = "system-search"
    catalog = ET.Element("components", origin="semfind", version="0.16")
    catalog.append(component)
    xml = build / "catalog.xml"
    yaml = build / "catalog.yml"
    ET.ElementTree(catalog).write(xml, encoding="utf-8", xml_declaration=True)
    run("appstreamcli", "convert", xml, yaml)
    write_index(dep11 / "Components-amd64.yml", yaml.read_bytes())


def publish_snapshot(build, packages, signing_key):
    repo = build / "repo"
    pool = repo / "pool/main/s/semfind"
    dist = repo / "dists/noble"
    packages_dir = dist / "main/binary-amd64"
    dep11 = dist / "main/dep11"
    for directory in (pool, packages_dir, dep11):
        directory.mkdir(parents=True)

    latest = {}
    for package in packages:
        name, version, architecture = package_identity(package)
        stored = pool / f"{name}_{version}_{architecture}.deb"
        if stored.exists():
            raise ValueError(f"Duplicate {name} package version: {version}")
        shutil.copyfile(package, stored)
        if name not in latest or subprocess.run(
            ["dpkg", "--compare-versions", version, "gt", latest[name][0]]
        ).returncode == 0:
            latest[name] = (version, stored)

    if "semfind" not in latest:
        raise ValueError("Include a Semfind CLI package in the repository.")
    desktop = latest.get("semfind-gui")
    if desktop and subprocess.run(
        ["dpkg", "--compare-versions", latest["semfind"][0], "lt", desktop[0]]
    ).returncode == 0:
        raise ValueError("Include a CLI package at least as recent as the desktop package.")
    write_index(packages_dir / "Packages",
                subprocess.check_output(["apt-ftparchive", "packages", "pool"], cwd=repo))
    if desktop:
        write_catalog(desktop[1], dep11, build)

    fields = {
        "Origin": "Semfind", "Label": "Semfind", "Suite": "noble", "Codename": "noble",
        "Architectures": "amd64", "Components": "main", "Description": "Semfind packages for Ubuntu and Pop!_OS 24.04",
    }
    release = subprocess.check_output([
        "apt-ftparchive",
        *(f"-oAPT::FTPArchive::Release::{name}={value}" for name, value in fields.items()),
        "release", ".",
    ], cwd=dist)
    if desktop and b"main/dep11/Components-amd64.yml.gz" not in release:
        raise ValueError("The Release file is missing AppStream catalog checksums.")
    (dist / "Release").write_bytes(release)
    run("gpg", "--batch", "--local-user", signing_key, "--digest-algo", "SHA256",
        "--output", dist / "InRelease", "--clearsign", dist / "Release")
    public_key = repo / "semfind-archive-keyring.gpg"
    with public_key.open("wb") as keyring:
        run("gpg", "--batch", "--export-options", "export-minimal", "--export", signing_key, stdout=keyring)
    if not public_key.stat().st_size:
        raise ValueError("The signing key could not be exported.")
    run("gpgv", "--keyring", public_key.resolve(), dist / "InRelease")
    return repo


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path, help="Path for a new repository snapshot.")
    parser.add_argument("--signing-key", required=True, help="Fingerprint of the configured OpenPGP archive key.")
    parser.add_argument("packages", nargs="+", type=Path)
    args = parser.parse_args()
    if not re.fullmatch(r"(?:[0-9a-fA-F]{40}|[0-9a-fA-F]{64})", args.signing_key):
        parser.error("Pass the full OpenPGP signing-key fingerprint.")
    output = args.output.absolute()
    if output.exists() or output.is_symlink():
        parser.error("Choose a new output directory.")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="semfind-apt-", dir=output.parent) as directory:
        repo = publish_snapshot(Path(directory), args.packages, args.signing_key)
        repo.rename(output)
    print(output)


if __name__ == "__main__":
    main()
