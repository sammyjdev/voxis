"""Floating dictation indicator, run as a child process:
`python -m laris.overlay_app` with OVERLAY_URL set.

A transparent non-activating panel hosting a WKWebView that loads the
dashboard's own orb page (/overlay) - identical visuals, same
/ws/voice subscription. Stdin only controls window visibility
("show ..." / "hide"); phases and captions arrive over the websocket.
Exits when stdin closes (daemon died).

Window work only - needs no TCC permission.
"""
from __future__ import annotations

import os
import sys
import threading

from laris import orb_theme


def panel_rect_for_theme(theme: dict, screen_w: float, screen_h: float,
                         origin_x: float = 0.0,
                         origin_y: float = 0.0) -> tuple[float, float, float, float]:
    """(x, y, w, h) for the overlay panel - pure, testable. origin_x/y is the
    target screen's frame origin: on multi-monitor a secondary display sits at
    a non-zero offset, and without it the orb lands on the primary screen."""
    x, y = orb_theme.position_rect(theme, screen_w, screen_h)
    size = float(theme["size"])
    return origin_x + x, origin_y + y, size, size


def main() -> None:
    import AppKit
    import WebKit

    url = os.environ.get("OVERLAY_URL", "")
    if not url:
        print("OVERLAY_URL not set", file=sys.stderr)
        sys.exit(1)

    app = AppKit.NSApplication.sharedApplication()
    app.setActivationPolicy_(AppKit.NSApplicationActivationPolicyAccessory)

    theme = orb_theme.load_theme()
    frame = AppKit.NSScreen.mainScreen().frame()
    x, y, w, h = panel_rect_for_theme(
        theme, frame.size.width, frame.size.height, frame.origin.x, frame.origin.y)
    rect = AppKit.NSMakeRect(x, y, w, h)

    panel = AppKit.NSPanel.alloc().initWithContentRect_styleMask_backing_defer_(
        rect,
        AppKit.NSWindowStyleMaskBorderless | AppKit.NSWindowStyleMaskNonactivatingPanel,
        AppKit.NSBackingStoreBuffered,
        False,
    )
    panel.setLevel_(AppKit.NSStatusWindowLevel)
    panel.setOpaque_(False)
    panel.setBackgroundColor_(AppKit.NSColor.clearColor())
    panel.setHasShadow_(False)
    panel.setIgnoresMouseEvents_(True)
    panel.setCollectionBehavior_(
        AppKit.NSWindowCollectionBehaviorCanJoinAllSpaces
        | AppKit.NSWindowCollectionBehaviorFullScreenAuxiliary
    )

    config = WebKit.WKWebViewConfiguration.alloc().init()
    web = WebKit.WKWebView.alloc().initWithFrame_configuration_(
        AppKit.NSMakeRect(0, 0, w, h), config
    )
    try:
        # KVC switch WebKit ships but doesn't expose as API; without it the
        # webview paints a white page background behind the orb
        web.setValue_forKey_(False, "drawsBackground")
    except Exception:  # noqa: BLE001 - fall back to underPageBackgroundColor only
        pass
    try:
        web.setUnderPageBackgroundColor_(AppKit.NSColor.clearColor())
    except Exception:  # noqa: BLE001 - older macOS without the setter
        pass
    try:
        web.setBackgroundColor_(AppKit.NSColor.clearColor())
    except Exception:  # noqa: BLE001
        pass
    try:
        web.setOpaque_(False)
    except Exception:  # noqa: BLE001
        pass
    panel.setContentView_(web)

    # bypass the webview cache: stale voice-orb.js kept an old opaque-canvas
    # bug alive through two fix attempts during live debugging
    request = AppKit.NSURLRequest.requestWithURL_cachePolicy_timeoutInterval_(
        AppKit.NSURL.URLWithString_(url),
        AppKit.NSURLRequestReloadIgnoringLocalCacheData,
        15.0,
    )
    web.loadRequest_(request)

    def reposition() -> None:
        """Re-place the panel on the CURRENT main screen. Read fresh each call
        so unplugging/rearranging monitors after launch (the panel is spawned
        once and lives on) doesn't strand the orb on a screen that's gone."""
        f = AppKit.NSScreen.mainScreen().frame()
        rx, ry, rw, rh = panel_rect_for_theme(
            orb_theme.load_theme(), f.size.width, f.size.height, f.origin.x, f.origin.y)
        panel.setFrame_display_(AppKit.NSMakeRect(rx, ry, rw, rh), True)
        web.setFrame_(AppKit.NSMakeRect(0, 0, rw, rh))

    theme_path = orb_theme.theme_path()
    try:
        state = {"mtime": theme_path.stat().st_mtime}
    except OSError:
        state = {"mtime": 0.0}

    def _poll_theme() -> None:
        try:
            mtime = theme_path.stat().st_mtime
        except OSError:
            return
        if mtime == state["mtime"]:
            return
        state["mtime"] = mtime
        reposition()

    AppKit.NSTimer.scheduledTimerWithTimeInterval_repeats_block_(
        2.0, True, lambda _timer: _poll_theme()
    )

    def apply(command: str) -> None:
        if command.startswith("show"):
            reposition()  # monitor may have changed since the panel was built
            panel.orderFrontRegardless()
        elif command == "hide":
            panel.orderOut_(None)

    def read_stdin() -> None:
        for raw in sys.stdin:
            command = raw.strip()
            if not command:
                continue
            AppKit.NSOperationQueue.mainQueue().addOperationWithBlock_(
                lambda cmd=command: apply(cmd)
            )
        # daemon closed the pipe: leave with it
        AppKit.NSOperationQueue.mainQueue().addOperationWithBlock_(
            lambda: app.terminate_(None)
        )

    threading.Thread(target=read_stdin, daemon=True).start()
    app.run()


if __name__ == "__main__":
    main()
