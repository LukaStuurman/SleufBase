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
        self.assertLessEqual(len(compact), 119)
        self.assertNotIn("Een tweede zin", compact)

    def test_settings_sections_use_flat_borderless_style_constant(self) -> None:
        self.assertEqual(settings_ui._SETTINGS_BG, "#f5f7fb")


if __name__ == "__main__":
    unittest.main()
