# Architecture

```mermaid
flowchart LR
    CLI[semfind CLI] --> Core[semfind-core]
    Files[Selected files] --> Core
    Catalog[COSMIC catalog] --> Core
    Core -->|Local pipes| Worker[E5 / ONNX Runtime]
    Core --> DB[(SQLite / FTS5 + vectors)]
```

## Write path

`Index::synchronize` reconciles one explicitly selected root and the versioned built-in settings catalog. It compares hashes of source bytes to skip unchanged content and deletes records missing from the completed scan. A scan failure or embedding error rolls back the transaction. Supported files that cannot be read or extracted are reported and omitted, including any prior contents. The index keeps roots, saved exclusions, source metadata, normalized little-endian float vectors, and FTS5 rows keyed by passage ID. Foreign keys and a delete trigger keep passage and FTS rows aligned.

Extracted text is broken at nearby word boundaries, up to 640 Unicode characters with about 80 characters of overlap. Embeddings include a clipped title and use E5's `query:` and `passage:` prefixes. Python tokenizes to at most 512 tokens, runs CPU inference and mean-pools unmasked output tokens. Rust checks dimensions and normalizes vectors. The tokenizer and weight checksums plus an encoding recipe version identify the vector space. A different identity requires a separate or rebuilt index.

## Read path

Path, extension and category define the result set. The index confirms that each file still opens under its registered root before considering its saved contents. FTS5 ranks escaped terms with BM25. Cosine similarity ranks stored vectors, taking the best passage per source in each retrieval channel. Reciprocal Rank Fusion combines the top candidates from each list with k = 60. Full path, filename and settings ID matches rank first, then literal code matches in passages. Remaining ties are deterministic. Semantic ranking uses relative similarity, so an unrelated query can still return candidates.

## Trade-offs

- **SQLite with bundled FTS5.** One portable database keeps full-text and vector updates atomic. [sqlite-vec's Rust bindings](https://github.com/asg017/sqlite-vec/blob/main/site/using/rust.md) support static registration with bundled rusqlite. The MVP uses a streamed, exact cosine scan in Rust to keep vector dimensions model-independent and avoid an additional native extension. A larger corpus will need measured latency and recall before moving to an approximate index.
- **Local Python inference process.** ONNX Runtime and the upstream tokenizer run as a child of the Rust application. The child is reused across batches during one command. This keeps inference separate from retrieval, with the cost of a Python setup and a fresh model load on each invocation. The `Embedder` trait is the boundary for alternate runtimes.
- **Manual reconciliation.** Scans hash every eligible file and run one transaction per selected root. This handles removal, interrupted scans, and changes that leave timestamps unchanged. The trade-offs are read I/O on every scan and a write lock during inference. A service can reuse the same schema and retrieval interface while staging work in smaller transactions.
- **Historical data.** Future history and restore would use dedicated storage and index records with distinct source identities. This schema holds the current indexed snapshot.
