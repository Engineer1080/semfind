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

Open **Manage index** to choose an embedding model and index a selected folder or your home directory. A home scan also indexes installed applications and local command manuals. A progress indicator and elapsed time remain visible while the CLI is working. Refresh or remove a folder from the list to update its index entries. Press **Ctrl+K** to focus the search box.

The **Local index** card shows the embedding model and the status of optional Florence-2 image captioning. After a search or scan loads the embedding model, the card shows the last execution device: CPU, NVIDIA CUDA, or AMD MIGraphX. A cached search can reuse an embedding without starting the model. The image caption device appears after captions are generated, using CPU, CUDA, or ROCm.

In **Manage index → Switch Model**, choose Granite 97M, Granite 311M, E5 Small, E5 Large, or a [custom ONNX model directory](../../docs/models.md#use-your-own-model). Granite 97M is selected by default for a new index. Semfind downloads and verifies any missing built-in model files. When the model is ready, choose **Reindex** to rebuild all saved embeddings, or **Use model** if the index is empty. The active model changes when the rebuild succeeds, and the app saves the selection with that index. Previously downloaded models, indexed folders, and saved image descriptions are retained. Use **Refresh** to scan a folder for new or changed files.

To open the desktop app with a separate index and model, close the running app and launch it from a terminal with `SEMFIND_INDEX_DIR` and `SEMFIND_MODEL` set. The selected index then applies to searches and all actions in **Manage index**.
