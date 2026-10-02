import json
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from SleufBase.cadastral_export import CadastralDxfExporter


class ProfileLabelLayoutParityTests(unittest.TestCase):
    def test_layout_matches_pre_optimization_golden_results(self):
        # Captured from the original v0.3.61 label search using synthetic data.
        # Includes crowded hard/soft obstacles, the fallback search, collision
        # avoidance disabled, empty input and the >80 label safety threshold.
        cases = json.loads((Path(__file__).parent / "fixtures" / "profile_label_layout.json").read_text(encoding="utf-8"))
        for case in cases:
            with self.subTest(count=len(case["entries"]), options=case["options"]):
                exporter = CadastralDxfExporter(SimpleNamespace())
                leaders, lines = [], []
                exporter._ensure_template_profile_leader_block = lambda doc: None
                exporter._add_template_profile_multileader = lambda ms, **kw: leaders.append(kw)
                modelspace = SimpleNamespace(add_lwpolyline=lambda points, **kw: lines.append((points, kw)))
                exporter._distribute_template_leader_labels(
                    None, modelspace, case["entries"], 0.02, **case["options"]
                )
                actual = json.loads(json.dumps({"leaders": leaders, "lines": lines}))
                self.assertEqual(actual, case["expected"])
