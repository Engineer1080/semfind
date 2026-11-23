# Install Semfind with APT

The Debian packages target **Ubuntu 24.04 and Pop!_OS 24.04 on amd64**. `semfind` provides the CLI, a private CPU inference environment, and a model downloader. The optional `semfind-gui` package provides the [GTK desktop app](../integrations/gui/README.md) and depends on `semfind`. APT installs the required system Python runtime and GTK libraries as dependencies of their respective packages.

## Build and try the package

On Ubuntu or Pop!_OS 24.04, install the build prerequisites and make Rust 1.93 or newer available on your PATH. Run the build from the repository root as your regular user.

```bash
sudo apt install build-essential dpkg-dev python3.12-venv
python3.12 -m venv target/deb-venv
target/deb-venv/bin/python packaging/deb/build.py
```

The builder downloads the Python wheels listed in `requirements.txt` and their dependencies. You can pass `--wheel-dir PATH` to use a prepared wheel directory. The package includes the Python libraries and their wheel license files. The model weights are downloaded separately.

Install the resulting CLI package using APT. Substitute your built version in the filename if it differs from the example.

```bash
sudo apt install ./target/deb/semfind_0.3.3-1_amd64.deb
semfind-download-model
semfind init
semfind "How do I show free disk space?" --category command
```

To try the desktop app before the APT repository is published, give APT both local packages so it can resolve the CLI dependency.

```bash
sudo apt install ./target/deb/semfind_0.3.3-1_amd64.deb ./target/deb/semfind-gui_0.3.3-1_all.deb
```

Open Semfind from the application menu, or run `semfind-gui`. Use **Manage index** to download the model and choose a folder to index. Run the Semfind commands as your regular user. The model and index use the existing Semfind data directory under `$XDG_DATA_HOME` (usually `~/.local/share/semfind`). Installing, upgrading, or removing the packages leaves that directory intact.

If you already installed Semfind from source, run `type -a semfind` to see which executable your shell uses. Use `/usr/bin/semfind` to select the APT version. Keep the same model and index settings to reuse your source installation's index.

## Prepare a signed repository

The generator builds a complete snapshot with package checksums, an AppStream catalog, a signed `InRelease`, and the public archive key. Install `apt-utils`, `appstream`, and `gpg` on the publishing machine. Use the fingerprint of a project-controlled signing key available in your GnuPG keyring.

```bash
python3.12 packaging/deb/repository.py \
  --output target/apt-noble \
  --signing-key YOUR_SIGNING_KEY_FINGERPRINT \
  target/deb/semfind_0.3.3-1_amd64.deb \
  target/deb/semfind-gui_0.3.3-1_all.deb
```

Publish the generated directory as a unit at a stable HTTPS URL. Keep the private signing key outside the published directory. For a later release, build a new snapshot and supply the new package together with any older packages you still want to serve. Keep published packages immutable by releasing any change under a new version.

## Add the published source

Once a Semfind repository has been published, download its public archive key and confirm its fingerprint against the one published by the maintainer. Install the verified key in APT's local keyring directory.

```bash
sudo install -d -m 0755 /etc/apt/keyrings
sudo install -m 0644 semfind-archive-keyring.gpg /etc/apt/keyrings/semfind.gpg
```

Save the following as `/etc/apt/sources.list.d/semfind.sources`, changing the example URI to the published repository URL.

```deb822
Types: deb
URIs: https://packages.example.org/semfind
Suites: noble
Components: main
Architectures: amd64
Signed-By: /etc/apt/keyrings/semfind.gpg
```

```bash
sudo apt update
sudo apt install semfind
```

After a new package is published, `sudo apt update` fetches its metadata and `sudo apt upgrade` can install it. The archive key is scoped to this source through `Signed-By`.

Use `sudo apt install semfind-gui` for the desktop app. The AppStream catalog points to `semfind-gui`, so installing Semfind from a compatible store also pulls in the CLI. COSMIC Store discovery still needs a graphical test with the published source. Ubuntu 24.04 App Center lists Debian packages from Ubuntu's official repositories. An official store listing there requires admission to the Ubuntu archive. [System76 package documentation](https://system76.com/support/managing-apps) · [Ubuntu App Center documentation](https://ubuntu.com/desktop/docs/en/24.04/explanation/snap-and-deb-packages/)
