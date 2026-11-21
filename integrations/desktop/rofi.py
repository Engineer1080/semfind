"""Rofi script mode for the shared Semfind search provider."""

import os
import sys

from gi.repository import Gio, GLib

from provider import BUS, GNOME, ROOT


def call(method, signature, *arguments):
    return Gio.bus_get_sync(Gio.BusType.SESSION, None).call_sync(
        BUS, ROOT + "/SearchProvider", GNOME, method,
        GLib.Variant(signature, arguments), None, Gio.DBusCallFlags.NONE, 35000, None,
    ).unpack()


def line(value):
    return " ".join("".join(" " if ord(char) < 32 or ord(char) == 127 else char
                            for char in value).split())


def notice(message):
    print(f"{message}\0nonselectable\x1ftrue")


def main():
    state = os.environ.get("ROFI_RETV", "0")
    if state == "1":
        if key := os.environ.get("ROFI_INFO"):
            call("ActivateResult", "(sasu)", key, [], 0)
        return

    print("\0prompt\x1fSemfind")
    query = sys.argv[1].split() if state == "2" and len(sys.argv) > 1 else []
    if not query:
        notice("Type a question and press Enter.")
        return

    ids, = call("GetInitialResultSet", "(as)", query)
    results, = call("GetResultMetas", "(as)", ids)
    for hit in results:
        if "gicon" not in hit:
            continue
        if hit["gicon"] == "utilities-terminal":
            example = hit.get("clipboardText", hit["name"])
            print(f'{line(example)}\0icon\x1futilities-terminal\x1fnonselectable\x1ftrue')
        else:
            details = line(hit["description"])
            label = line(hit["name"])
            if details:
                label += f"  —  {details}"
            print(f'{label}\0icon\x1f{line(hit["gicon"])}\x1finfo\x1f{line(hit["id"])}')
    if not results:
        notice("No results.")


if __name__ == "__main__":
    try:
        main()
    except GLib.Error as error:
        print(f"semfind-rofi: {error}", file=sys.stderr)
        if os.environ.get("ROFI_RETV") != "1":
            notice("Semfind search is unavailable.")
