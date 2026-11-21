"""D-Bus search adapter for Plasma 6 and GNOME Shell."""

from collections import OrderedDict
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys

from gi.repository import Gio, GLib

BUS = "io.github.Engineer1080.Semfind"
ROOT = "/io/github/Engineer1080/Semfind"
KDE = "org.kde.krunner1"
GNOME = "org.gnome.Shell.SearchProvider2"
INTERFACES = {
    KDE: {
        "Match": (["s"], "a(sssuda{sv})"),
        "Actions": ([], "a(sss)"),
        "Run": (["s", "s"], ""),
    },
    GNOME: {
        "GetInitialResultSet": (["as"], "as"),
        "GetSubsearchResultSet": (["as", "as"], "as"),
        "GetResultMetas": (["as"], "aa{sv}"),
        "ActivateResult": (["s", "as", "u"], ""),
        "LaunchSearch": (["as", "u"], ""),
    },
}
ICONS = {
    "file": "text-x-generic",
    "application": "applications-other",
    "command": "utilities-terminal",
}


def interface_info(name):
    methods = []
    for method, (inputs, output) in INTERFACES[name].items():
        args = "".join(f'<arg type="{value}" direction="in"/>' for value in inputs)
        if output:
            args += f'<arg type="{output}" direction="out"/>'
        methods.append(f'<method name="{method}">{args}</method>')
    return Gio.DBusNodeInfo.new_for_xml(
        f'<node><interface name="{name}">{"".join(methods)}</interface></node>'
    ).interfaces[0]


def variant(value):
    return GLib.Variant("s", value)


def reply(call, signature="", *values):
    call.return_value(GLib.Variant(f"({signature})", values))


class Provider:
    def __init__(self, config):
        self.cli = config["semfind"]
        self.env = dict(os.environ, **config["environment"])
        self.worker = ThreadPoolExecutor(max_workers=1)
        self.pending = None
        self.timer = 0
        self.running = False
        self.hits = OrderedDict()

    def search(self, query):
        result = subprocess.run(
            [self.cli, "search", "--json", "--limit", "32", "--no-gpu-hint", "--", query],
            env=self.env, stdout=subprocess.PIPE, timeout=30, check=True,
        )
        return [hit for hit in json.loads(result.stdout) if hit["category"] in ICONS][:8]

    def enqueue(self, call, interface, query):
        signature = INTERFACES[interface][call.get_method_name()][1]
        if not query or len(query.encode("utf-8")) > 2048:
            reply(call, signature, [])
            return
        if self.pending:
            old_call, old_interface, _ = self.pending
            reply(old_call, INTERFACES[old_interface][old_call.get_method_name()][1], [])
        self.pending = call, interface, query
        if self.timer:
            GLib.source_remove(self.timer)
            self.timer = 0
        if not self.running:
            self.timer = GLib.timeout_add(150, self.start_search)

    def start_search(self):
        self.timer = 0
        call, interface, query = self.pending
        self.pending = None
        self.running = True
        self.worker.submit(self.search, query).add_done_callback(
            lambda result: GLib.idle_add(self.finish_search, call, interface, result)
        )
        return GLib.SOURCE_REMOVE

    def finish_search(self, call, interface, result):
        self.running = False
        hits = []
        try:
            hits = result.result()
        except (OSError, subprocess.SubprocessError, ValueError) as error:
            print(f"semfind-desktop: search failed ({type(error).__name__})", file=sys.stderr)
        if self.pending:
            hits = []
        ids = []
        for hit in hits:
            key = hashlib.sha256(hit["id"].encode()).hexdigest()
            self.hits[key] = hit
            self.hits.move_to_end(key)
            ids.append(key)
        while len(self.hits) > 256:
            self.hits.popitem(last=False)
        if interface == KDE:
            reply(call, "a(sssuda{sv})", [
                (key, hit["title"], ICONS[hit["category"]], 50,
                 1.0 - rank / 16, {"subtext": variant(self.description(hit))})
                for rank, (key, hit) in enumerate(zip(ids, hits))
            ])
        else:
            reply(call, "as", ids)
        if self.pending:
            self.timer = GLib.timeout_add(150, self.start_search)
        return GLib.SOURCE_REMOVE

    @staticmethod
    def description(hit):
        return " ".join((hit.get("example") or hit.get("snippet") or "").split())[:200]

    def metadata(self, key):
        hit = self.hits.get(key)
        if hit is None:
            return {"id": variant(key), "name": variant("Semfind")}
        result = {
            "id": variant(key),
            "name": variant(hit["title"]),
            "description": variant(self.description(hit)),
            "gicon": variant(ICONS[hit["category"]]),
        }
        if hit["category"] == "command" and hit.get("example"):
            result["clipboardText"] = variant(hit["example"])
        return result

    def activate(self, key):
        hit = self.hits.get(key)
        if not hit:
            return
        if hit["category"] == "file":
            path = Path(hit["path"])
            if path.is_absolute() and not path.is_symlink() and path.is_file():
                Gio.AppInfo.launch_default_for_uri(path.as_uri())
        elif hit["category"] == "application" and Path(hit["id"]).name == hit["id"]:
            app = Gio.DesktopAppInfo.new(hit["id"])
            if app:
                app.launch([], None)

    def launch_terminal(self, query):
        command = " ".join(GLib.shell_quote(value) for value in
                           (sys.executable, str(Path(__file__).resolve()), "--show", query))
        app = Gio.AppInfo.create_from_commandline(
            command.replace("%", "%%"), "Semfind", Gio.AppInfoCreateFlags.NEEDS_TERMINAL
        )
        app.launch([], None)

    def dispatch(self, connection, sender, path, interface, method, parameters, call):
        args = parameters.unpack()
        try:
            if method == "Match":
                words = args[0].split(maxsplit=1)
                query = words[1] if len(words) == 2 and words[0].lower() == "semfind" else ""
                self.enqueue(call, interface, query)
            elif method in ("GetInitialResultSet", "GetSubsearchResultSet"):
                self.enqueue(call, interface, " ".join(args[-1]).strip())
            elif method == "GetResultMetas":
                reply(call, "aa{sv}", [self.metadata(key) for key in args[0]])
            elif method == "Actions":
                reply(call, "a(sss)", [])
            else:
                if method == "LaunchSearch":
                    self.launch_terminal(" ".join(args[0]).strip())
                elif method == "ActivateResult" or not args[1]:
                    self.activate(args[0])
                reply(call)
        except (GLib.Error, OSError, ValueError) as error:
            call.return_dbus_error(f"{BUS}.Failed", str(error))


def main():
    config = json.loads(Path(__file__).with_name("config.json").read_text())
    if sys.argv[1:2] == ["--show"]:
        query = sys.argv[2] if len(sys.argv) > 2 else ""
        args = ["search", "--", query] if query else ["--help"]
        subprocess.run([config["semfind"], *args], env=dict(os.environ, **config["environment"]))
        if sys.stdin.isatty():
            input("Press Enter to close Semfind. ")
        return

    provider = Provider(config)
    loop = GLib.MainLoop()
    paths = {KDE: ROOT + "/Runner", GNOME: ROOT + "/SearchProvider"}

    def register(connection, name):
        for interface, path in paths.items():
            connection.register_object(path, interface_info(interface), provider.dispatch, None, None)

    owner = Gio.bus_own_name(Gio.BusType.SESSION, BUS, Gio.BusNameOwnerFlags.NONE,
                            register, None, lambda *_: loop.quit())
    GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, signal.SIGTERM, loop.quit)
    try:
        loop.run()
    finally:
        Gio.bus_unown_name(owner)
        provider.worker.shutdown(cancel_futures=True)


if __name__ == "__main__":
    main()
