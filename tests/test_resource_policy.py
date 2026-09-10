from __future__ import annotations

import unittest

from SleufBase.resource_policy import HardwareSnapshot, build_resource_policy


_GIB = 1024**3


class ResourcePolicyTests(unittest.TestCase):
    def test_low_cost_laptop_stays_memory_safe(self) -> None:
        policy = build_resource_policy(
            HardwareSnapshot(
                logical_cpus=4,
                total_memory_bytes=8 * _GIB,
                available_memory_bytes=5 * _GIB,
            ),
            env={},
        )

        self.assertEqual(policy.mode, "auto")
        self.assertLessEqual(policy.network_workers, 8)
        self.assertLessEqual(policy.light_map_workers, 4)
        self.assertEqual(policy.heavy_raster_workers, 1)
        self.assertLessEqual(policy.tile_memory_cache_entries, 160)
        self.assertLessEqual(policy.wms_cache_bytes, 48 * 1024 * 1024)

    def test_high_end_workstation_scales_up(self) -> None:
        policy = build_resource_policy(
            HardwareSnapshot(
                logical_cpus=32,
                total_memory_bytes=128 * _GIB,
                available_memory_bytes=112 * _GIB,
            ),
            env={},
        )

        self.assertGreaterEqual(policy.network_workers, 16)
        self.assertGreaterEqual(policy.local_geometry_workers, 8)
        self.assertGreaterEqual(policy.light_map_workers, 8)
        self.assertGreaterEqual(policy.template_asset_workers, 8)
        self.assertEqual(policy.heavy_raster_workers, 4)
        self.assertGreaterEqual(policy.virtual_tiff_pixel_budget, 48_000_000)
        self.assertGreaterEqual(policy.wms_cache_bytes, 192 * 1024 * 1024)

    def test_global_worker_cap_is_respected(self) -> None:
        policy = build_resource_policy(
            HardwareSnapshot(
                logical_cpus=64,
                total_memory_bytes=256 * _GIB,
                available_memory_bytes=220 * _GIB,
            ),
            env={"SLEUFBASE_MAX_WORKERS": "3"},
        )

        self.assertEqual(policy.network_workers, 3)
        self.assertEqual(policy.local_geometry_workers, 3)
        self.assertEqual(policy.light_map_workers, 3)
        self.assertEqual(policy.template_asset_workers, 3)
        self.assertEqual(policy.heavy_raster_workers, 3)

    def test_specific_override_wins_before_global_cap(self) -> None:
        policy = build_resource_policy(
            HardwareSnapshot(
                logical_cpus=16,
                total_memory_bytes=64 * _GIB,
                available_memory_bytes=50 * _GIB,
            ),
            env={
                "SLEUFBASE_MAP_WORKERS": "7",
                "SLEUFBASE_RASTER_WORKERS": "2",
                "SLEUFBASE_MAX_WORKERS": "5",
            },
        )

        self.assertEqual(policy.light_map_workers, 5)
        self.assertEqual(policy.heavy_raster_workers, 2)

    def test_eco_mode_never_forces_parallel_heavy_raster(self) -> None:
        policy = build_resource_policy(
            HardwareSnapshot(
                logical_cpus=32,
                total_memory_bytes=128 * _GIB,
                available_memory_bytes=100 * _GIB,
            ),
            env={"SLEUFBASE_RESOURCE_MODE": "eco"},
        )

        self.assertEqual(policy.mode, "eco")
        self.assertEqual(policy.heavy_raster_workers, 1)
        self.assertLessEqual(policy.virtual_tiff_pixel_budget, 16_000_000)

    def test_invalid_mode_falls_back_to_auto(self) -> None:
        policy = build_resource_policy(
            HardwareSnapshot(8, 16 * _GIB, 12 * _GIB),
            env={"SLEUFBASE_RESOURCE_MODE": "turbo-potato"},
        )
        self.assertEqual(policy.mode, "auto")


if __name__ == "__main__":
    unittest.main()
