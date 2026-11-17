# Privacy and local data

semfind processes files only under roots passed to `semfind index`. The built-in catalog is included on every scan. The app loads model and tokenizer files locally, uses an embedded Python worker over process pipes, and disables ONNX Runtime telemetry. Setup downloads external dependencies and model files. The downloader only requests the pinned model artifacts.

## Files and stored data

- Only regular files with a supported extension are opened. Hidden files and folders, including `.ssh`, are excluded. Named and relative-path exclusions are saved per root. The system bases `/proc`, `/sys`, `/dev`, and `/run` are excluded.
- On Linux, directory and file lookups use `openat2` to reject symbolic links. Descendant file lookup also rejects mount crossings. A selection must be a directory and cannot be a filesystem root. Unreadable files are omitted. Search checks that an indexed file still opens before displaying its result.
- Extraction reads at most 16 MiB per file and caps stored text at 512 KiB of PDF output or 512 Ki Unicode characters of UTF-8 text. `pdftotext` runs with fixed arguments and a bounded read deadline. It receives bytes over stdin.
- SQLite stores root paths, exclusions, absolute file paths, names, extensions, content hashes, text passages, catalog descriptions, and embedding vectors. Error output can include local paths. The index directory and database are created with modes `0700` and `0600` respectively on Linux. The index is plaintext and its paths and snippets can be sensitive. Keep the index on storage appropriate for the data it indexes.

Run `semfind unindex ROOT` to remove the registered root and its data. A rescan removes entries for disappeared, newly excluded, or unreadable files. SQLite and FTS5 secure deletion are enabled. Storage-level copies, backups, and filesystem snapshots have their own retention. A search result may include old text until its still-readable file is rescanned.
