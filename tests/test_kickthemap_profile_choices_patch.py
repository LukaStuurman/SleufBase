from __future__ import annotations

import unittest

from SleufBase.kickthemap_profile_choices_patch import merge_profile_dropdown_options
from SleufBase.settings import normalize_kickthemap_profile_extra_choices


class KickTheMapProfileChoicesTests(unittest.TestCase):
    def test_normalize_removes_empty_and_duplicate_choices(self) -> None:
        self.assertEqual(
            normalize_kickthemap_profile_extra_choices(
                ["  Data  ", "", "data", "Water", None, " water "]
            ),
            ["Data", "Water"],
        )

    def test_extra_choices_extend_existing_dropdown_without_duplicates(self) -> None:
        base = [
            {"label": "Datakabel", "code": "DA", "keywords": "DA,data"},
            {"label": "Waterleiding", "code": "WA", "keywords": "WA,water"},
        ]
        merged = merge_profile_dropdown_options(
            base,
            ["Extra kabel", "DA", "datakabel", "Nieuwe leiding"],
        )

        self.assertEqual(
            merged,
            [
                {"label": "Datakabel", "code": "DA", "keywords": "DA,data"},
                {"label": "Waterleiding", "code": "WA", "keywords": "WA,water"},
                {
                    "label": "Extra kabel",
                    "code": "Extra kabel",
                    "keywords": "Extra kabel",
                },
                {
                    "label": "Nieuwe leiding",
                    "code": "Nieuwe leiding",
                    "keywords": "Nieuwe leiding",
                },
            ],
        )

    def test_base_input_is_not_mutated(self) -> None:
        base = [{"label": "Gas", "code": "GA", "keywords": "GA"}]
        merged = merge_profile_dropdown_options(base, ["Extra"])
        merged[0]["label"] = "Gewijzigd"
        self.assertEqual(base[0]["label"], "Gas")


if __name__ == "__main__":
    unittest.main()
