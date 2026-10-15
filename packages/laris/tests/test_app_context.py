from unittest.mock import MagicMock, patch


def test_current_bundle_id_returns_string_on_success():
    from laris import app_context

    fake_app = MagicMock()
    fake_app.bundleIdentifier.return_value = "com.microsoft.VSCode"
    fake_workspace = MagicMock()
    fake_workspace.frontmostApplication.return_value = fake_app

    with patch.object(app_context, "_get_workspace", return_value=fake_workspace):
        result = app_context.current_bundle_id()

    assert result == "com.microsoft.VSCode"


def test_current_bundle_id_returns_none_on_failure():
    from laris import app_context

    with patch.object(app_context, "_get_workspace", side_effect=Exception("no Cocoa")):
        result = app_context.current_bundle_id()

    assert result is None


def test_current_format_mode_code_for_vscode():
    from laris import app_context

    with patch.object(app_context, "current_bundle_id", return_value="com.microsoft.VSCode"):
        assert app_context.current_format_mode() == "code"


def test_current_format_mode_code_for_warp():
    from laris import app_context

    with patch.object(app_context, "current_bundle_id", return_value="dev.warp.Warp-Stable"):
        assert app_context.current_format_mode() == "code"


def test_current_format_mode_prose_for_slack():
    from laris import app_context

    with patch.object(app_context, "current_bundle_id", return_value="com.tinyspeck.slackmacgap"):
        assert app_context.current_format_mode() == "prose"


def test_current_format_mode_prose_for_unknown_app():
    from laris import app_context

    with patch.object(app_context, "current_bundle_id", return_value="com.unknown.app"):
        assert app_context.current_format_mode() == "prose"


def test_current_format_mode_prose_when_bundle_id_none():
    from laris import app_context

    with patch.object(app_context, "current_bundle_id", return_value=None):
        assert app_context.current_format_mode() == "prose"


def test_format_for_focus_code_mode_passes_through():
    from laris import app_context

    with patch.object(app_context, "current_format_mode", return_value="code"):
        result = app_context.format_for_focus("hello world dot")

    assert result == "hello world dot"


def test_format_for_focus_prose_capitalizes_first_letter():
    from laris import app_context

    with patch.object(app_context, "current_format_mode", return_value="prose"):
        result = app_context.format_for_focus("hello world")

    assert result == "Hello world."


def test_format_for_focus_prose_appends_period_if_missing():
    from laris import app_context

    with patch.object(app_context, "current_format_mode", return_value="prose"):
        result = app_context.format_for_focus("isso e um teste")

    assert result.endswith(".")


def test_format_for_focus_prose_preserves_existing_punctuation():
    from laris import app_context

    with patch.object(app_context, "current_format_mode", return_value="prose"):
        result = app_context.format_for_focus("ja tem ponto.")

    assert result == "Ja tem ponto."


def test_format_for_focus_prose_no_period_after_question_mark():
    from laris import app_context

    with patch.object(app_context, "current_format_mode", return_value="prose"):
        result = app_context.format_for_focus("tudo bem?")

    assert result == "Tudo bem?"
    assert not result.endswith("?.")


def test_format_for_focus_prose_no_period_after_newline():
    from laris import app_context

    with patch.object(app_context, "current_format_mode", return_value="prose"):
        result = app_context.format_for_focus("primeira linha\nsegunda")

    assert result == "Primeira linha\nsegunda"


def test_prose_formatting_keeps_the_space_before_a_follow_up_utterance():
    """Hands-free pastes " pode seguir" after an earlier sentence; the space
    stays in front and the first letter still becomes a capital."""
    from laris import app_context

    with patch.object(app_context, "current_format_mode", return_value="prose"):
        assert app_context.format_for_focus(" pode seguir") == " Pode seguir."
