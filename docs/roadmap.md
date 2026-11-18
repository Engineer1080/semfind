# Roadmap

1. **CLI MVP.** Selected roots, manual reconciliation, bilingual local embeddings, hybrid retrieval, an experimental COSMIC settings catalog, and CLI integration tests.
2. **COSMIC.** Validate the packaged launcher backend on Pop!_OS 24.04. Prototype a native client if a globally discoverable, maintainable extension is unavailable. Add explicit activation for opening files and allowlisted settings routes. Search and activation remain separate operations. Add application discovery using desktop entry metadata.
3. **Background indexer.** Run as the user. Use inotify to coalesce bursts, then perform periodic or overflow-triggered reconciliation. Add resource budgets, supported-type configuration, and a private Unix socket with a versioned request and response format.
4. **Semantic image search.** Use a local multimodal model to encode image pixels and natural-language queries into the same vector space. Store one embedding per indexed image and recompute it when the image or model changes. Use the existing image filter to retrieve pictures by subject, including a cat photo with a neutral filename. Test German and English queries against a mix of matching and unrelated pictures.
5. **File history.** Explore event provenance and an opt-in version store with independent retention and restoration. Record process or user attribution only where the source is reliable.
