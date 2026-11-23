# Roadmap

The Linux CLI is the primary interface. It supports local multilingual search over files, application metadata, Linux commands, and the COSMIC settings catalog. Scripts, applications, and AI agents can consume structured results with `--json`.

## Planned work

1. **Desktop integrations.** Verify the [Plasma, GNOME, Cinnamon, and Rofi adapters](../integrations/desktop/README.md) in clean desktop sessions.
2. **COSMIC distribution.** Verify the [COSMIC Launcher bundle](../integrations/cosmic/README.md) in a clean Pop!_OS session and include it in release distribution.
3. **Background indexer.** Run as the user. Use inotify to coalesce bursts, then perform periodic or overflow-triggered reconciliation. Add resource budgets, supported-type configuration, and a private Unix socket with a versioned request and response format.
4. **Image search improvements.** Evaluate OCR for screenshots and direct image embeddings for fine visual details alongside the existing Florence-2 captions. Expand German and English retrieval tests across mixed documents and images with neutral filenames.
5. **File history.** Explore event provenance and an opt-in version store with independent retention and restoration. Record process or user attribution only where the source is reliable.
6. **APT distribution.** Test installation from the [public signed repository](apt.md) through the COSMIC Store and verify upgrades on a clean system. Package upgrades should preserve users' local indexes and model files.
