# Semfind desktop app

Semfind has an optional GTK window for searching the same local index as the CLI. Select a result category to narrow the search. File results show paths and excerpts, plus page or line numbers when the index has them. Open files and applications with their desktop handlers. Copy command examples to the clipboard.

## Run from source

Install the [Semfind CLI](../../README.md#quickstart), a system Python with PyGObject, and GTK 4.10 or newer. On Ubuntu and Pop!_OS 24.04, the system packages are `python3-gi` and `gir1.2-gtk-4.0`.

Run the app from the repository root in the same environment you use for Semfind.

```bash
/usr/bin/python3 integrations/gui/app.py
```

Use `--semfind /path/to/semfind` to select a particular executable or wrapper. The app inherits the CLI's index, model, and device settings. See the [APT guide](../../docs/apt.md) to build the optional `semfind-gui` package and install the menu entry.

## Manage the index

Open **Manage index** to download an embedding model and index a selected folder or your home directory. A home scan also indexes installed applications and local command manuals. A progress indicator and elapsed time remain visible while the CLI is working. Refresh or remove a folder from the list to update its index entries. Press **Ctrl+K** to focus the search box.
