# semfind

**Semfind is an offline semantic search CLI for Linux.** Describe what you want to find in everyday language. Semfind searches file contents, installed applications, COSMIC settings, and Linux commands. Use `--json` for structured results in scripts, applications, and AI agents.

Semfind uses local multilingual embeddings to match queries by meaning. A German question can retrieve an English document. It combines semantic similarity with full-text search and gives priority to exact filenames, settings IDs, and error codes.

## What works

- Manual indexing of selected directories and a curated, multilingual COSMIC settings catalog.
- On Linux, `semfind init` indexes your home directory, registered desktop applications, local command manuals, and the built-in catalogs.
- A multilingual Linux command catalog with runnable examples and Debian and Arch package-management variants.
- Indexing of installed manual pages and executable filenames from `PATH` through `semfind init` or `semfind index --manpages`.
- Search across UTF-8 text, Markdown, logs, CSV, reStructuredText, and PDFs with a text layer.
- Local Granite Embedding 97M Multilingual R2 embeddings, SQLite FTS5, and hybrid ranking with one result per indexed source.
- Filters for folder, file extension, file kind (text, image, video), and result category. JSON for scripting.
- With Florence-2 set up, `semfind init` makes JPEG, PNG, and WebP images searchable by subject alongside documents.
- Incremental rescans that recalculate embeddings only for changed content. Saved directory exclusions and manual index removal.
- An optional COSMIC Launcher plugin searches the same index. [Installation and usage](integrations/cosmic/README.md)
- Optional KDE Plasma 6, GNOME Shell, Cinnamon, and Rofi adapters use the same index. [Installation and usage](integrations/desktop/README.md)

The CLI works independently of the desktop environment. Pop!_OS 24.04 is one of the [tested environments](docs/verification.md). See the [roadmap](docs/roadmap.md) for planned desktop integrations and other work.

## Use semfind from programs and AI agents

Call semfind as a subprocess to find local sources for a natural-language question. `--category`, `--path`, and `--limit` narrow the results before the caller opens a file or consults a manual. The index and model run locally after setup.

```bash
semfind "How do I show free disk space?" --category command --limit 3 --json
```

This query can return `df` with the example `df -h`. For file results, the JSON includes the source path and a matching text excerpt that a program or agent can use to inspect the original file. Use `semfind status --json` to check which directories are indexed.

File results can also include a `location` for the passage behind the snippet. Text files use one-based, inclusive line numbers such as `{"line_start":12,"line_end":17}`. PDFs use a one-based physical page number such as `{"page":3}`. The terminal shows these as `notes.md:12-17` or `report.pdf#page=3`. The JSON field is optional.

Existing indexes remain searchable after Semfind 0.2.1 automatically upgrades them to schema 5. Use 0.2.1 or newer with an upgraded index. Run `semfind index /path/to/folder` for each indexed folder, or `semfind init` for a home index, to add locations. The first scan after the upgrade reprocesses previously indexed text files and PDFs. Locations refer to the indexed text and refresh on rescan.

With `--json`, search writes a JSON array to stdout, including `[]` for zero results. Diagnostics go to stderr. Exit codes are `0` for success (including zero hits), `1` for an operational error, and `2` for invalid CLI usage.

Add `--time` to report the search duration in milliseconds on stderr. Measurement starts after the index is opened and includes model startup when needed, retrieval, and ranking. Result formatting is outside the measurement.

On Linux, standalone searches in automatic mode can reuse the last query embedding from the selected index directory. Semfind still ranks the current index and checks that file hits are accessible. `--time` reports a cache hit, and `--no-cache` skips reading and saving the cached embedding for one invocation.

When a standalone search needs a new embedding, Semfind on Linux starts a local background worker on demand. By default, the worker keeps the selected model loaded for 15 minutes after the last embedding. It uses automatic CUDA, MIGraphX, and CPU selection. `--time` shows `ONNX Runtime (resident)` when a search uses an already loaded model.

Save a five-minute timeout for future searches, show the saved value, or restore the 15-minute default.

```bash
semfind config daemon-idle-seconds 300
semfind config daemon-idle-seconds
semfind config daemon-idle-seconds --reset
```

Semfind stores this setting in `$XDG_CONFIG_HOME/semfind/daemon-idle-seconds` (by default `~/.config/semfind/daemon-idle-seconds`). The value survives a restart. The `--daemon-idle-seconds` flag takes precedence over `SEMFIND_DAEMON_IDLE_SECONDS`, which takes precedence over the saved value. The worker uses the setting when it computes the next embedding. A cached query leaves the current timer running. Use `--no-cache` to compute a new embedding for a repeated query.

Use `--no-daemon` or `SEMFIND_NO_DAEMON=true` to run inference directly. Semfind also runs inference directly when you select CPU, CUDA, or MIGraphX explicitly, pass `--python`, or when the worker is unavailable. For cold-start comparisons, combine `--no-daemon` with `--no-cache`.

```bash
semfind "How do I show free disk space?" --category command --json --time
```

### Run multiple searches in one process

```bash
printf '%s\n' 'How do I show free disk space?' 'How do I find files by name?' |
  semfind search --stdin --category command --limit 3 --time
```

`--stdin` reads one plain UTF-8 query per line and ignores blank lines. It writes and flushes one JSON result array for each query in input order, including `[]` for zero hits. Applications can send another query after reading the previous result while keeping stdin open. All queries share the same filters. The process opens the index once and reuses the model worker after the first query that requires embeddings.

`--time` reports each query's duration on stderr. Closing stdin ends the session. If a query or input line fails, semfind reports its line number on stderr and exits with code `1`. Results already emitted remain valid.

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

`cargo install` installs the `semfind` command in `$HOME/.cargo/bin` by default. For later terminal sessions, keep that directory on `PATH` and set `SEMFIND_PYTHON` to the absolute path of the virtual environment's Python interpreter.

### Choose an embedding model

The CLI and downloader accept `granite-97m`, `granite-311m`, `e5-small`, and `e5-large` with `--model` or `SEMFIND_MODEL`. Granite 97M is the default for new indexes. Compare the models in the [retrieval benchmark](benchmarks/retrieval/README.md).

```bash
export SEMFIND_MODEL=granite-311m
export SEMFIND_INDEX_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/semfind/index-granite-311m"
python scripts/download_model.py
semfind init
semfind 'How do I find files modified today?'
```

Keep the same model and index settings for subsequent searches. To change models or weights, use a new index directory or rebuild the current index with `semfind init --force`. Each index records the exact embedding identity and refuses to combine incompatible vectors.

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

Changes to the weights, tokenizer, or encoding settings require a full rebuild or a separate index. You can adjust the similarity cutoff without reindexing.

### Run inference on an NVIDIA GPU

For automatic CUDA use, create a dedicated Python environment under the Semfind data directory. Check that `nvidia-smi` can see the GPU. The PyPI build below requires a driver that supports CUDA 13 and installs the CUDA and cuDNN runtime libraries through pip. [ONNX Runtime CUDA requirements](https://onnxruntime.ai/docs/execution-providers/CUDA-ExecutionProvider.html#requirements)

```bash
cuda_runtime="${XDG_DATA_HOME:-$HOME/.local/share}/semfind/runtime/cuda"
python3 -m venv "$cuda_runtime"
"$cuda_runtime/bin/python" -m pip install 'onnxruntime-gpu[cuda,cudnn]==1.32.0' 'numpy==2.5.5' 'tokenizers==0.23.3'
semfind init --time
```

With the default `--device auto`, search and indexing first try dedicated text embedding runtimes in `runtime/cuda` and `runtime/migraphx` under the Semfind data directory, in that order. You can symlink either directory to an existing virtual environment. If neither dedicated GPU starts, Semfind uses its configured Python environment and tries CUDA, MIGraphX, then CPU as available. A GPU inference error in automatic mode retries on the CPU. `--time` reports the provider selected when the embedding model starts. Florence-2 uses [separate PyTorch runtimes for captioning](#search-images-by-subject).

When a search or indexing command uses the CPU for text embeddings, Semfind prints a performance tip with CUDA and MIGraphX setup links on stderr once per invocation. Use `--no-gpu-hint` to suppress it for one invocation, or set `SEMFIND_NO_GPU_HINT=true` for your environment.

Use `--python` to select a specific interpreter for one invocation. `SEMFIND_PYTHON` sets the default interpreter, including the fallback in automatic mode. Use `--device cpu` to force CPU inference or `--device cuda` to require CUDA in the selected interpreter. You can also set `SEMFIND_DEVICE`. The same model and weights can reuse an index on another backend. Tokenization, filesystem scanning, SQLite, and search ranking run on the CPU. On the tested RTX 4080 SUPER, CUDA sped up index building and searches with a running model worker. A cold CUDA session took longer to start than a CPU session. Automatic searches can now reuse a running session across invocations. [CPU and CUDA benchmark](benchmarks/retrieval/README.md#granite-97m-cpu-and-cuda-results)

### AMD GPUs

Semfind supports AMD MIGraphX experimentally. Install ROCm for your GPU and Linux distribution. Follow AMD's guide to install its MIGraphX-enabled ONNX Runtime in a separate Python environment. Use `${XDG_DATA_HOME:-$HOME/.local/share}/semfind/runtime/migraphx` for automatic discovery. Install NumPy and Tokenizers in that environment. [AMD ONNX Runtime setup and compatibility](https://rocm.docs.amd.com/projects/ai-ecosystem/en/latest/inference/onnxruntime.html)

```bash
amd_runtime="${XDG_DATA_HOME:-$HOME/.local/share}/semfind/runtime/migraphx"
"$amd_runtime/bin/python" -m pip install 'numpy==2.5.5' 'tokenizers==0.23.3'
"$amd_runtime/bin/python" -c 'import onnxruntime as ort; print(ort.get_available_providers())'
semfind init --time
```

The provider list should include `MIGraphXExecutionProvider`. The explicit `--device migraphx` setting reports an error if the provider cannot start or inference fails. ONNX Runtime can still place individual graph operations on the CPU. With the default `--device auto`, Semfind falls back to the CPU if GPU inference fails. Granite 97M execution and performance on AMD hardware still need verification. [ONNX Runtime MIGraphX provider](https://onnxruntime.ai/docs/execution-providers/MIGraphX-ExecutionProvider.html)

## Try it

On Linux, run `semfind init` after setup to index your home directory, desktop applications, and local command manuals. The first scan can take time on a large home directory or a system with many manuals.
In an interactive terminal, `init` shows the current phase, elapsed time, and the number of supported files found so far. The final summary reports how many entries were written, unchanged, removed, or skipped.
Use `semfind init --time` to report the total duration and the time spent in each phase on stderr. Combine it with `--force` to measure a complete rebuild. Timing covers model preparation through saving the index, in seconds.
Status, indexing, and search commands print a setup reminder on stderr until `init` succeeds for the selected index.

```bash
semfind init --exclude node_modules
semfind "Screenshots bearbeiten" --category application
semfind "Ein Programm zum Chatten"
semfind status
```

Run `semfind init` again to refresh the home directory, desktop applications, and local commands. Home exclusions follow the same rules as `semfind index`. If a saved root overlaps your home directory, use `semfind unindex` with that root before adopting the whole home directory. Reapply any exclusions you want to keep.

Queries that explicitly ask for an application, such as `Ein Programm zum Chatten` or `An app for chatting`, search installed applications by the task described. Exact names and explicit filters take precedence.

Run `semfind init --force` to rebuild the selected index from scratch. It rescans the home directory and all registered directories with their saved exclusions. It also rebuilds the desktop application, settings, and command catalogs. All registered directories must be accessible. Semfind writes the rebuilt index in one transaction and rolls back if the rebuild fails. The final report counts the replaced entries as removed and the newly indexed entries as written.

During a forced rebuild, configured Florence-2 captioning applies to all scanned directories. With `--no-caption-images` or without a local Florence-2 model, the rebuilt index stores image filenames and paths. To switch the embedding model in place, select the new model when running `init --force` and use the same selection for searches.

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

`--kind` groups file extensions and can list indexed files without a query. Images are searchable by filename and path, and captioned images are also searchable by subject. Videos are searchable by filename and path. Listing files and searching filenames in an index with no matching embeddings work without loading the model.

```bash
semfind index "$HOME/Documents" --exclude Private --exclude Archive/taxes
semfind "Probleme mit meinem Vermieter" --type pdf --path "$HOME/Documents"
semfind index "$HOME/Documents"  # rescan with the saved exclusions
semfind unindex "$HOME/Documents"
```

Passing exclusions replaces the saved list for that root. Use `--clear-excludes` to clear it. Quote multiword queries. Use `semfind search "status"` to search for a semfind subcommand name.

Local data defaults to `${XDG_DATA_HOME:-$HOME/.local/share}/semfind`. Set `SEMFIND_MODEL_DIR` and `SEMFIND_INDEX_DIR` to use other locations. The index directory must be private on Linux.

### Search images by subject

Once Florence-2 is set up locally, `semfind init` automatically generates English descriptions of supported JPEG, PNG, and WebP files in your home directory. Semfind indexes the captions as text alongside document passages. The same search can return both, while `--kind image` limits it to pictures. Semfind includes the generated description in the result snippet.

From the repository root, add the optional dependencies to the Python environment selected during setup. The [Florence-2 Base FT checkpoint](https://huggingface.co/florence-community/Florence-2-base-ft) download is about 470 MB.

```bash
"$SEMFIND_PYTHON" -m pip install 'torch==2.14.1' 'torchvision==0.29.1' --index-url https://download.pytorch.org/whl/cpu
"$SEMFIND_PYTHON" -m pip install 'transformers==5.23.0' 'Pillow==13.0.0'
"$SEMFIND_PYTHON" scripts/download_model.py --model florence-2
semfind init
semfind "Katze auf dem Sofa" --json
semfind "Katze auf dem Sofa" --kind image --json
```

Florence-2 runs locally and automatically uses a compatible PyTorch GPU when available. Semfind tries dedicated runtimes in `runtime/florence-cuda` and `runtime/florence-rocm` under the Semfind data directory, then checks the configured Python environment. If GPU captioning fails, Semfind retries on the CPU. `--device cpu` forces CPU captioning, and `--python` uses the chosen interpreter for device detection. `semfind init --time` reports the Florence-2 backend separately when it generates captions.

For an NVIDIA GPU with a CUDA 13-capable driver, create a dedicated Florence-2 runtime:

```bash
florence_runtime="${XDG_DATA_HOME:-$HOME/.local/share}/semfind/runtime/florence-cuda"
python3 -m venv "$florence_runtime"
"$florence_runtime/bin/python" -m pip install 'torch==2.14.1' 'torchvision==0.29.1' --index-url https://download.pytorch.org/whl/cu130
"$florence_runtime/bin/python" -m pip install 'transformers==5.23.0' 'Pillow==13.0.0'
```

For AMD GPUs, use a [PyTorch ROCm build](https://pytorch.org/get-started/locally/) in `runtime/florence-rocm` and install the same Transformers and Pillow dependencies. The Florence-2 checkpoint is shared between backends.

In a local check with Florence-2 Base FT, the median time to describe one repeated test image after warmup was 3.01 s on a Ryzen 7 5800X3D and 0.15 s on a GeForce RTX 4080 SUPER with CUDA. Both backends produced the same description for that image. The timing excludes model startup and index updates. ROCm still needs hardware verification.

The first run can take longer for large photo collections. Subsequent scans generate captions for new or changed images, or when the caption model changes. If Florence-2 is unavailable during `init`, Semfind keeps the captions it already has and indexes new images by filename and path. Use `semfind init --no-caption-images` to skip captioning for one run.

Use `semfind index ROOT --caption-images` to explicitly caption images in a separate indexed root. Use `SEMFIND_CAPTION_MODEL_DIR` or `--caption-model-dir` for another local checkpoint directory. Deleting an image or unindexing its root removes its caption. Unreadable image contents fall back to filename and path search when the file itself remains accessible. Caption extraction limits images to 16 MiB and 40 million pixels.

## Architecture and limits

The Rust CLI calls the same library that future clients will use. The core stages reads, extracts text, and stores passages and vectors in a single SQLite transaction. A local worker runs the embedding model through ONNX Runtime on the CPU or with NVIDIA CUDA or AMD MIGraphX. Query-time cosine similarity and FTS5's BM25 rankings are fused with Reciprocal Rank Fusion. See [architecture](docs/architecture.md) and [privacy](docs/privacy.md).

Semfind is intended for small collections. Exact cosine search scales linearly with the number of passages. Semantic hits use a model-specific cosine similarity cutoff. Literal and full-text matching still apply at any similarity. Scanned PDFs require a text layer. Settings results carry catalog metadata.

## Contribute

Run `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, and `cargo test --locked --workspace`. See [contributing](CONTRIBUTING.md) for the real-model test and review guidelines.

Code is [Apache-2.0 licensed](LICENSE). The [default Granite model](https://huggingface.co/ibm-granite/granite-embedding-97m-multilingual-r2) is also Apache-2.0 licensed and downloaded separately. See [dependencies and model provenance](docs/dependencies.md).
