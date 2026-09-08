from __future__ import annotations

import argparse
import json
from pathlib import Path

from SleufBase.template_footprint import analyze_template_footprint


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Rapporteer DXF-sjabloon footprint zonder AutoCAD metadata te muteren."
    )
    parser.add_argument(
        "template",
        nargs="?",
        default=str(Path(__file__).resolve().parents[1] / "assets" / "cadastral_template.dxf"),
    )
    parser.add_argument("--top", type=int, default=20, help="Aantal grootste blocks in het rapport.")
    args = parser.parse_args()
    report = analyze_template_footprint(args.template, largest_block_limit=args.top)
    print(json.dumps(report.as_dict(), indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
