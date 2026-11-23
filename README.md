# semfind

**Semfind is an offline semantic search CLI for Linux.** Describe what you want to find to search files by content, installed apps by purpose, and Linux commands by task. Multilingual search matches by meaning and combines those matches with full-text search and exact identifiers. The model and index stay on your machine after setup. [Privacy and local data](docs/privacy.md)

A German question about how long to keep personal data finds the matching passage in an English sample document. Shortened terminal output from the [sample index](#try-it) is shown below.

```console
$ semfind "Wie lange sollen personenbezogene Daten gespeichert werden?" --category file --limit 1
[file] data-retention.md  (0.0164)
  …/examples/documents/data-retention.md:1-5
  # Data retention and lawful processing An organization should collect only the personal information it needs for a specified purpose. …
```

## Quickstart

For Ubuntu 24.04 and Pop!_OS 24.04 on amd64, see the [Debian package and APT guide](docs/apt.md) to build and try the CLI and optional desktop app.

<a name="install-on-pop_os-2404"></a>

### Install from source on Linux

Install the system packages for your distribution. The build needs a C toolchain, and PDF search uses `pdftotext`.

| Distribution | System packages |
| --- | --- |
| Ubuntu 24.04 and Pop!_OS 24.04 | `sudo apt install git build-essential python3.12-venv poppler-utils` |
| Debian | `sudo apt install git build-essential poppler-utils` |
| Fedora | `sudo dnf install git gcc glibc-devel poppler-utils` |
| Arch Linux (up to date) | `sudo pacman -S --needed git base-devel poppler` |

For other distributions, install Git, a C compiler and linker, and `pdftotext` through your package manager. The steps below use Rust 1.93 or newer and Python 3.12. Install Rust through [rustup](https://rustup.rs/) if needed. Run the build and Python setup as your regular user.

If Python 3.12 with `venv` is unavailable, install [uv](https://docs.astral.sh/uv/getting-started/installation/) and run `uv venv --python 3.12 --seed .venv` in place of the `python3.12 -m venv .venv` line. By default, `uv` downloads Python 3.12 if needed and creates the environment without changing your system Python.

Ubuntu 24.04 is the CI environment and Pop!_OS 24.04 is the locally tested Linux reference. The package commands for Debian, Fedora, and Arch are provided as starting points. See the [verification notes](docs/verification.md) for what has been tested.

```bash
git clone https://github.com/Engineer1080/semfind.git
cd semfind
python3.12 -m venv .venv
.venv/bin/python -m pip install -r requirements.txt
.venv/bin/python scripts/download_model.py
cargo install --locked --path crates/semfind-cli
export PATH="$HOME/.cargo/bin:$PATH"
export SEMFIND_PYTHON="$PWD/.venv/bin/python"
```

Setup downloads the dependencies and the default Granite 97M Multilingual R2 model. The model and tokenizer take about 415 MB on disk. For new terminals, keep `$HOME/.cargo/bin` on your `PATH` and set `SEMFIND_PYTHON` to the absolute path of the virtual environment's Python interpreter.

### Try it

From the repository root, index the included [sample documents](examples/documents) and run the search above. The example uses a separate index under `target/` and scans only the sample directory for files. You may see a reminder to run `semfind init` when using the sample index.

```bash
semfind --index-dir ./target/semfind-demo-index index ./examples/documents
semfind --index-dir ./target/semfind-demo-index "Wie lange sollen personenbezogene Daten gespeichert werden?" --category file --limit 1
```

For everyday use, `semfind init` indexes supported files under your home directory, registered desktop applications, local command manuals, and the built-in catalogs. The first scan can take a while. In an interactive terminal, Semfind reports progress as it scans.

```bash
semfind init
semfind "Edit screenshots" --category application --limit 3
semfind "How do I find the biggest directories?" --category command --limit 3
semfind status
```

Use `semfind index ~/Documents` to scan only a chosen folder. The [search guide](docs/search.md) covers exclusions, rescans, command examples for Debian and Arch, and filters such as `--path`, `--type`, and `--kind`.

## Desktop integrations

The optional [Semfind desktop app](integrations/gui/README.md) provides a search window with category filters, file excerpts, and index management. The app and launcher integrations all use the same local index as the CLI.

| Desktop or launcher | Search interface | Setup |
| --- | --- | --- |
| COSMIC | COSMIC Launcher | [COSMIC plugin](integrations/cosmic/README.md) |
| KDE Plasma 6 | KRunner | [Plasma adapter](integrations/desktop/README.md#kde-plasma-6) |
| GNOME | GNOME Shell Activities search | [GNOME search provider](integrations/desktop/README.md#gnome-shell) |
| Cinnamon | Cinnamon menu search | [Cinnamon search provider](integrations/desktop/README.md#cinnamon) |
| Rofi | Rofi search window on Xfce, LXQt, MATE, and other desktops | [Rofi adapter](integrations/desktop/README.md#rofi-on-xfce-lxqt-mate-and-other-desktops) |

Semfind includes a built-in COSMIC settings catalog. The COSMIC plugin can open supported settings pages. See the [verification notes](docs/verification.md) for tested environments.

## Use Semfind from programs and AI agents

Use `--json` to get a result array on stdout. File hits include paths and text excerpts, with line or page references when available. Command hits can include examples such as `df -h`. Semfind writes diagnostics to stderr.

```bash
semfind "How do I show free disk space?" --category command --limit 1 --json
```

The [scripting guide](docs/scripting.md) covers exit codes, batch queries with `--stdin`, timing with `--time`, and the model worker's configurable idle timeout.

## Optional setup

### Run inference on an NVIDIA GPU

Semfind automatically tries available GPU runtimes and falls back to the CPU. For NVIDIA prerequisites and the `semfind setup-gpu cuda` command, see the [CUDA setup guide](docs/gpu.md#run-inference-on-an-nvidia-gpu).

### AMD GPUs

Experimental MIGraphX support requires compatible system libraries. See the [AMD setup guide](docs/gpu.md#amd-gpus) for prerequisites and the `semfind setup-gpu migraphx` command.

### Search images by subject

With Florence-2 set up locally, Semfind can search supported JPEG, PNG, and WebP images by their generated descriptions alongside documents. See the [image search guide](docs/models.md#search-images-by-subject).

### Choose an embedding model

Granite 97M is the default for new indexes. See the [model guide](docs/models.md#choose-an-embedding-model) for alternative downloads and custom ONNX models, and the [retrieval benchmarks](benchmarks/retrieval/README.md) for comparisons.

## Architecture and limits

Semfind uses SQLite FTS5 and embeddings from a local ONNX Runtime worker. It combines keyword and semantic rankings. Exact vector search scales linearly with the number of indexed passages. PDF search requires a text layer. See the [architecture](docs/architecture.md) and [roadmap](docs/roadmap.md).

## Contribute

Run `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, and `cargo test --locked --workspace`. See the [contributing guide](CONTRIBUTING.md) for testing with real models.

Semfind and its default Granite model are Apache-2.0 licensed. See the [license](LICENSE) and [dependency and model provenance](docs/dependencies.md).
