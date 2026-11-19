# semfind

An experimental, offline search engine for Linux. Search selected folders and a small catalog of COSMIC settings using everyday German or English.

Linux filename search helps when you know what a file is called. Full-text search helps when you remember its wording. semfind also compares the meaning of a query with document passages. A German question can retrieve an English document, and an exact filename, settings ID, or error code still takes priority.

## What works

- Manual indexing of selected directories and a curated, bilingual COSMIC settings catalog.
- Search across UTF-8 text, Markdown, logs, CSV, reStructuredText, and PDFs with a text layer.
- Local multilingual E5 embeddings, SQLite FTS5, and hybrid ranking with one result per file or setting.
- Filters for folder, file extension, file kind (text, image, video), and result category. JSON for scripting.
- Incremental rescans that recalculate embeddings only for changed content. Saved directory exclusions and manual index removal.

The target desktop is Pop!_OS 24.04 with COSMIC. The CLI works independently of the desktop. See [verification](docs/verification.md) for the environments exercised.

## Install on Pop!_OS 24.04

With this source directory open in a terminal, install Rust 1.93 or newer through [rustup](https://rustup.rs/) and run the following with the system Python 3.12. System package installation uses sudo. Build, indexing and search run as your regular user.

```bash
sudo apt install build-essential python3-venv poppler-utils
python3 -m venv .venv
source .venv/bin/activate
python -m pip install -r requirements.txt
python scripts/download_model.py
cargo install --locked --path crates/semfind-cli
export PATH="$HOME/.cargo/bin:$PATH"
export SEMFIND_PYTHON="$VIRTUAL_ENV/bin/python"
```

Dependencies and the model are downloaded during setup. The default FP32 model and tokenizer take about 487 MB on disk. The downloader verifies SHA-256 checksums against a fixed model revision. Subsequent indexing and search read the model from disk. For a smaller download, the upstream INT8 export is available with `--variant int8` for compatible CPUs. Use a separate model and index directory when changing variants.

## Try it

```bash
semfind index ./examples/documents
semfind "Wie lange sollen personenbezogene Daten gespeichert werden?" --type md
semfind "Mein Bildschirm ist zu dunkel" --category settings
semfind "E-0042" --type log --json
semfind status --json
```

```bash
semfind "Sommerurlaub" --kind image
semfind --kind video --path "$HOME/Videos"
semfind "Datenschutz" --kind text --type pdf
```

`--kind` groups file extensions and can list indexed files without a query. Image and video searches use filenames and paths. Listing indexed files or filtering a search to image and video formats works without loading the model. Rescan your indexed folders to include images and videos.

```bash
semfind index "$HOME/Documents" --exclude Private --exclude Archive/taxes
semfind "Probleme mit meinem Vermieter" --type pdf --path "$HOME/Documents"
semfind index "$HOME/Documents"  # rescan with the saved exclusions
semfind unindex "$HOME/Documents"
```

Passing exclusions replaces the saved list for that root. Use `--clear-excludes` to clear it, or `semfind index` to initialize only the settings catalog. Quote multiword queries. Use `semfind search "status"` to search for a command name. Search outputs a JSON array with `--json`, including an empty array for zero results. Exit codes are `0` for success (including zero hits), `1` for an operational error, and `2` for invalid CLI usage. Diagnostics go to stderr.

Local data defaults to `${XDG_DATA_HOME:-$HOME/.local/share}/semfind`. Set `SEMFIND_MODEL_DIR` and `SEMFIND_INDEX_DIR` to use other locations. The index directory must be private on Linux.

## Architecture and limits

The Rust CLI calls the same library that future clients will use. The core stages reads, extracts text, and stores passages and vectors in a single SQLite transaction. A local CPU worker runs the E5 model through ONNX Runtime. Query-time cosine similarity and FTS5's BM25 rankings are fused with Reciprocal Rank Fusion. See [architecture](docs/architecture.md) and [privacy](docs/privacy.md).

The MVP is intended for small collections. Exact cosine search scales linearly with the number of passages. The local E5 model uses a minimum cosine similarity for semantic hits. Literal and full-text matching still apply at any similarity. Scanned PDFs require a text layer. Settings results carry catalog metadata. A desktop UI, installed application discovery, activation, a background service, and file history are roadmap items. See [roadmap](docs/roadmap.md) and [COSMIC research](integrations/cosmic/README.md).

## Contribute

Run `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, and `cargo test --locked --workspace`. See [contributing](CONTRIBUTING.md) for the real-model test and review guidelines.

Code is [Apache-2.0 licensed](LICENSE). The [upstream model](https://huggingface.co/intfloat/multilingual-e5-small) is MIT-licensed and downloaded separately. See [dependencies and model provenance](docs/dependencies.md).
