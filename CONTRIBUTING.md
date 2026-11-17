# Contributing to semfind

1. Install the prerequisites from the [README](README.md).
2. Run the checks below.
3. Describe the behavior change and how you verified it. Include synthetic or redacted examples for search bugs.

```bash
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo deny --locked check
```

The default test suite uses controlled vectors for ranking and storage regressions. For the CPU inference and PDF smoke test, set `SEMFIND_PYTHON` and `SEMFIND_MODEL_DIR` to your prepared runtime and model, install `pdftotext`, and run:

```bash
cargo test --locked -p semfind-cli --test cli real_model -- --ignored
```

Keep changes focused and use small descriptive commits. Update the model recipe version when encoding changes and add a migration or explicit rebuild path when the schema changes. Describe changes to collected data in [the privacy documentation](docs/privacy.md). semfind follows Semantic Versioning from 0.1.0. Early CLI and schema changes are recorded in the changelog.

For security issues, use the process in [SECURITY.md](SECURITY.md).
