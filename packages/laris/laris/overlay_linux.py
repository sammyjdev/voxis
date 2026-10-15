"""Floating dictation indicator on Linux, run as a child process on the
SYSTEM Python (PyGObject does not build in the venv):
`/usr/bin/python3 -m laris.overlay_linux` with OVERLAY_URL set,
GDK_BACKEND=x11 and the laris package on PYTHONPATH.

Same contract as overlay_app.py on macOS: a transparent window hosting a
webview on the dashboard's orb page (/overlay), which subscribes to
/ws/voice on its own; stdin only says "show ..." / "hide"; stdin closing
(daemon died) ends the process.

GNOME Wayland gives a native client no always-on-top window, so this is an
X11 override-redirect popup under XWayland: mutter keeps it above every
window, at the position asked for, and never gives it focus - the paste
keystroke must keep landing in the user's app.
"""
from __future__ import annotations

import os
import sys
import threading

from laris import orb_theme

def window_rect_for_theme(theme: dict, monitor_x: int, monitor_y: int,
                          monitor_w: int, monitor_h: int) -> tuple[int, int, int]:
    """(x, y, size) of the orb window in X11 desktop coordinates. The theme
    speaks macOS's bottom-left origin; X11's origin is the top-left."""
    x, y_from_bottom = orb_theme.position_rect(theme, monitor_w, monitor_h)
    size = int(theme["size"])
    return monitor_x + int(x), monitor_y + int(monitor_h - y_from_bottom - size), size


def _ignore_input(window) -> bool:
    """Clicks fall through to the window below, as setIgnoresMouseEvents does
    on macOS. GTK keeps an input shape set through its own API and re-applies
    it on every map; one set on the X window from outside was reset by GTK's
    first map (seen 2026-10-14). The Python binding for it needs gi-cairo,
    which Ubuntu does not install by default, so call GTK directly with an
    empty cairo region. False when it could not be applied."""
    import ctypes

    try:
        gtk = ctypes.CDLL("libgtk-3.so.0")
        cairo = ctypes.CDLL("libcairo.so.2")
    except OSError:
        return False
    capsule_pointer = ctypes.pythonapi.PyCapsule_GetPointer
    capsule_pointer.restype = ctypes.c_void_p
    capsule_pointer.argtypes = [ctypes.py_object, ctypes.c_char_p]
    cairo.cairo_region_create.restype = ctypes.c_void_p
    cairo.cairo_region_destroy.argtypes = [ctypes.c_void_p]
    gtk.gtk_widget_input_shape_combine_region.argtypes = [ctypes.c_void_p, ctypes.c_void_p]

    widget = capsule_pointer(window.__gpointer__, None)
    if not widget:
        return False
    region = cairo.cairo_region_create()  # empty: no pixel takes input
    gtk.gtk_widget_input_shape_combine_region(widget, region)  # GTK keeps a copy
    cairo.cairo_region_destroy(region)
    return True


def main() -> None:
    import gi

    gi.require_version("Gdk", "3.0")
    gi.require_version("Gtk", "3.0")
    gi.require_version("WebKit2", "4.1")
    from gi.repository import Gdk, GLib, Gtk, WebKit2

    url = os.environ.get("OVERLAY_URL", "")
    if not url:
        print("OVERLAY_URL not set", file=sys.stderr)
        sys.exit(1)

    # POPUP = override-redirect: above every window, placed exactly, never focused
    window = Gtk.Window(type=Gtk.WindowType.POPUP)
    window.set_app_paintable(True)
    visual = window.get_screen().get_rgba_visual()
    if visual is not None:
        window.set_visual(visual)  # without it the transparent page paints black
    web = WebKit2.WebView()
    web.set_background_color(Gdk.RGBA(0, 0, 0, 0))
    # no disk cache: a stale voice-orb.js outlived two fixes on macOS
    web.get_context().set_cache_model(WebKit2.CacheModel.DOCUMENT_VIEWER)
    window.add(web)
    web.load_uri(url)  # load now, so the first show is not a blank window
    # before the window is ever shown, so no click can land on it
    click_through = _ignore_input(window)
    if not click_through:
        print("could not make the orb click-through - it stays hidden", file=sys.stderr)

    def reposition() -> None:
        """Read fresh each call: monitors and the theme change while the
        process lives on."""
        display = Gdk.Display.get_default()
        monitor = display.get_primary_monitor() or display.get_monitor(0)
        geometry = monitor.get_geometry()
        x, y, size = window_rect_for_theme(
            orb_theme.load_theme(), geometry.x, geometry.y, geometry.width, geometry.height)
        window.resize(size, size)
        window.move(x, y)

    def apply(command: str) -> bool:
        if command.startswith("show"):
            if not click_through:
                return False  # an orb that swallows clicks is worse than none
            reposition()
            window.show_all()
        elif command == "hide":
            window.hide()
        return False  # one-shot idle callback

    theme_path = orb_theme.theme_path()
    try:
        state = {"mtime": theme_path.stat().st_mtime}
    except OSError:  # missing, or replaced mid-read by an atomic theme save
        state = {"mtime": 0.0}

    def poll_theme() -> bool:
        try:
            mtime = theme_path.stat().st_mtime
        except OSError:
            return True
        if mtime != state["mtime"]:
            state["mtime"] = mtime
            reposition()
        return True  # keep the timer

    GLib.timeout_add_seconds(2, poll_theme)

    def read_stdin() -> None:
        for raw in sys.stdin:
            command = raw.strip()
            if command:
                GLib.idle_add(apply, command)
        GLib.idle_add(Gtk.main_quit)  # daemon closed the pipe: leave with it

    threading.Thread(target=read_stdin, daemon=True).start()
    Gtk.main()


if __name__ == "__main__":
    main()
