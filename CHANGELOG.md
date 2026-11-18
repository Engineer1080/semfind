# Changelog

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
