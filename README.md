# semfind

**Semfind is an offline semantic search CLI for Linux.** Describe what you want to find to search files by content, installed apps by purpose, Linux commands by task, and desktop settings by what you want to change. Multilingual search matches by meaning and combines those matches with full-text search and exact identifiers. The model and index stay on your machine after setup. [Privacy and local data](docs/privacy.md)

A German question about how long to keep personal data finds the matching passage in an English sample document. Shortened terminal output from the [sample index](#try-it) is shown below.

```console
$ semfind "Wie lange sollen personenbezogene Daten gespeichert werden?" --category file --limit 1
[file] data-retention.md  (0.0164)
  …/examples/documents/data-retention.md:1-5
  # Data retention and lawful processing An organization should collect only the personal information it needs for a specified purpose. …
```

## Quickstart

On Ubuntu 24.04 and Pop!_OS 24.04 on amd64, install the CLI or optional desktop app from the [signed Semfind APT repository](docs/apt.md#add-the-published-source).

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

For everyday use, `semfind init` indexes supported files under your home directory, registered desktop applications, installed settings pages, local command manuals, and the built-in catalogs. The first scan can take a while. In an interactive terminal, Semfind reports progress as it scans.

```bash
semfind init
semfind "Edit screenshots" --category application --limit 3
semfind "How do I find the biggest directories?" --category command --limit 3
semfind status
```

Use `semfind index ~/Documents` to scan only a chosen folder. The [search guide](docs/search.md) covers exclusions, rescans, command examples for Debian and Arch, and filters such as `--path`, `--type`, and `--kind`.

## Desktop integrations

### Desktop app

The optional desktop app lets you search by category, preview matching passages, open files and apps, and copy Linux command examples. Use **Manage index** to add folders or switch models. After indexing, the start page offers ten clickable examples. The GUI and CLI share the local index and support the same twelve interface languages. [Language selection](docs/search.md#languages)

Semfind discovers installed settings pages on **COSMIC, GNOME, KDE Plasma 6, Cinnamon, Xfce, MATE, and LXQt**. It searches their names, descriptions, and translated keywords when available. The GUI can open discovered pages and selected built-in settings. Run `semfind index` from your desktop session to refresh the settings after installing or removing desktop components. [Desktop app guide](integrations/gui/README.md)

### Launcher adapters

Search the same local index from a desktop launcher or search provider.

| Desktop or launcher | Search interface | Setup |
| --- | --- | --- |
| COSMIC | COSMIC Launcher | [COSMIC plugin](integrations/cosmic/README.md) |
| KDE Plasma 6 | KRunner | [Plasma adapter](integrations/desktop/README.md#kde-plasma-6) |
| GNOME | GNOME Shell Activities search | [GNOME search provider](integrations/desktop/README.md#gnome-shell) |
| Cinnamon | Cinnamon menu search | [Cinnamon search provider](integrations/desktop/README.md#cinnamon) |
| Rofi | Rofi search window on Xfce, LXQt, MATE, and other desktops | [Rofi adapter](integrations/desktop/README.md#rofi-on-xfce-lxqt-mate-and-other-desktops) |

The COSMIC launcher plugin also opens supported COSMIC settings pages. See the [verification notes](docs/verification.md) for tested environments.

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

Semfind can search JPEG, PNG, and WebP images alongside documents. Florence-2 Base creates image descriptions by default once set up, processing up to eight images at a time on a supported GPU. Florence-2 Large is an optional caption model, and EmbeddingGemma 2 can embed text and images directly with one model. See the [image search guide](docs/models.md#search-images-by-subject).

## Choose an embedding model

Granite 97M is the default for new indexes. Select a model in **Manage index → Switch Model** or download it with `semfind-download-model --model NAME` after installing the Debian package. Florence-2 is selected separately under **Manage index → Image model** or with `semfind config caption-model NAME`. It generates descriptions during indexing for the active text model to embed. EmbeddingGemma 2 embeds text and images directly in the same index. [Model setup and reindexing](docs/models.md)

| Model | Download name | Role |
| --- | --- | --- |
| Granite 97M Multilingual R2 | `granite-97m` | Text · default |
| Granite 311M Multilingual R2 | `granite-311m` | Text |
| Multilingual E5 Small | `e5-small` | Text · FP32 or optional INT8 |
| Multilingual E5 Large | `e5-large` | Text |
| EmbeddingGemma 300M FP32 | `embeddinggemma-300m` | Text |
| EmbeddingGemma 300M INT8 | `embeddinggemma-300m-int8` | Text |
| GTE Multilingual Base FP32 | `gte-multilingual-base` | Text |
| Qwen3 Embedding 0.6B INT8 | `qwen3-embedding-0.6b-int8` | Text |
| EmbeddingGemma 2 | `embeddinggemma-2` | Direct text and image embeddings |
| Florence-2 Base FT | `florence-2` | Image captions · default once installed |
| Florence-2 Large FT | `florence-2-large` | Image captions |

For E5 Small INT8, add `--variant int8` to the download command. Semfind also accepts a local ONNX checkpoint with `--model-dir`. Switching embedding models requires rebuilding the vectors with `semfind reindex` or using a separate index. [Custom ONNX models](docs/models.md#use-your-own-model)

## Benchmark results

### Text retrieval

The benchmark indexed 896 Markdown documents derived from German and English Belebele passages. Cutoffs were chosen on 120 separate queries and evaluated on 200 held-out queries. Top 5 reports how often the expected document appeared in the first five results for the 160 queries with a known match. Warm p95 covers query embedding and ranking with the model already loaded, using the same 60 queries for both devices. All eight models returned results for each of the 40 queries without a known match. [Belebele dataset revision](https://huggingface.co/datasets/facebook/belebele/tree/7899cdfa4e1e0d733fd77c848e2c273cb1d32be2)

| Text model | Cutoff | Top 5 | Warm CPU p95 | Warm CUDA p95 |
| --- | ---: | ---: | ---: | ---: |
| Granite 97M | 0.80 | 88.8% | 19.0 ms | 12.4 ms |
| Granite 311M | 0.82 | 89.4% | 45.6 ms | 16.1 ms |
| E5 Small FP32 | 0.80 | 76.9% | 18.9 ms | 13.1 ms |
| E5 Large | 0.78 | 85.0% | 101.3 ms | 15.4 ms |
| EmbeddingGemma 300M FP32 | 0.30 | 93.8% | 51.6 ms | 16.8 ms |
| EmbeddingGemma 300M INT8 | 0.30 | 93.8% | 116.1 ms | 17.7 ms |
| GTE Multilingual Base FP32 | 0.58 | 91.2% | 49.1 ms | 13.9 ms |
| Qwen3 Embedding 0.6B INT8 | 0.40 | 92.5% | 627.0 ms | 21.6 ms |

In a follow-up on the same corpus and test queries, EmbeddingGemma 2 in text-only mode reached **144/160 Top-5 matches (90.0%)** at its calibrated text cutoff of 0.68. With the same timing protocol across all 200 queries, its warm p95 was **104.7 ms on CPU** and **40.0 ms on CUDA**. Granite 97M reached **142/160 (88.8%)**, with **18.6 ms** and **13.4 ms** respectively. In a separate test of 66 Semfind tasks with known matches, both models reached 63/66 in the top five. Gemma 2 reached 55/66 at rank one and Granite reached 49/66. The benchmark machine used a Ryzen 7 5800X3D and a GeForce RTX 4080 SUPER. [Earlier synthetic retrieval benchmark and runner](benchmarks/retrieval/README.md)

### Photo retrieval

The photo benchmark used 1,000 Flickr8k images and 5,000 English queries on a GeForce RTX 4080 SUPER. It ranked the expected photo by cosine similarity with no cutoff or filename matching. Gallery time excludes model startup and includes computing the image or caption embeddings. [Flickr8k dataset revision](https://huggingface.co/datasets/jxie/flickr8k/tree/56f58c967835f7c508d684f36bd7897cca9d7634)

| Image search method | Top 1 | Top 5 | Gallery time | Batch size |
| --- | ---: | ---: | ---: | ---: |
| Florence-2 Base + Granite 97M, previous prompt | 16.6% | 36.0% | 74.7 s | 8 |
| Florence-2 Base + Granite 97M, more detailed prompt (80 tokens) | 32.4% | 60.6% | 80.5 s | 8 |
| Florence-2 Large + Granite 97M, more detailed prompt (160 tokens in this test) | 39.0% | 65.4% | 182.8 s | 8 |
| EmbeddingGemma 2, direct image embeddings | 82.1% | 95.7% | 58.5 s | 1 |
| Qwen3-VL-2B-Instruct + Granite 97M, experimental captioning | 39.9% | 66.0% | 508.5 s | 8 |

A separate check of the production Florence-2 Base worker measured **16.5 s with single-image requests** and **5.3 s with batches of eight** on 65 inputs, including one undecodable file. Both runs returned identical captions for the 64 valid images. A separate OCR experiment found the expected screenshot within the first five results for **30 of 32 German queries** on synthetic Linux screenshots, up from **6 of 32** using ordinary captions. OCR and Qwen3-VL captioning were benchmark experiments.

## Architecture and limits

Semfind uses SQLite FTS5 and embeddings from local ONNX Runtime or PyTorch workers. It combines keyword and semantic rankings. Exact vector search scales linearly with the number of indexed passages. PDF search requires a text layer. See the [architecture](docs/architecture.md) and [roadmap](docs/roadmap.md).

## Contribute

Run `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, and `cargo test --locked --workspace`. See the [contributing guide](CONTRIBUTING.md) for testing with real models.

Semfind and its default Granite model are Apache-2.0 licensed. See the [license](LICENSE) and [dependency and model provenance](docs/dependencies.md).
