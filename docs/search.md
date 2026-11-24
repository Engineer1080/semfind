# Search and manage the index

Follow the [quickstart](../README.md#quickstart) to install Semfind and run your first search.

## Build and refresh the index

On Linux, run `semfind init` after setup to index your home directory, desktop applications, and local command manuals. The first scan can take time on a large home directory or a system with many manuals.
In an interactive terminal, `init` shows the current phase, elapsed time, and the number of supported files found so far. The final summary reports how many entries were written, unchanged, removed, or skipped.
Use `semfind init --time` to report the total duration and the time spent in each phase on stderr. Combine it with `--force` to measure a complete rebuild. Timing covers model preparation through saving the index, in seconds.
Status, indexing, and search commands print a setup reminder on stderr until `init` succeeds for the selected index.

```bash
semfind init --exclude node_modules
semfind "Screenshots bearbeiten" --category application
semfind "Ein Programm zum Chatten"
semfind status
```

Run `semfind init` again to refresh the home directory, desktop applications, and local commands. Incremental rescans reuse embeddings for unchanged content. Home exclusions follow the same rules as `semfind index`. If a saved root overlaps your home directory, use `semfind unindex` with that root before adopting the whole home directory. Reapply any exclusions you want to keep.

Queries that explicitly ask for an application, such as `Ein Programm zum Chatten` or `An app for chatting`, search installed applications by the task described. Exact names and explicit filters take precedence.

Run `semfind init --force` to rebuild the selected index from scratch. It rescans the home directory and all registered directories with their saved exclusions. It also rebuilds the desktop application, settings, and command catalogs. All registered directories must be accessible. Semfind writes the rebuilt index in one transaction and rolls back if the rebuild fails. The final report counts the replaced entries as removed and the newly indexed entries as written.

During a forced rebuild, configured Florence-2 captioning applies to all scanned directories. With `--no-caption-images` or without a local Florence-2 model, the rebuilt index stores image filenames and paths. To switch the embedding model in place, select the new model when running `init --force` and use the same selection for searches.

To start with a smaller index, select a directory instead.

```bash
semfind index ./examples/documents
semfind "Wie lange sollen personenbezogene Daten gespeichert werden?" --type md
semfind "Mein Bildschirm ist zu dunkel" --category setting
semfind "E-0042" --type log --json
semfind status --json
```

## Languages

Terminal help, setup reminders, and status labels are available in English, German, Russian, Portuguese, French, Spanish, Italian, Simplified Chinese, Japanese, Dutch, Polish, and Turkish. The GTK desktop app is available in the same languages. Chinese locales use the Simplified Chinese translation. Semfind uses the configured message locale. It reads `LC_ALL`, `LC_MESSAGES`, and `LANG` in that order, with `LANGUAGE` preferences for non-`C` locales. The command names, flags, and JSON fields use stable identifiers.

```bash
LC_ALL=fr_FR.UTF-8 semfind --help
semfind "Comment installer un nouveau paquet ?" --category command
```

The built-in command and COSMIC catalogs index descriptions in all twelve languages. Run `semfind index` to add the translations to an existing catalog. Run `semfind init` to refresh translated desktop application metadata and local manuals when available.

## Find Linux commands

Run `semfind index` to load the built-in command and settings catalogs without scanning personal folders. Both `semfind init` and directory indexing also refresh the command catalog.

```bash
semfind index
semfind "Welche Ordner belegen am meisten Speicherplatz?" --category command
semfind "Wie installiere ich ein neues Paket?" --category command --distro debian
semfind "Wie installiere ich ein neues Paket?" --category command --distro arch --json
```

The CLI looks up the current distribution family in `os-release`. Debian, Ubuntu, Pop!_OS, and distributions with compatible IDs get Debian examples. Arch and its recognized derivatives get Arch examples. Other systems get generic Linux examples. Use `--distro debian`, `--distro arch`, or `--distro generic` to override the detection. Generic commands are included for both families. Examples are displayed as text, and command results include `distro` and `example` in JSON output. See the [APT manual](https://manpages.debian.org/stable/apt/apt.8.en.html) and the [pacman manual](https://man.archlinux.org/man/pacman.8.en) for package management syntax.

On Linux, `semfind init` includes the commands available on your machine. To refresh the command catalog on its own, run `semfind index --manpages`.

```bash
semfind index --manpages
semfind "Wie prüfe ich, ob zwei Dateien Byte für Byte gleich sind?" --category command
```

This scans manual sections 1, 6, and 8 from the configured manual path. It indexes the name, synopsis, and a short description where available. Install `man-db` for your system's configured `manpath` and for opening results with `man`. Compressed pages use the installed `gzip`, `xz`, `bzip2`, or `zstd` helper. Executable files from absolute `PATH` directories without a manual entry are searchable by name. Shell aliases, functions, and built-ins need an installed manual page to appear.

The first scan can take a minute or more on a large system. Run `semfind index --manpages` again after installing or removing packages. When you select another distribution, search uses the built-in catalog for that distribution. See the [manpath manual](https://manpages.debian.org/stable/man-db/manpath.1.en.html).

## Filter and manage indexed files

Semfind searches the contents of supported UTF-8 files, including plain text, Markdown, logs, CSV, and reStructuredText. It also searches PDFs with a text layer. File search returns the best matching passage per file.

```bash
semfind "Sommerurlaub" --kind image
semfind --kind video --path "$HOME/Videos"
semfind "Datenschutz" --kind text --type pdf
```

`--kind` groups file extensions and can list indexed files without a query. Images are searchable by filename and path, and captioned images are also searchable by subject. Videos are searchable by filename and path. Listing files and searching filenames in an index with no matching embeddings work without loading the model.

```bash
semfind index "$HOME/Documents" --exclude Private --exclude Archive/taxes
semfind "Probleme mit meinem Vermieter" --type pdf --path "$HOME/Documents"
semfind index "$HOME/Documents"  # rescan with the saved exclusions
semfind unindex "$HOME/Documents"
```

Passing exclusions replaces the saved list for that root. Use `--clear-excludes` to clear it. Quote multiword queries. Use `semfind search "status"` to search for a semfind subcommand name.

Local data defaults to `${XDG_DATA_HOME:-$HOME/.local/share}/semfind`. Set `SEMFIND_MODEL_DIR` and `SEMFIND_INDEX_DIR` to use other locations. The index directory must be private on Linux.
