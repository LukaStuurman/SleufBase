from __future__ import annotations

from pathlib import Path
import unittest

from PIL import Image

from SleufBase import cadastral_export as cadastral_export_module
from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.marxact_boundary_patch import (
    PATCH_VERSION,
    VIRTUAL_TRENCH_BOUNDARY_3D_KEY,
)
from SleufBase.marxact_import import MarXactObject, MarXactTrench, build_marxact_virtual_layer
from SleufBase.models import Bounds, GeoTiffLayer, GeoTransform
from SleufBase.virtual_trench import (
    VIRTUAL_TRENCH_METADATA_KEY,
    virtual_trench_boundary_3d,
    virtual_trench_polygon,
)


class MarXactBoundaryPatchTests(unittest.TestCase):
    @staticmethod
    def _marxact_layer():
        trench = MarXactTrench(
            name="ps-irregular",
            polygon=(
                (100.0, 200.0, 5.10),
                (106.0, 200.8, 5.25),
                (105.2, 202.4, 5.40),
                (102.0, 203.2, 5.55),
                (99.2, 201.4, 5.30),
            ),
            objects=[
                MarXactObject(101.0, 201.0, 4.2, "Water", "water", 4.2, "marxact_point"),
                MarXactObject(104.0, 201.5, 4.1, "Laagspanning", "ls", 4.1, "marxact_point"),
            ],
        )
        return trench, build_marxact_virtual_layer(
            trench,
            source_path="irregular.dxf",
            source_name_resolver=lambda item: item.mapping_name,
            fallback_index=1,
        )

    def test_runtime_patch_is_installed(self) -> None:
        self.assertGreaterEqual(
            int(getattr(CadastralDxfExporter, "_sleufbase_marxact_boundary_patch_version", 0) or 0),
            PATCH_VERSION,
        )

    def test_marxact_uses_exact_measured_polygon_instead_of_generated_rectangle(self) -> None:
        trench, layer = self._marxact_layer()
        expected_xy = [(x, y) for x, y, _z in trench.polygon]
        self.assertEqual(virtual_trench_polygon(layer), expected_xy)
        self.assertEqual(cadastral_export_module.virtual_trench_polygon(layer), expected_xy)
        self.assertEqual(len(virtual_trench_polygon(layer)), 5)

    def test_marxact_boundary_keeps_z_and_is_promoted_to_generic_payload(self) -> None:
        trench, layer = self._marxact_layer()
        boundary = virtual_trench_boundary_3d(layer)
        self.assertEqual(boundary, list(trench.polygon))

        payload = layer.metadata[VIRTUAL_TRENCH_METADATA_KEY]
        self.assertIn(VIRTUAL_TRENCH_BOUNDARY_3D_KEY, payload)
        self.assertEqual(
            payload[VIRTUAL_TRENCH_BOUNDARY_3D_KEY],
            [[x, y, z] for x, y, z in trench.polygon],
        )

    def test_generic_boundary_3d_takes_precedence_over_legacy_marxact_field(self) -> None:
        _trench, layer = self._marxact_layer()
        generic = [
            [0.0, 0.0, 1.0],
            [5.0, 0.0, 1.1],
            [4.0, 2.0, 1.2],
            [0.0, 1.0, 1.3],
        ]
        layer.metadata[VIRTUAL_TRENCH_METADATA_KEY][VIRTUAL_TRENCH_BOUNDARY_3D_KEY] = generic
        self.assertEqual(
            virtual_trench_polygon(layer),
            [(0.0, 0.0), (5.0, 0.0), (4.0, 2.0), (0.0, 1.0)],
        )

    def test_normal_virtual_trench_without_measured_boundary_keeps_rectangle_fallback(self) -> None:
        layer = GeoTiffLayer(
            path=Path("normal-virtual.tif"),
            image=Image.new("RGBA", (4, 4), (0, 0, 0, 0)),
            transform=GeoTransform(1.0, 0.0, -1.0, 0.0, -1.0, 1.0),
            bounds=Bounds(-1.0, -1.0, 5.0, 1.0),
            epsg=28992,
            opacity=1.0,
            metadata={
                VIRTUAL_TRENCH_METADATA_KEY: {
                    "width_meters": 0.6,
                    "points": [
                        {"role": "start", "x": 0.0, "y": 0.0, "z": 1.0},
                        {"role": "end", "x": 4.0, "y": 0.0, "z": 1.1},
                    ],
                }
            },
        )
        self.assertEqual(virtual_trench_boundary_3d(layer), [])
        self.assertEqual(
            virtual_trench_polygon(layer),
            [(0.0, 0.3), (0.0, -0.3), (4.0, -0.3), (4.0, 0.3)],
        )


if __name__ == "__main__":
    unittest.main()
