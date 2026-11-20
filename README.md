# semfind

An experimental, offline search engine for Linux. Use everyday language to find files in selected folders, desktop applications, COSMIC settings, and Linux commands.

Linux filename search helps when you know what a file is called. Full-text search helps when you remember its wording. semfind also compares the meaning of a query with document passages. A German question can retrieve an English document, and an exact filename, settings ID, or error code still takes priority.

## What works

- Manual indexing of selected directories and a curated, multilingual COSMIC settings catalog.
- On Linux, `semfind init` indexes your home directory, registered desktop applications, local command manuals, and the built-in catalogs.
- A multilingual Linux command catalog with runnable examples and Debian and Arch package-management variants.
- Indexing of installed manual pages and executable filenames from `PATH` through `semfind init` or `semfind index --manpages`.
- Search across UTF-8 text, Markdown, logs, CSV, reStructuredText, and PDFs with a text layer.
- Local Granite Embedding 97M Multilingual R2 embeddings, SQLite FTS5, and hybrid ranking with one result per indexed source.
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

Dependencies and the model are downloaded during setup. The default Granite FP32 model and tokenizer take about 415 MB on disk. The downloader verifies SHA-256 checksums against a fixed model revision. Subsequent indexing and search read the model from disk. Use `python scripts/download_model.py --model e5-small` to download E5 Small, optionally with `--variant int8` for compatible CPUs. Use a separate model and index directory when changing variants.

### Upgrade an existing E5 installation

Existing E5 indexes still use their original model from the previous default model directory. Explicit `--model-dir` and `SEMFIND_MODEL_DIR` settings still take priority. To switch to Granite, download it and build a fresh index in a separate directory.

```bash
python scripts/download_model.py
export SEMFIND_INDEX_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/semfind/index-granite"
semfind init
```

If you previously set `SEMFIND_MODEL_DIR` to E5, unset it for these commands or point it to the Granite download directory. Keep the new index setting for later searches. Reapply any home exclusions, or use `semfind index /path/to/folder` to index selected directories.

### Choose an embedding model

The CLI and downloader accept `granite-97m`, `granite-311m`, `e5-small`, and `e5-large` with `--model` or `SEMFIND_MODEL`. Granite 97M is the default for new indexes. Compare the models in the [retrieval benchmark](benchmarks/retrieval/README.md).

```bash
export SEMFIND_MODEL=granite-311m
export SEMFIND_INDEX_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/semfind/index-granite-311m"
python scripts/download_model.py
semfind init
semfind 'How do I find files modified today?'
```

Keep the same model and index settings for subsequent searches. Use a new index directory when changing models or weights. Each index records the exact embedding identity and refuses to combine incompatible vectors.

#### Use your own model

Set `--model-dir` or `SEMFIND_MODEL_DIR` to a local directory. Use this setting by itself in place of `--model` and `SEMFIND_MODEL`. Supply `model.onnx`, a Hugging Face `tokenizer.json` with batch padding configured, and a `semfind-model.json` describing the encoder. For an ONNX export with external data, use a single sidecar named `model.onnx_data`.

```json
{
  "pooling": "mean",
  "output": "last_hidden_state",
  "max_tokens": 512,
  "query_prefix": "query: ",
  "passage_prefix": "passage: ",
  "minimum_similarity": 0.80
}
```

`pooling` is required. Use `mean` for attention-masked mean pooling, `cls` for the first token, or `sentence` for an already pooled output of shape `[batch, dimensions]`. The other settings are optional. The default output name is `last_hidden_state`, the token limit is 512, prefixes are empty, and the similarity cutoff is disabled unless specified. The ONNX graph must run on CPU with batched INT64 inputs named `input_ids`, `attention_mask`, or `token_type_ids` and produce one vector of up to 4096 dimensions per input after pooling.

Use a separate index for your model. Changes to the weights, tokenizer, or encoding settings require reindexing. You can adjust the similarity cutoff without reindexing.

## Try it

On Linux, run `semfind init` after setup to index your home directory, desktop applications, and local command manuals. The first scan can take time on a large home directory or a system with many manuals.
Until `init` succeeds for the selected index, other commands print a setup reminder on stderr.

```bash
semfind init --exclude node_modules
semfind "Screenshots bearbeiten" --category application
semfind status
```

Run `semfind init` again to refresh the home directory, desktop applications, and local commands. Home exclusions follow the same rules as `semfind index`. If a saved root overlaps your home directory, use `semfind unindex` with that root before adopting the whole home directory. Reapply any exclusions you want to keep.

To start with a smaller index, select a directory instead.

```bash
semfind index ./examples/documents
semfind "Wie lange sollen personenbezogene Daten gespeichert werden?" --type md
semfind "Mein Bildschirm ist zu dunkel" --category settings
semfind "E-0042" --type log --json
semfind status --json
```

### Languages

Terminal help, setup reminders, and status labels are available in English, German, Russian, Portuguese, French, Spanish, and Italian. semfind uses the configured message locale. It reads `LC_ALL`, `LC_MESSAGES`, and `LANG` in that order, with `LANGUAGE` preferences for non-`C` locales. The command names, flags, and JSON fields use stable identifiers.

```bash
LC_ALL=fr_FR.UTF-8 semfind --help
semfind "Comment installer un nouveau paquet ?" --category command
```

The built-in command and COSMIC catalogs index descriptions in all seven languages. Run `semfind index` to add the translations to an existing catalog. Run `semfind init` to refresh translated desktop application metadata and local manuals when available. The five additional languages were selected from voluntarily reported desktop locales, grouped by language. [Linux Hardware Trends](https://github.com/linuxhw/Trends/blob/master/Desktop/README.md#os-lang)

### Find Linux commands

Run `semfind index` to load the built-in command and settings catalogs without scanning personal folders. Both `semfind init` and directory indexing also refresh the command catalog.

```bash
semfind index
semfind "Welche Ordner belegen am meisten Speicherplatz?" --category command
semfind "Wie installiere ich ein neues Paket?" --category command --distro debian
semfind "Wie installiere ich ein neues Paket?" --category command --distro arch --json
```

The CLI looks up the current distribution family in `os-release`. Debian, Ubuntu, Pop!_OS, and distributions with compatible IDs get Debian examples. Arch and its recognized derivatives get Arch examples. Other systems get generic Linux examples. Use `--distro debian`, `--distro arch`, or `--distro generic` to override the detection. Generic commands are included for both families. Examples are displayed as text, and command results include `distro` and `example` in JSON output. See the [APT manual](https://manpages.debian.org/stable/apt/apt.8.en.html) and the [pacman manual](https://man.archlinux.org/man/pacman.8.en) for package management syntax.

On Linux, `semfind init` includes the commands available on your machine. To refresh the command catalog on its own, run `semfind index --manpages`.

```bash
semfind index --manpages
semfind "Wie prüfe ich, ob zwei Dateien Byte für Byte gleich sind?" --category command
```

This scans manual sections 1, 6, and 8 from the configured manual path. It indexes the name, synopsis, and a short description where available. Install `man-db` for your system's configured `manpath` and for opening results with `man`. Compressed pages use the installed `gzip`, `xz`, `bzip2`, or `zstd` helper. Executable files from absolute `PATH` directories without a manual entry are searchable by name. Shell aliases, functions, and built-ins need an installed manual page to appear.

The first scan can take a minute or more on a large system. Run `semfind index --manpages` again after installing or removing packages. When you select another distribution, search uses the built-in catalog for that distribution. See the [manpath manual](https://manpages.debian.org/stable/man-db/manpath.1.en.html).

### Filter and manage indexed files

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

Passing exclusions replaces the saved list for that root. Use `--clear-excludes` to clear it. Quote multiword queries. Use `semfind search "status"` to search for a semfind subcommand name. Search outputs a JSON array with `--json`, including an empty array for zero results. Exit codes are `0` for success (including zero hits), `1` for an operational error, and `2` for invalid CLI usage. Diagnostics go to stderr.

Local data defaults to `${XDG_DATA_HOME:-$HOME/.local/share}/semfind`. Set `SEMFIND_MODEL_DIR` and `SEMFIND_INDEX_DIR` to use other locations. The index directory must be private on Linux.

## Architecture and limits

The Rust CLI calls the same library that future clients will use. The core stages reads, extracts text, and stores passages and vectors in a single SQLite transaction. A local CPU worker runs the embedding model through ONNX Runtime. Query-time cosine similarity and FTS5's BM25 rankings are fused with Reciprocal Rank Fusion. See [architecture](docs/architecture.md) and [privacy](docs/privacy.md).

The MVP is intended for small collections. Exact cosine search scales linearly with the number of passages. Semantic hits use a model-specific cosine similarity cutoff. Literal and full-text matching still apply at any similarity. Scanned PDFs require a text layer. Settings results carry catalog metadata. A desktop UI, launching apps from results, a background service, and file history are roadmap items. See [roadmap](docs/roadmap.md) and [COSMIC research](integrations/cosmic/README.md).

## Contribute

Run `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, and `cargo test --locked --workspace`. See [contributing](CONTRIBUTING.md) for the real-model test and review guidelines.

Code is [Apache-2.0 licensed](LICENSE). The [default Granite model](https://huggingface.co/ibm-granite/granite-embedding-97m-multilingual-r2) is also Apache-2.0 licensed and downloaded separately. See [dependencies and model provenance](docs/dependencies.md).
