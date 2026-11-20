# Semfind for COSMIC Launcher

The optional plugin searches your existing Semfind index from COSMIC Launcher. It displays files, captioned images, desktop applications, COSMIC settings, and Linux command examples. Run `semfind init` to build or refresh the index.

## Install

Set up Semfind and its embedding model as described in the [main README](../../README.md#install-on-pop_os-2404). Then build and install the plugin from the repository root as your regular user.

```bash
cargo build --locked --release -p semfind-cli -p semfind-cosmic
python3 integrations/cosmic/install.py
```

Run the installer from the environment you use to search with Semfind. It uses the `semfind` executable on your `PATH` or the local release build, and saves any configured `SEMFIND_PYTHON`, `SEMFIND_MODEL_DIR`, `SEMFIND_MODEL`, `SEMFIND_INDEX_DIR`, and `SEMFIND_DEVICE` values. If you run the installer with a virtual environment's Python, it uses that interpreter by default. Pass `--semfind` to select a specific executable or wrapper.

The installer writes to `~/.local/share/pop-launcher/plugins/semfind/`. Log out and back in to reload the plugin list. Remove that directory to uninstall the plugin.

## Search

- Type a query directly into COSMIC Launcher to see Semfind hits alongside other launcher results.
- Type `semfind <query>` to see up to eight Semfind results in Semfind's ranking order.
- Select a file, application, or supported setting and press Enter to open it. The plugin uses `xdg-open` for files, `gtk-launch` for indexed desktop entries, and `cosmic-settings` for the supported settings pages. Linux command hits show their examples for reference.

After the first query loads the embedding model, subsequent queries reuse it for the lifetime of the launcher service. Pop Launcher sorts and can limit mixed results. Use the prefix when you want the full set of Semfind hits. Image content is searchable after you [enable Florence-2](../../README.md#search-images-by-subject) and refresh the index.

The adapter uses [Pop Launcher's JSON IPC](https://github.com/pop-os/launcher#json-ipc) and sends queries to a local `semfind search --stdin` process. The installer uses [Pop Launcher's user-local plugin directory](https://github.com/pop-os/launcher#plugin-directories).
