const { Gio, GLib, St } = imports.gi;

const BUS = "io.github.Engineer1080.Semfind";
const OBJECT = "/io/github/Engineer1080/Semfind/SearchProvider";
const INTERFACE = "org.gnome.Shell.SearchProvider2";
let activeSearch = null;

function call(method, input, output, cancellable, onSuccess) {
    Gio.DBus.session.call(BUS, OBJECT, INTERFACE, method, input,
        new GLib.VariantType(output), Gio.DBusCallFlags.NONE, 35000, cancellable,
        (connection, result) => {
            try {
                onSuccess(connection.call_finish(result).deep_unpack());
            } catch (error) {
                if (!cancellable || !cancellable.is_cancelled())
                    global.logError(error, "Semfind search provider");
            }
        });
}

function perform_search(pattern) {
    if (activeSearch)
        activeSearch.cancel();
    activeSearch = new Gio.Cancellable();
    const current = activeSearch;
    const send = this.send_results;
    const words = pattern.trim().split(/\s+/);
    if (!words[0]) {
        send([]);
        return;
    }

    call("GetInitialResultSet", new GLib.Variant("(as)", [words]), "(as)", current, ([ids]) => {
        if (current.is_cancelled())
            return;
        call("GetResultMetas", new GLib.Variant("(as)", [ids]), "(aa{sv})", current, ([metas]) => {
            if (current.is_cancelled())
                return;
            send(metas.filter(meta => meta.gicon).map(meta => ({
                id: meta.id.deep_unpack(),
                label: meta.clipboardText ? meta.clipboardText.deep_unpack() : meta.name.deep_unpack(),
                description: meta.description.deep_unpack(),
                icon: new St.Icon({ icon_name: meta.gicon.deep_unpack(), icon_size: 24 }),
                clipboard: meta.clipboardText ? meta.clipboardText.deep_unpack() : null,
            })));
        });
    });
}

function on_result_selected(result) {
    if (result.clipboard) {
        St.Clipboard.get_default().set_text(St.ClipboardType.CLIPBOARD, result.clipboard);
        return;
    }
    call("ActivateResult", new GLib.Variant("(sasu)", [result.id, [], 0]), "()", null, () => {});
}
