# Changelog

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
