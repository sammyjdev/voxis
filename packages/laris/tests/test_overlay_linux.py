"""Where the Linux orb window goes. The theme speaks macOS coordinates
(origin at the bottom-left of the screen); X11 puts the origin at the top-left
of the whole desktop, so the same theme must land in the same visual spot."""
from laris import orb_theme
from laris.overlay_linux import window_rect_for_theme


def _theme(position, size=100):
    return dict(orb_theme.DEFAULTS, position=position, size=size)


def test_a_bottom_center_orb_sits_16_px_above_the_bottom_edge():
    x, y, size = window_rect_for_theme(_theme("bottom-center"), 0, 0, 1000, 800)

    assert (x, y, size) == (450, 800 - 100 - 16, 100)


def test_a_top_right_orb_sits_40_px_below_the_top_edge():
    x, y, size = window_rect_for_theme(_theme("top-right"), 0, 0, 1000, 800)

    assert (x, y, size) == (1000 - 100 - 16, 40, 100)


def test_the_orb_lands_on_a_monitor_to_the_right_of_another():
    """The primary monitor here starts at x=1920: without its origin the orb
    would land on the other screen."""
    x, y, size = window_rect_for_theme(_theme("center"), 1920, 0, 1920, 1080)

    assert (x, y, size) == (1920 + (1920 - 100) // 2, (1080 - 100) // 2, 100)
