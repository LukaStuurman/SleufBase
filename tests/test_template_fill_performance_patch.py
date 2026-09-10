from __future__ import annotations

from types import SimpleNamespace
import unittest

from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase import template_export_performance_patch as template_perf
from SleufBase.resource_policy import get_resource_policy
from SleufBase.template_fill_performance_patch import (
    MAX_PARALLEL_TEMPLATE_ASSETS,
    MAX_PARALLEL_TEMPLATE_MAPS,
)


class _FakeLayer:
    def __init__(self, color: int, lineweight: int) -> None:
        self.dxf = SimpleNamespace(color=color, lineweight=lineweight)


class _FakeLayers:
    def __init__(self) -> None:
        self.items: dict[str, _FakeLayer] = {}
        self.contains_calls = 0
        self.get_calls = 0
        self.add_calls = 0

    def __contains__(self, name: str) -> bool:
        self.contains_calls += 1
        return name in self.items

    def get(self, name: str) -> _FakeLayer:
        self.get_calls += 1
        return self.items[name]

    def add(self, name: str, dxfattribs: dict[str, int]) -> _FakeLayer:
        self.add_calls += 1
        layer = _FakeLayer(dxfattribs["color"], dxfattribs["lineweight"])
        self.items[name] = layer
        return layer


class _FakeDocument:
    def __init__(self) -> None:
        self.layers = _FakeLayers()


class TemplateFillPerformancePatchTests(unittest.TestCase):
    def test_template_parallelism_matches_resolved_resource_policy(self) -> None:
        policy = get_resource_policy()
        self.assertEqual(MAX_PARALLEL_TEMPLATE_MAPS, policy.light_map_workers)
        self.assertEqual(MAX_PARALLEL_TEMPLATE_ASSETS, policy.template_asset_workers)
        self.assertEqual(
            template_perf.MAX_VIRTUAL_TEMPLATE_MAP_WORKERS,
            policy.light_map_workers,
        )
        self.assertEqual(
            CadastralDxfExporter.SLEUFBASE_TEMPLATE_MAP_WORKERS,
            policy.light_map_workers,
        )
        self.assertEqual(
            CadastralDxfExporter.SLEUFBASE_TEMPLATE_ASSET_WORKERS,
            policy.template_asset_workers,
        )
        self.assertEqual(
            CadastralDxfExporter._template_asset_worker_count(999),
            policy.template_asset_workers,
        )

    def test_profile_layer_table_work_is_compacted_per_unique_layer(self) -> None:
        exporter = CadastralDxfExporter(wfs_client=object())
        document = _FakeDocument()
        points = tuple(
            [SimpleNamespace(layer_name="LS", color=1) for _ in range(80)]
            + [SimpleNamespace(layer_name="WATER", color=5) for _ in range(60)]
            + [SimpleNamespace(layer_name="0", color=7) for _ in range(20)]
        )

        exporter._ensure_template_profile_layers(document, points)
        self.assertEqual(document.layers.add_calls, 2)
        self.assertEqual(document.layers.contains_calls, 2)
        self.assertEqual(document.layers.get_calls, 0)

        # A second profile using the same styles should not touch the DXF layer
        # table at all; the per-document effective-style cache can answer it.
        exporter._ensure_template_profile_layers(document, points)
        self.assertEqual(document.layers.add_calls, 2)
        self.assertEqual(document.layers.contains_calls, 2)
        self.assertEqual(document.layers.get_calls, 0)

    def test_profile_layer_color_change_preserves_legacy_last_style(self) -> None:
        exporter = CadastralDxfExporter(wfs_client=object())
        document = _FakeDocument()
        points = (
            SimpleNamespace(layer_name="LS", color=1),
            SimpleNamespace(layer_name="LS", color=3),
        )
        exporter._ensure_template_profile_layers(document, points)
        self.assertEqual(document.layers.items["LS"].dxf.color, 3)

        exporter._ensure_template_profile_layers(
            document,
            (SimpleNamespace(layer_name="LS", color=6),),
        )
        self.assertEqual(document.layers.get_calls, 1)
        self.assertEqual(document.layers.items["LS"].dxf.color, 6)

    def test_profile_fill_patch_is_installed_after_pipeline_v7(self) -> None:
        self.assertGreaterEqual(
            int(getattr(CadastralDxfExporter, "_sleufbase_template_fill_performance_version", 0) or 0),
            2,
        )
        self.assertTrue(CadastralDxfExporter.SLEUFBASE_PROFILE_LAYER_CACHE)
        self.assertTrue(CadastralDxfExporter.SLEUFBASE_PROFILE_LEADER_BLOCK_CACHE)
        self.assertTrue(CadastralDxfExporter.SLEUFBASE_REVERSE_PROFILE_INDEPENDENT_BUILD)
        self.assertFalse(CadastralDxfExporter.SLEUFBASE_REVERSE_PROFILE_REUSE)


if __name__ == "__main__":
    unittest.main()
