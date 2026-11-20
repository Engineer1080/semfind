# Roadmap

The Linux CLI is the primary interface. It supports local multilingual search over files, application metadata, Linux commands, and the COSMIC settings catalog. Scripts, applications, and AI agents can consume structured results with `--json`.

## Planned work

1. **Desktop integrations.** Develop search and launcher integrations for KDE Plasma, GNOME, COSMIC, and other Linux desktops using the shared search core. Choose the integration point for each desktop based on its extension APIs.
2. **COSMIC prototype.** Validate the packaged launcher backend on Pop!_OS 24.04. Prototype a native client if a globally discoverable, maintainable extension is unavailable. Add explicit activation for opening files, starting desktop applications discovered by `semfind init`, and opening allowlisted settings routes. Search and activation remain separate operations. See the [COSMIC research](../integrations/cosmic/README.md).
3. **Background indexer.** Run as the user. Use inotify to coalesce bursts, then perform periodic or overflow-triggered reconciliation. Add resource budgets, supported-type configuration, and a private Unix socket with a versioned request and response format.
4. **Image search improvements.** Evaluate OCR for screenshots and direct image embeddings for fine visual details alongside the existing Florence-2 captions. Expand German and English retrieval tests across mixed documents and images with neutral filenames.
5. **File history.** Explore event provenance and an opt-in version store with independent retention and restoration. Record process or user attribution only where the source is reliable.
