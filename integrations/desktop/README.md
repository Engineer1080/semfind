# Semfind desktop search

These integrations reuse your existing Semfind CLI, model, and index. They show files, installed applications, and Linux command examples. Files and applications open with their desktop handlers. In GNOME, selecting a command copies its example when the Shell supports the search provider clipboard field. The adapters pass searches to the CLI and reuse its background worker and idle configuration in automatic mode.

Install Semfind and run `semfind init` before setup. Use a Python interpreter with PyGObject and Gio (for example, `/usr/bin/python3` and the `python3-gi` package on Debian or Ubuntu). Run the installer in the environment where your Semfind CLI works. Pass `--semfind /absolute/path/to/semfind` if the executable or wrapper is outside your PATH.

## KDE Plasma 6

From the repository root, run:

```bash
/usr/bin/python3 integrations/desktop/install.py plasma
```

Log out and back in so KRunner reloads its plugins. Open KRunner and enter `semfind Freien Speicherplatz auf Laufwerken anzeigen`. Semfind is listed in the KRunner plugin settings. The `semfind` prefix limits when our runner participates. [KRunner D-Bus runner metadata](https://develop.kde.org/docs/plasma/krunner/metadata/)

## GNOME Shell

From the repository root, run:

```bash
/usr/bin/python3 integrations/desktop/install.py gnome
sudo install -Dm644 integrations/desktop/semfind-search-provider.ini \
  /usr/local/share/gnome-shell/search-providers/semfind-search-provider.ini
```

The installer registers the D-Bus service and application entry for your user. GNOME Shell discovers provider descriptors through its system data directories, so the second command installs the descriptor separately. Log out and back in. In Settings → Search, enable Semfind if it is disabled. Open Activities and type a query such as `Freien Speicherplatz auf Laufwerken anzeigen`. [GNOME Shell search providers](https://developer.gnome.org/documentation/tutorials/search-provider.html) · [GNOME Shell provider discovery](https://github.com/GNOME/gnome-shell/blob/main/js/ui/remoteSearch.js)

Use `both` in place of `plasma` or `gnome` to install both adapters for the same user. For GNOME, install the provider descriptor as above.

## Cinnamon

From a Cinnamon session, run:

```bash
/usr/bin/python3 integrations/desktop/install.py cinnamon
```

The installer adds Semfind to Cinnamon's enabled search providers. Open the built-in Cinnamon menu and type a query such as `Freien Speicherplatz auf Laufwerken anzeigen`. For commands, the result shows the example and selecting it copies that example. If you just upgraded the adapter and Cinnamon still has an older copy loaded, log out and back in. [Cinnamon search provider interface](https://github.com/linuxmint/cinnamon/blob/master/js/ui/searchProviderManager.js)

## Rofi on Xfce, LXQt, MATE and other desktops

Install Rofi for your session, then run from the repository root:

```bash
/usr/bin/python3 integrations/desktop/install.py rofi
rofi -show semfind -modi "semfind:${XDG_DATA_HOME:-$HOME/.local/share}/semfind/desktop/semfind-rofi" -show-icons
```

Type a query and press Enter to run the search. Select a file or application and press Enter to open it. Command examples appear as reference rows. You can bind the Rofi command to a desktop keyboard shortcut or invoke it from an Xfce Application Finder custom action. This opens its own search window. [Rofi script mode](https://davatorium.github.io/rofi/current/rofi-script.5/)

## Remove

Remove `krunner/dbusplugins/semfind-runner.desktop` for Plasma or `applications/io.github.Engineer1080.Semfind.desktop` for GNOME under your user data directory (`$XDG_DATA_HOME`, normally `~/.local/share`). For GNOME remove the installed system provider descriptor when it is no longer needed by any user. For Cinnamon remove `semfind@engineer1080` from `org.cinnamon enabled-search-providers` and remove `cinnamon/search_providers/semfind@engineer1080/`. For Rofi remove `semfind/desktop/semfind-rofi` and `semfind/desktop/rofi.py`. Once you have removed every adapter, remove `dbus-1/services/io.github.Engineer1080.Semfind.service` and `semfind/desktop/`. Reopen the desktop session to reload providers.
