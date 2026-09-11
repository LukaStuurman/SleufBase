from __future__ import annotations

from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

import ezdxf

from SleufBase.kickthemap_dxf_export import (
    APP_ID,
    KickTheMapDxfExporter,
    KickTheMapObjectDataset,
    KickTheMapObjectPolyline,
    KickTheMapPolylineVertex,
)


class KickTheMapDxfPolylineTests(unittest.TestCase):
    def test_export_includes_polyline_only_dataset(self) -> None:
        with TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            dataset = KickTheMapObjectDataset(
                job_id=123,
                job_title="PS63",
                source_path=temp_path / "123_jobFeatures.json",
                points=(),
                polylines=(
                    KickTheMapObjectPolyline(
                        object_name="Waterleiding",
                        source_name="water",
                        vertices=(
                            KickTheMapPolylineVertex(100.0, 200.0, 1.0),
                            KickTheMapPolylineVertex(101.0, 202.0, 1.5),
                            KickTheMapPolylineVertex(103.0, 203.0, None),
                        ),
                    ),
                ),
            )
            output_path = temp_path / "objects.dxf"

            KickTheMapDxfExporter().export(output_path, [dataset])

            document = ezdxf.readfile(output_path)
            modelspace = document.modelspace()
            polylines = list(modelspace.query("POLYLINE"))
            self.assertEqual(len(polylines), 1)
            self.assertEqual(
                [
                    (vertex.dxf.location.x, vertex.dxf.location.y, vertex.dxf.location.z)
                    for vertex in polylines[0].vertices
                ],
                [(100.0, 200.0, 1.0), (101.0, 202.0, 1.5), (103.0, 203.0, 0.0)],
            )
            self.assertEqual(
                polylines[0].dxf.layer,
                "N-OI-KL-WATER_DISTRIBUTIELEIDING_PVCBV_110-G",
            )
            self.assertEqual(
                [tag.value for tag in polylines[0].get_xdata(APP_ID)],
                [
                    "Job: PS63",
                    "JobId: 123",
                    "Object: Waterleiding",
                    "Coding: water",
                    "Layer: N-OI-KL-WATER_DISTRIBUTIELEIDING_PVCBV_110-G",
                ],
            )
            self.assertEqual([entity.dxf.text for entity in modelspace.query("TEXT")], ["PS63"])
            self.assertFalse(document.audit().has_errors)


if __name__ == "__main__":
    unittest.main()
