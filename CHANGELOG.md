# Changelog

## 0.4.4

- Add an Open button for supported setting results that opens the matching panel on COSMIC, GNOME, KDE Plasma 6, Cinnamon, Xfce, MATE, and LXQt when the appropriate tool is installed.
- List all built-in model downloads and summarize recent text and photo retrieval benchmarks in the README.

## 0.4.3

- Show ten localized, clickable search examples for files, applications, Linux commands, and settings in the GTK app. Each example fills the query, selects its category, and starts searching.

## 0.4.2

- Add Simplified Chinese, Japanese, Dutch, Polish, and Turkish to terminal help, setup hints, status messages, and the built-in command and COSMIC settings catalogs.
- Localize the GTK desktop app in all twelve supported languages. Use the Linux message locale for search, result labels, model selection, dialogs, and index management. Include the translations and desktop menu comments in the Debian package.

## 0.4.1

- Show the Semfind logo and the active CLI version in the desktop app header.
- Generate Florence-2 captions on compatible GPUs in batches of up to eight images. Retry smaller batches when GPU memory is tight and fall back to single-image CPU processing if necessary.

## 0.4.0

- Keep Granite 97M as the default text encoder. Add **EmbeddingGemma 300M FP32 and INT8**, **GTE Multilingual Base FP32**, and **Qwen3 Embedding 0.6B INT8** as verified ONNX downloads and choices in the CLI and desktop app.
- Use calibrated semantic similarity cutoffs for the built-in ONNX models. Save or reset a cutoff per model with `semfind config minimum-similarity`, or override it for a search with `--minimum-similarity`.
- Use the more detailed Florence-2 Base FT prompt by default for image captions. The next caption-enabled scan refreshes existing captions.
- Offer Florence-2 Large FT through **Manage index → Image model** or `semfind config caption-model florence-2-large`. Use `semfind reindex --refresh-images` to regenerate captions in registered folders.
- Offer **EmbeddingGemma 2** as a single encoder for documents, applications, commands, settings, images, and text queries. It stores direct image vectors and shares one search model across result types. Its default cutoff of 0.68 applies to text.
- Reread images in registered folders when reindexing to or from EmbeddingGemma 2. The new model and its vectors are saved in one transaction.
- Let the downloader set up or reuse a user-local PyTorch runtime for image models. The CLI and desktop app report the actual PyTorch device, and the desktop app labels EmbeddingGemma 2 as a text and image model.

See the [model guide](docs/models.md) for downloads, image model selection, and reindexing instructions.

## 0.3.6

- Add **Switch Model** to the desktop app. Choose Granite 97M, Granite 311M, E5 Small, E5 Large, or a local custom ONNX model. Granite 97M remains the default for new indexes.
- Show when the selected model is ready, whether its files were downloaded or already present. Keep existing model files and ask before rebuilding the index with the selected model.
- Add `semfind reindex` to rebuild saved embeddings and save the model directory in one transaction. Preserve indexed folders and existing image descriptions, and keep the previous model and vectors if the rebuild fails.

## 0.3.5

- Show the active index's embedding model and the last execution device in the desktop app. The device reflects CPU fallback and stays visible when a search reuses a cached embedding.
- Show the Florence-2 checkpoint and its most recent CPU, CUDA, or ROCm device separately from the text embedding device.
- Include model names in `semfind status --json`. Pass `--runtime-info` to search or indexing commands to get execution-provider diagnostics on stderr.

## 0.3.4

- Fix **Download model** in the desktop app for existing indexes. It retrieves the exact supported ONNX weights and tokenizer used by the index, including the E5 Small INT8 variant. Granite 97M remains the default for new indexes.
- Automatically select the matching installed model directory when opening existing E5 Large and Granite 311M indexes without an explicit model selection.
- Explain how to switch models when a search or incremental scan detects incompatible embeddings. Rebuild with `semfind init --force` or maintain a separate index with `--index-dir`.

## 0.3.3

- Add an optional GTK 4 desktop app that searches the same local index as the CLI. Filter results by category, read file excerpts, open files and apps, and copy command examples.
- Manage indexed folders from the desktop app. Download the embedding model, index a selected folder or the home directory, and see elapsed time while indexing.
- Build separate `semfind` and `semfind-gui` Debian packages for Ubuntu and Pop!_OS 24.04 on amd64. The CLI package includes the Python libraries for CPU inference. The desktop package provides the menu entry and AppStream metadata.
- Add a generator and installation guide for signed APT repositories. The catalog links the Store entry to the optional desktop package.

## 0.3.2

- Add a Linux Quickstart with a verified cross-language search, a separate sample index, and steps to index your own files and applications.
- Document system packages for Pop!_OS, Ubuntu, Debian, Fedora, and Arch Linux. Explain how `uv` can download Python 3.12 when needed.
- List the COSMIC, KDE Plasma, GNOME, Cinnamon, and Rofi integrations together and link to dedicated guides for scripting, indexing, models, image search, and GPU setup.

## 0.3.1

- Add `semfind setup-gpu cuda` and `semfind setup-gpu migraphx` to install and verify local GPU runtimes for text embeddings.
- Offer GPU runtime setup before interactive `semfind init` when automatic selection falls back to the CPU and a suitable graphics driver is detected.
- Show the CUDA and MIGraphX setup commands in the CPU performance tip on Linux.

## 0.3.0

- Use compatible PyTorch CUDA or ROCm devices for Florence-2 captioning by default. Retry on the CPU if GPU inference fails and report the caption backend with `init --time`. Existing captions are reused across backends.
- Remove the messenger-specific query rewrite and keyword filter. Use the full query for embeddings and keep general application intent detection.

## 0.2.9

- Build a user-local COSMIC Launcher plugin bundle with `python3 integrations/cosmic/package.py`. Its installer reuses the user's existing Semfind setup.
- Add optional KDE Plasma 6 KRunner and GNOME Shell search adapters for the existing Semfind index.
- Add a Cinnamon menu search provider and a Rofi search mode for desktops such as Xfce, LXQt, and MATE.

## 0.2.8

- Interpret explicit requests such as `Ein Programm zum Chatten` as searches for installed applications. Match common chat and messenger terms across languages to keep unrelated results out of chat application searches.
- Describe Semfind as a semantic search CLI in the README, package metadata, and localized help.
- Discover configured CUDA and AMD MIGraphX runtimes for regular searches and manual indexing as well as `init`. Timed searches report the startup provider when they load the model.
- Reuse the last query embedding across standalone searches on Linux. Rank the current index on each lookup and skip the cache with `--no-cache`.
- Keep the search model loaded in a local background worker across separate Linux searches. The worker exits after 15 idle minutes by default. Set the timeout with `--daemon-idle-seconds` or `SEMFIND_DAEMON_IDLE_SECONDS` and bypass the worker with `--no-daemon`.
- Save, show, or reset the per-user background worker timeout with `semfind config daemon-idle-seconds`.

## 0.2.7

- Let `semfind init` use dedicated NVIDIA CUDA or AMD MIGraphX Python environments automatically, with CPU fallback. `init --time` also reports the ONNX Runtime provider selected at startup.
- Show a localized GPU performance tip once per search or indexing command when it uses the CPU for text embeddings. Suppress it with `--no-gpu-hint` or `SEMFIND_NO_GPU_HINT=true`.

## 0.2.6

- Add `semfind init --time` to report total indexing time and the duration of each phase in seconds on stderr. It also works with `--force`. The final index summary remains on stdout.

## 0.2.5

- Add `semfind init --force` to rebuild the home index, all registered directories, and the application, settings, and command catalogs. Saved exclusions apply to the rebuild.
- Rebuild the index in one transaction and roll it back on failure. Select a different embedding model during the rebuild to switch models in place.

## 0.2.4

- Show the current phase, elapsed time, and discovered file count during interactive `semfind init` runs. Progress appears on terminal stderr, and stdout still carries the final summary.
- Document APT package distribution as planned work for Pop!_OS and Ubuntu 24.04.

## 0.2.3

- Add an optional COSMIC Launcher plugin that shows local Semfind hits alongside launcher results. Use `semfind <query>` to see up to eight hits in Semfind's ranking order.
- Open files, installed applications, and supported COSMIC settings directly from search results. Linux command hits display usage examples.
- Add a user-local installer that reuses the configured Semfind index and model. The launcher keeps the search process running so subsequent queries reuse the loaded model.

## 0.2.2

- Search JPEG, PNG, and WebP images by subject using locally generated Florence-2 captions. Queries can return both images and documents. Use `--kind image` to show only images.
- `semfind init` uses Florence-2 automatically after local setup. It reuses existing captions and falls back to filename and path when the model is unavailable. Use `--no-caption-images` to skip caption generation for one run, or `semfind index ROOT --caption-images` to caption images in a selected directory.
- Add an optional Florence-2 Base FT download with a fixed model revision and SHA-256 verification. Caption generation and search run offline after setup.

## 0.2.1

- Show one-based line ranges for text passages and physical page numbers for PDF passages. JSON and streaming results include an optional `location` field.
- Upgrade existing indexes to schema 5 on open. Existing results remain searchable, and the first rescan adds locations to indexed text files and PDFs.

## 0.2.0

- Add `semfind search --stdin` for programs and AI agents. Read one UTF-8 query per line, flush one JSON result array per query, and reuse the index and model worker across requests.
- Add `--time` to report each search duration in milliseconds on stderr. Timing includes model startup when needed and ends before result formatting. JSON output stays on stdout.

## 0.1.9

- Describe semfind as an offline Linux search CLI in the README, package metadata, and localized command help.
- Document subprocess integration for programs and AI agents with a JSON search example, index status, and exit codes.
- List search and launcher integrations for KDE Plasma, GNOME, COSMIC, and other Linux desktops as planned work. The CLI continues to include a COSMIC settings catalog.

## 0.1.8

- Select the inference device automatically based on the configured ONNX Runtime. Try NVIDIA CUDA, AMD MIGraphX, and then the CPU. In automatic mode, retry failed GPU inference on the CPU.
- Choose a backend explicitly with `--device cpu`, `--device cuda`, or `--device migraphx`. You can also set `SEMFIND_DEVICE`. The same model can reuse an existing index across backends.
- Add NVIDIA and AMD setup instructions and a reproducible Granite 97M CPU and CUDA benchmark. MIGraphX support is experimental. Provider selection and CPU fallback were tested with a simulated AMD provider. Granite 97M inference and performance on AMD hardware still need verification.

## 0.1.7

- Select Granite 97M, Granite 311M, E5 Small, or E5 Large through `--model` or `SEMFIND_MODEL`. The downloader accepts the same choices.
- Configure custom ONNX embeddings with `semfind-model.json`, including pooling, prefixes, token limit, output name, and an optional similarity cutoff.
- Build a separate index when switching embedding models. The CLI checks the saved model identity before searching or writing vectors.

## 0.1.6

- Use Granite Embedding 97M Multilingual R2 by default for downloads and new indexes. The CPU worker uses CLS pooling and a cosine similarity cutoff of 0.86.
- Keep existing E5 indexes usable with their original model. To switch, download Granite and build a fresh index using the upgrade instructions in the README.
- Add a reproducible English retrieval benchmark for E5 Small and Large and Granite 97M and 311M. Granite 97M ranked all eight Linux command answers first in the benchmark.

## 0.1.5

- Localized terminal help, setup reminders, and status and result labels in English, German, Russian, Portuguese, French, Spanish, and Italian. semfind selects the language from the system locale and uses English as the fallback. Command names, options, and JSON fields use the same identifiers across locales.
- Added Russian, Portuguese, French, Spanish, and Italian search passages to the built-in Linux command and COSMIC settings catalogs. Run `semfind index` to add them to an existing index.
- `semfind init` picks up available desktop entry translations in the supported languages. Manual indexing recognizes localized section headings and prefers the configured language for translated excerpts.

## 0.1.4

- Added a searchable catalog of 31 Linux command entries with German and English descriptions and examples. Package-management results select Debian or Arch variants using the local distribution or the `--distro` override.
- `semfind init` indexes installed command manuals and executable names from `PATH`. Use `semfind index --manpages` to refresh the local command catalog independently of your home scan.
- Command results show the relevant manual or a copyable example. `semfind status` reports the command count, and existing indexes migrate automatically.
- On Linux, regular commands remind users on stderr to run `semfind init` until it completes successfully for the selected index. JSON output stays on stdout.

## 0.1.3

- Added `semfind init` on Linux to index supported files in the home directory, registered desktop applications, and COSMIC settings in one command. Run it again to refresh all three and reuse saved home exclusions.
- Desktop applications are searchable with `--category application` by name, desktop file ID, descriptions, and keywords, including German and English translations when available.
- `semfind status` now reports the application count. Existing indexes migrate automatically and retain their indexed files, settings, and saved exclusions.

## 0.1.2

- File listings and searches filtered to images or videos run without starting Python or loading the embedding model.
- E5 semantic matches require a cosine similarity of at least 0.78. Full-text and exact matches remain eligible regardless of similarity.
- Searches and status checks can read an existing index during a rescan's SQLite write reservation. Opening an index takes a write reservation only for schema creation or migration.
- In a local Pop!_OS benchmark with 203 synthetic files, image listing and video filename search fell from about 1.77 s to about 2 ms. Under a controlled SQLite write reservation, median status latency fell from 831 ms to 1.72 ms. Semantic search and rescan times were comparable.

## 0.1.1

- Added grouped filters for text, images, and videos. Run `semfind --kind image` to list images or supply a query to search image filenames and paths.
- Added filename and path indexing for supported image and video formats. Text and PDF searches continue to use lexical and semantic ranking.
- Existing indexes upgrade in place. Rescan your selected folders to include images and videos.
- Added semantic image search to the roadmap, including local image embeddings and tests with German and English queries.

## 0.1.0

- Added a local Rust CLI for selected folders and the COSMIC settings catalog.
- Added local multilingual E5 inference, persistent SQLite passage storage, and hybrid search with exact identifier priority.
- Added directory and type filters, JSON, saved exclusions, rescans, and root removal.
- Added quality checks and integration tests for persistence, ranking, CLI behavior, and local inference.
