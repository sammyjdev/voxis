from laris import orb_theme
from laris.overlay_app import panel_rect_for_theme


def test_panel_rect_uses_theme_position_and_size():
    theme = dict(orb_theme.DEFAULTS, position="bottom-right", size=100)
    x, y, w, h = panel_rect_for_theme(theme, 1000.0, 800.0)
    assert (x, y, w, h) == (1000 - 100 - 16.0, 16.0, 100.0, 100.0)


def test_panel_rect_offsets_by_screen_origin():
    """A secondary/external monitor has a non-zero frame origin; the panel
    corner must be translated by it or the orb lands on the primary display."""
    theme = dict(orb_theme.DEFAULTS, position="center", size=100)
    x, y, w, h = panel_rect_for_theme(theme, 1920.0, 1080.0, origin_x=1920.0, origin_y=-200.0)
    assert (x, y, w, h) == (1920 + (1920 - 100) / 2, -200 + (1080 - 100) / 2, 100.0, 100.0)
