#!/usr/bin/python3
"""Graphical client for Semfind's local index."""

import argparse
import json
from pathlib import Path
import shutil
import sys
import time

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
from gi.repository import Gdk, Gio, GLib, Gtk, Pango

APP_ID = "io.github.engineer1080.semfind"
HERE = Path(__file__).resolve().parent
CATEGORIES = {
    "": ("All results", "system-search-symbolic"),
    "file": ("Files", "text-x-generic-symbolic"),
    "application": ("Applications", "view-app-grid-symbolic"),
    "command": ("Commands", "utilities-terminal-symbolic"),
    "setting": ("Settings", "preferences-system-symbolic"),
}
HIT_LABELS = {
    "file": "File", "application": "Application", "command": "Command",
    "setting": "Setting", "action": "Action",
}


def box(spacing=0, vertical=False, style=None):
    widget = Gtk.Box(
        orientation=Gtk.Orientation.VERTICAL if vertical else Gtk.Orientation.HORIZONTAL,
        spacing=spacing,
    )
    if style:
        widget.add_css_class(style)
    return widget


def label(text, style=None, *, wrap=False):
    widget = Gtk.Label(label=text, xalign=0, wrap=wrap)
    if wrap:
        widget.set_wrap_mode(Pango.WrapMode.WORD_CHAR)
    if style:
        widget.add_css_class(style)
    return widget


def button(text, callback, style=None, icon=None):
    widget = Gtk.Button()
    if icon:
        content = box(8)
        content.set_halign(Gtk.Align.CENTER)
        content.append(Gtk.Image.new_from_icon_name(icon))
        content.append(Gtk.Label(label=text))
        widget.set_child(content)
    else:
        widget.set_label(text)
    widget.connect("clicked", lambda *_: callback())
    if style:
        widget.add_css_class(style)
    return widget


def clear(box_widget):
    while (child := box_widget.get_first_child()) is not None:
        box_widget.remove(child)


def run_process(arguments, complete):
    """Collect both pipes asynchronously so CLI output cannot block the window."""
    flags = Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_PIPE
    try:
        process = Gio.Subprocess.new(arguments, flags)
    except GLib.Error as error:
        GLib.idle_add(complete, None, error.message)
        return None

    def finished(process, result):
        try:
            _, output, errors = process.communicate_utf8_finish(result)
        except GLib.Error as error:
            complete(None, error.message)
            return
        if process.get_successful():
            complete(output, "")
        else:
            complete(None, errors.strip() or output.strip() or "Semfind exited unexpectedly.")

    process.communicate_utf8_async(None, None, finished)
    return process


class SearchWindow(Gtk.ApplicationWindow):
    def __init__(self, application, cli):
        super().__init__(application=application, title="Semfind", default_width=1080, default_height=760)
        self.cli = cli
        self.status = {}
        self.category = ""
        self.search_request = 0
        self.search_process = None
        self.active_query = ""
        self.job = None
        self.tabs = {}
        self.add_css_class("semfind")
        self.connect("close-request", self.on_close)
        self.build()
        for name, handler in (("focus-search", self.entry.grab_focus), ("manage-index", self.manage_index)):
            action = Gio.SimpleAction.new(name, None)
            action.connect("activate", lambda *_args, handler=handler: handler())
            self.add_action(action)
        application.set_accels_for_action("win.focus-search", ["<Control>k"])
        self.refresh_status()

    def build(self):
        header = Gtk.HeaderBar()
        header.add_css_class("top-bar")
        brand = box(10)
        mark = Gtk.Image.new_from_icon_name("system-search-symbolic")
        mark.add_css_class("brand-icon")
        brand.append(mark)
        brand.append(label("Semfind", "brand-word"))
        header.pack_start(brand)
        header.set_title_widget(Gtk.Label())
        self.set_titlebar(header)

        body = box()
        self.set_child(body)
        self.sidebar = box(6, vertical=True, style="sidebar")
        body.append(self.sidebar)
        self.sidebar.append(label("SEARCH IN", "section-label"))
        previous = None
        for category, (name, icon) in CATEGORIES.items():
            tab = Gtk.ToggleButton()
            if previous:
                tab.set_group(previous)
            previous = tab
            row = box(10)
            row.append(Gtk.Image.new_from_icon_name(icon))
            row.append(label(name))
            tab.set_child(row)
            tab.add_css_class("nav")
            tab.set_active(category == "")
            tab.connect("toggled", self.change_category, category)
            self.sidebar.append(tab)
            self.tabs[category] = tab

        space = box()
        space.set_vexpand(True)
        self.sidebar.append(space)
        index_card = box(8, vertical=True, style="index-card")
        index_card.append(label("LOCAL INDEX", "section-label"))
        self.index_summary = label("Checking index…", "index-summary", wrap=True)
        index_card.append(self.index_summary)
        manage = button("Manage index", self.manage_index, "quiet", "folder-symbolic")
        index_card.append(manage)
        self.sidebar.append(index_card)

        scroll = Gtk.ScrolledWindow(hexpand=True, vexpand=True)
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        body.append(scroll)
        main = box(18, vertical=True, style="workspace")
        scroll.set_child(main)

        hero = box(14, vertical=True, style="search-panel")
        hero.append(label("Find by meaning.", "hero-title"))
        hero.append(label("Search your files, apps, Linux commands, and settings.", "hero-subtitle", wrap=True))
        form = box(10)
        self.entry = Gtk.Entry(
            placeholder_text="Describe what you're looking for…", hexpand=True, max_length=2048
        )
        self.entry.add_css_class("query")
        self.entry.connect("activate", lambda *_: self.search())
        form.append(self.entry)
        self.search_button = button("Search", self.search, "primary", "system-search-symbolic")
        form.append(self.search_button)
        hero.append(form)
        hero.append(label("Ctrl+K to focus search", "shortcut"))
        main.append(hero)

        self.feedback_label = label("", "feedback", wrap=True)
        self.feedback_label.set_visible(False)
        main.append(self.feedback_label)
        self.progress = Gtk.ProgressBar(visible=False)
        main.append(self.progress)

        self.content = box(10, vertical=True)
        main.append(self.content)
        self.show_welcome()

    def change_category(self, tab, category):
        if tab.get_active():
            self.category = category
            if self.active_query and not self.job:
                self.search()

    def feedback(self, text="", error=False):
        self.feedback_label.set_label(text)
        self.feedback_label.set_visible(bool(text))
        if error:
            self.feedback_label.add_css_class("error")
        else:
            self.feedback_label.remove_css_class("error")

    def alert(self, title, detail):
        dialog = Gtk.AlertDialog(message=title, detail=detail)
        dialog.show(self)

    def confirm(self, title, detail, action, callback):
        dialog = Gtk.AlertDialog(message=title, detail=detail, buttons=["Cancel", action],
                                 cancel_button=0)
        def chosen(dialog, result):
            try:
                choice = dialog.choose_finish(result)
            except GLib.Error:
                return
            if choice == 1:
                callback()
        dialog.choose(self, None, chosen)

    def on_close(self, _window):
        if self.job:
            self.alert("An index operation is running", "The window can close when the operation finishes.")
            return True
        self.cancel_search()
        return False

    def update_status(self, output, error):
        if output is None:
            self.index_summary.set_label("Index unavailable")
            self.feedback(error, error=True)
            return
        try:
            self.status = json.loads(output)
        except ValueError:
            self.feedback("Semfind returned an unreadable index status.", error=True)
            return
        files = self.status["files"]
        roots = len(self.status["roots"])
        file_label = "file" if files == 1 else "files"
        root_label = "folder" if roots == 1 else "folders"
        self.index_summary.set_label(f"{files:,} {file_label} · {roots} {root_label}")
        if not self.active_query:
            self.show_welcome()

    def refresh_status(self):
        run_process([self.cli, "status", "--json"], self.update_status)

    def show_welcome(self):
        clear(self.content)
        if not self.status.get("model"):
            welcome = box(10, vertical=True, style="surface")
            welcome.append(label("Make your files searchable", "card-title"))
            welcome.append(label(
                "Download the local model, then index a folder or your home directory.",
                "muted", wrap=True,
            ))
            welcome.append(button("Set up your index", self.manage_index, "quiet", "folder-symbolic"))
        else:
            welcome = box(10, vertical=True, style="surface")
            welcome.append(label("Try a search", "card-title"))
            examples = (
                ("How do I show free disk space?", "command"),
                ("An app for editing screenshots", "application"),
            )
            for query, category in examples:
                welcome.append(button(query, lambda q=query, c=category: self.example(q, c), "example"))
        self.content.append(welcome)

    def example(self, query, category):
        self.entry.set_text(query)
        tab = self.tabs[category]
        needs_search = tab.get_active() or not self.active_query
        tab.set_active(True)
        if needs_search:
            self.search()

    def cancel_search(self):
        self.search_request += 1
        if self.search_process:
            self.search_process.force_exit()
            self.search_process = None

    def search(self):
        if self.job:
            return
        query = self.entry.get_text().strip()
        self.cancel_search()
        self.active_query = query
        if not query:
            self.feedback()
            self.show_welcome()
            return

        arguments = [self.cli, "search", "--json", "--limit", "30", "--no-gpu-hint"]
        if self.category:
            arguments += ["--category", self.category]
        request = self.search_request
        self.feedback("Searching your local index…")
        clear(self.content)

        def completed(output, error):
            if request != self.search_request:
                return
            self.search_process = None
            if output is None:
                self.feedback("Search failed. Check the model and index in Manage index.", error=True)
                self.show_error_details(error)
                return
            try:
                hits = json.loads(output)
            except ValueError:
                self.feedback("Semfind returned an unreadable response.", error=True)
                return
            self.feedback()
            self.render_results(hits)

        self.search_process = run_process([*arguments, "--", query], completed)

    def show_error_details(self, error):
        self.content.append(button("Show error details", lambda: self.alert(
            "Semfind reported an error", error[-1600:]
        ), "quiet"))

    def render_results(self, hits):
        clear(self.content)
        if not hits:
            card = box(8, vertical=True, style="surface")
            card.append(label("No matches", "card-title"))
            card.append(label("Try a different query or select another category.", "muted", wrap=True))
            self.content.append(card)
            return
        name = "result" if len(hits) == 1 else "results"
        self.content.append(label(f"{len(hits)} {name}", "results-heading"))
        for hit in hits:
            self.content.append(self.result_card(hit))

    def result_card(self, hit):
        category = hit["category"]
        card = box(12, style="result")
        icon_name = CATEGORIES.get(category, CATEGORIES[""])[1]
        badge = Gtk.Image.new_from_icon_name(icon_name)
        badge.add_css_class("result-icon")
        badge.add_css_class(category)
        badge.set_valign(Gtk.Align.START)
        card.append(badge)
        text = box(5, vertical=True)
        text.set_hexpand(True)
        card.append(text)
        kind = HIT_LABELS.get(category, "Result")
        if category == "command" and hit.get("distro") not in (None, "generic"):
            kind += " · " + hit["distro"].capitalize()
        text.append(label(kind, "result-kind"))
        title = label(hit["title"], "result-title")
        title.set_ellipsize(Pango.EllipsizeMode.END)
        text.append(title)

        path = hit.get("path")
        location = hit.get("location") or {}
        if path:
            source = path
            if "page" in location:
                source += f' · page {location["page"]}'
            elif "line_start" in location:
                source += f' · lines {location["line_start"]}–{location["line_end"]}'
            context = label(source, "source")
            context.set_ellipsize(Pango.EllipsizeMode.MIDDLE)
            context.set_tooltip_text(source)
            text.append(context)
        if hit.get("snippet"):
            summary = label(hit["snippet"], "snippet", wrap=True)
            summary.set_lines(3)
            summary.set_ellipsize(Pango.EllipsizeMode.END)
            text.append(summary)
        if example := hit.get("example"):
            command = label(example, "command-example", wrap=True)
            command.set_selectable(True)
            text.append(command)

        action = None
        if category == "file" and path:
            action = button("Open", lambda: self.open_file(path), "quiet", "document-open-symbolic")
        elif category == "application":
            action = button("Launch", lambda: self.launch_app(hit["id"]), "quiet", "media-playback-start-symbolic")
        elif category == "command" and example:
            action = button("Copy", lambda: self.copy_command(example), "quiet", "edit-copy-symbolic")
        if action:
            action.set_valign(Gtk.Align.CENTER)
            card.append(action)
        return card

    def open_file(self, path):
        try:
            uri = Gio.File.new_for_path(path).get_uri()
            Gio.AppInfo.launch_default_for_uri(uri, None)
        except GLib.Error as error:
            self.alert("Could not open the file", error.message)

    def launch_app(self, desktop_id):
        try:
            app = Gio.DesktopAppInfo.new(desktop_id)
            if not app or not app.launch([], None):
                self.alert("Application unavailable", "Refresh the index after installing or removing apps.")
        except GLib.Error as error:
            self.alert("Could not launch the application", error.message)

    def copy_command(self, command):
        self.get_clipboard().set_content(Gdk.ContentProvider.new_for_value(command))
        self.feedback("Command copied to clipboard.")

    def manage_index(self):
        if self.job:
            return
        dialog = Gtk.Window(title="Manage index", transient_for=self, modal=True, default_width=580)
        dialog.add_css_class("semfind")
        dialog.set_titlebar(Gtk.HeaderBar())
        container = box(14, vertical=True, style="workspace")
        dialog.set_child(container)
        container.append(label("Your local index", "card-title"))
        container.append(label(
            "Semfind indexes only the folders you choose. The home scan also adds installed apps and local commands.",
            "muted", wrap=True,
        ))

        roots = box(8, vertical=True)
        scroll = Gtk.ScrolledWindow(propagate_natural_height=True, max_content_height=270)
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        scroll.set_child(roots)
        container.append(scroll)
        for root in self.status.get("roots", []):
            row = box(8, style="root-row")
            location = label(root, "source")
            location.set_hexpand(True)
            location.set_ellipsize(Pango.EllipsizeMode.MIDDLE)
            location.set_tooltip_text(root)
            row.append(location)
            row.append(button("Refresh", lambda root=root: self.refresh_folder(dialog, root), "quiet"))
            row.append(button("Remove", lambda root=root: self.remove_folder(dialog, root), "quiet"))
            roots.append(row)

        buttons = box(8)
        buttons.append(button("Download model", lambda: self.download_model(dialog), "quiet"))
        buttons.append(button("Index home", lambda: self.index_home(dialog), "quiet"))
        buttons.append(button("Choose folder", lambda: self.choose_folder(dialog), "primary"))
        container.append(buttons)
        dialog.present()

    def download_model(self, dialog):
        packaged = Path(self.cli).with_name("semfind-download-model")
        source = HERE.parents[1] / "scripts/download_model.py"
        if packaged.is_file():
            arguments = [str(packaged)]
        elif source.is_file():
            arguments = [sys.executable, str(source)]
        else:
            self.alert("Model downloader unavailable", "Run the model downloader from your Semfind installation.")
            return
        dialog.close()
        self.start_job(arguments, "Downloading the embedding model")

    def choose_folder(self, dialog):
        chooser = Gtk.FileDialog(title="Choose a folder to index")
        def selected(chooser, result):
            try:
                folder = chooser.select_folder_finish(result).get_path()
            except GLib.Error:
                return
            if folder:
                self.refresh_folder(dialog, folder)
            else:
                self.alert("Choose a local folder", "Semfind can index directories on your computer.")
        chooser.select_folder(dialog, None, selected)

    def refresh_folder(self, dialog, folder):
        dialog.close()
        self.start_job([self.cli, "index", "--", folder], "Indexing the selected folder")

    def index_home(self, dialog):
        dialog.close()
        self.confirm("Index your home directory?",
                     "This scans supported files under your home directory and updates installed apps and local commands.",
                     "Index home",
                     lambda: self.start_job([self.cli, "init"], "Indexing your home directory"))

    def remove_folder(self, dialog, folder):
        dialog.close()
        self.confirm("Remove this folder from the index?",
                     f"Semfind will remove its index entries. Your files stay at {folder}.",
                     "Remove",
                     lambda: self.start_job([self.cli, "unindex", "--", folder], "Updating the index"))

    def start_job(self, arguments, title):
        if self.job:
            return
        self.cancel_search()
        self.job = title
        self.search_button.set_sensitive(False)
        self.progress.set_visible(True)
        started = time.monotonic()

        def progress():
            minutes, seconds = divmod(int(time.monotonic() - started), 60)
            self.feedback(f"{title} · {minutes}:{seconds:02d}")
            self.progress.pulse()
            return GLib.SOURCE_CONTINUE

        progress()
        timer = GLib.timeout_add(140, progress)

        def complete(output, error):
            GLib.source_remove(timer)
            self.job = None
            self.search_button.set_sensitive(True)
            self.progress.set_visible(False)
            if output is None:
                self.feedback(f"{title} failed.", error=True)
                self.alert("Semfind reported an error", error[-1600:])
            else:
                self.feedback(output.strip().splitlines()[-1] if output.strip() else "Done.")
                if self.active_query:
                    self.search()
            self.refresh_status()

        run_process(arguments, complete)


class SemfindApplication(Gtk.Application):
    def __init__(self, cli):
        super().__init__(application_id=APP_ID)
        self.cli = cli

    def do_activate(self):
        if not (window := self.get_active_window()):
            Gtk.Settings.get_default().set_property("gtk-application-prefer-dark-theme", True)
            css = Gtk.CssProvider()
            css.load_from_path(str(HERE / "style.css"))
            Gtk.StyleContext.add_provider_for_display(
                Gdk.Display.get_default(), css, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION,
            )
            window = SearchWindow(self, self.cli)
        window.present()
        window.entry.grab_focus()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    packaged = HERE.parents[1] / "bin/semfind"
    cli = str(packaged) if packaged.is_file() else shutil.which("semfind") or "semfind"
    parser.add_argument("--semfind", default=cli, help="Path to the Semfind CLI.")
    args = parser.parse_args()
    return SemfindApplication(args.semfind).run([sys.argv[0]])


if __name__ == "__main__":
    raise SystemExit(main())
