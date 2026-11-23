# Use Semfind from programs and AI agents

Call semfind as a subprocess to find local sources for a natural-language question. `--category`, `--path`, and `--limit` narrow the results before the caller opens a file or consults a manual. The index and model run locally after setup.

```bash
semfind "How do I show free disk space?" --category command --limit 3 --json
```

This query can return `df` with the example `df -h`. For file results, the JSON includes the source path and a matching text excerpt that a program or agent can use to inspect the original file. Use `semfind status --json` to check which directories are indexed.

File results can also include a `location` for the passage behind the snippet. Text files use one-based, inclusive line numbers such as `{"line_start":12,"line_end":17}`. PDFs use a one-based physical page number such as `{"page":3}`. The terminal shows these as `notes.md:12-17` or `report.pdf#page=3`. The JSON field is optional.

Existing indexes remain searchable after Semfind 0.2.1 automatically upgrades them to schema 5. Use 0.2.1 or newer with an upgraded index. Run `semfind index /path/to/folder` for each indexed folder, or `semfind init` for a home index, to add locations. The first scan after the upgrade reprocesses previously indexed text files and PDFs. Locations refer to the indexed text and refresh on rescan.

With `--json`, search writes a JSON array to stdout, including `[]` for zero results. Diagnostics go to stderr. Exit codes are `0` for success (including zero hits), `1` for an operational error, and `2` for invalid CLI usage.

`status --json` includes `model_name` for the embedding model, `model_directory` for a model directory saved by `reindex`, and `caption_model_name` for an available Florence-2 checkpoint. Use `--runtime-info` on a search, `index`, `reindex`, or `init` to get a JSON record prefixed with `semfind: runtime ` on stderr. `embedding_provider` reports the loaded ONNX provider after any automatic fallback. `caption_backend` reports the Florence-2 device when the caption worker started. If a worker was never loaded, its provider is `null`. `cache_hit` indicates a reused query embedding.

## Timing, cache, and model worker

Add `--time` to report the search duration in milliseconds on stderr. Measurement starts after the index is opened and includes model startup when needed, retrieval, and ranking. Result formatting is outside the measurement.

On Linux, standalone searches in automatic mode can reuse the last query embedding from the selected index directory. Semfind still ranks the current index and checks that file hits are accessible. `--time` reports a cache hit, and `--no-cache` skips reading and saving the cached embedding for one invocation.

When a standalone search needs a new embedding, Semfind on Linux starts a local background worker on demand. By default, the worker keeps the selected model loaded for 15 minutes after the last embedding. It uses automatic CUDA, MIGraphX, and CPU selection. `--time` shows `ONNX Runtime (resident)` when a search uses an already loaded model.

Save a five-minute timeout for future searches, show the saved value, or restore the 15-minute default.

```bash
semfind config daemon-idle-seconds 300
semfind config daemon-idle-seconds
semfind config daemon-idle-seconds --reset
```

Semfind stores this setting in `$XDG_CONFIG_HOME/semfind/daemon-idle-seconds` (by default `~/.config/semfind/daemon-idle-seconds`). The value survives a restart. The `--daemon-idle-seconds` flag takes precedence over `SEMFIND_DAEMON_IDLE_SECONDS`, which takes precedence over the saved value. The worker uses the setting when it computes the next embedding. A cached query leaves the current timer running. Use `--no-cache` to compute a new embedding for a repeated query.

Use `--no-daemon` or `SEMFIND_NO_DAEMON=true` to run inference directly. Semfind also runs inference directly when you select CPU, CUDA, or MIGraphX explicitly, pass `--python`, or when the worker is unavailable. For cold-start comparisons, combine `--no-daemon` with `--no-cache`.

```bash
semfind "How do I show free disk space?" --category command --json --time
```

## Run multiple searches in one process

```bash
printf '%s\n' 'How do I show free disk space?' 'How do I find files by name?' |
  semfind search --stdin --category command --limit 3 --time
```

`--stdin` reads one plain UTF-8 query per line and ignores blank lines. It writes and flushes one JSON result array for each query in input order, including `[]` for zero hits. Applications can send another query after reading the previous result while keeping stdin open. All queries share the same filters. The process opens the index once and reuses the model worker after the first query that requires embeddings.

`--time` reports each query's duration on stderr. Closing stdin ends the session. If a query or input line fails, semfind reports its line number on stderr and exits with code `1`. Results already emitted remain valid.
