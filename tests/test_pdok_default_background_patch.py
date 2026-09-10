from __future__ import annotations

import unittest

from SleufBase.cyclomedia import CyclomediaAerialClient
from SleufBase.exporting import MapExporter
from SleufBase.pdok import PdokWmtsTileClient
from SleufBase.pdok_default_background_patch import (
    DEFAULT_BACKGROUND_LABEL,
    DEFAULT_BACKGROUND_LAYER,
)


class PdokDefaultBackgroundPatchTests(unittest.TestCase):
    def test_cyclomedia_default_is_replaced_by_actual_pdok_aerial(self) -> None:
        source = CyclomediaAerialClient.__new__(CyclomediaAerialClient)
        source.timeout = 17
        source.retries = 2
        exporter = MapExporter(source, renderer=object())

        provider = exporter.default_background_provider
        self.assertIsInstance(provider, PdokWmtsTileClient)
        self.assertEqual(provider.layer_name, DEFAULT_BACKGROUND_LAYER)
        self.assertEqual(provider.timeout, 17)
        self.assertEqual(provider.retries, 2)

    def test_explicit_non_cyclomedia_provider_is_preserved(self) -> None:
        explicit = object()
        exporter = MapExporter(explicit, renderer=object())
        self.assertIs(exporter.default_background_provider, explicit)

    def test_default_background_contract_is_exposed(self) -> None:
        self.assertEqual(DEFAULT_BACKGROUND_LAYER, "Actueel_orthoHR")
        self.assertEqual(DEFAULT_BACKGROUND_LABEL, "Luchtfoto NL actueel (8 cm, deels 5 cm)")
        self.assertEqual(MapExporter.SLEUFBASE_DEFAULT_BACKGROUND_LAYER, DEFAULT_BACKGROUND_LAYER)
        self.assertEqual(MapExporter.SLEUFBASE_DEFAULT_BACKGROUND_LABEL, DEFAULT_BACKGROUND_LABEL)
        self.assertGreaterEqual(
            int(getattr(MapExporter, "_sleufbase_pdok_default_background_version", 0) or 0),
            2,
        )


if __name__ == "__main__":
    unittest.main()
