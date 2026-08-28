from __future__ import annotations

import unittest

from SleufBase import settings_ui


class SettingsUiTests(unittest.TestCase):
    def test_kickthemap_sections_have_distinct_compact_titles(self) -> None:
        self.assertEqual(
            settings_ui._SECTION_TITLES["KickTheMap kabeltype, materiaal en DXF-laag"],
            "KickTheMap – kabels",
        )
        self.assertEqual(
            settings_ui._SECTION_TITLES["KickTheMap woord-naar-laag"],
            "KickTheMap – woorden",
        )

    def test_help_text_is_kept_short(self) -> None:
        source = (
            "Dit is een lange toelichting die in de instellingen niet als een groot tekstblok "
            "moet verschijnen maar compact en snel leesbaar moet blijven voor de gebruiker. "
            "Een tweede zin hoort niet nodig te zijn."
        )
        compact = settings_ui._compact_help_text(source)
        self.assertLessEqual(len(compact), 113)
        self.assertNotIn("Een tweede zin", compact)

    def test_settings_content_is_white_instead_of_gray_text_tiles(self) -> None:
        self.assertEqual(settings_ui._WINDOW_BG, "#f5f7fb")
        self.assertEqual(settings_ui._SETTINGS_BG, "#ffffff")

    def test_words_list_has_dedicated_view_and_launcher_labels(self) -> None:
        self.assertEqual(settings_ui._WORDS_DISPLAY_TITLE, "KickTheMap – woorden")
        self.assertEqual(settings_ui._WORDS_VIEW_TITLE, "KickTheMap – woordenlijst")
        self.assertTrue(callable(settings_ui._install_words_launcher))
        self.assertTrue(callable(settings_ui._open_words_view))
        self.assertTrue(callable(settings_ui._close_words_view))


if __name__ == "__main__":
    unittest.main()
