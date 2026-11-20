# Changelog

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
