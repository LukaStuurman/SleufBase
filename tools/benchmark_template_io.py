from __future__ import annotations

import argparse
import gc
import json
from pathlib import Path
import tempfile
import time

import ezdxf


DEFAULT_TEMPLATE = Path(__file__).resolve().parents[1] / "assets" / "cadastral_template.dxf"


def _seconds(start: float) -> float:
    return round(max(0.0, time.perf_counter() - start), 6)


def benchmark_template_io(template_path: Path) -> dict[str, object]:
    path = template_path.expanduser().resolve()
    if not path.is_file():
        raise FileNotFoundError(path)

    gc.collect()
    started = time.perf_counter()
    document = ezdxf.readfile(path)
    read_seconds = _seconds(started)

    with tempfile.TemporaryDirectory(prefix="sleufbase-template-benchmark-") as temporary_directory:
        output = Path(temporary_directory) / "template-roundtrip.dxf"
        started = time.perf_counter()
        document.saveas(output)
        save_seconds = _seconds(started)
        output_bytes = output.stat().st_size
        del document
        gc.collect()

        started = time.perf_counter()
        validation = ezdxf.readfile(output)
        reread_seconds = _seconds(started)
        modelspace_entities = len(validation.modelspace())
        del validation

    return {
        "template": str(path),
        "source_bytes": path.stat().st_size,
        "roundtrip_bytes": output_bytes,
        "read_seconds": read_seconds,
        "save_seconds": save_seconds,
        "reread_seconds": reread_seconds,
        "save_plus_reread_seconds": round(save_seconds + reread_seconds, 6),
        "modelspace_entities": modelspace_entities,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Meet de ezdxf parse/save-kosten van het echte SleufBase productiesjabloon."
    )
    parser.add_argument("--template", type=Path, default=DEFAULT_TEMPLATE)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    result = benchmark_template_io(args.template)
    if args.json:
        print(json.dumps(result, ensure_ascii=False, sort_keys=True))
    else:
        print(f"Template: {result['template']}")
        print(f"Bron: {result['source_bytes']} bytes")
        print(f"readfile: {result['read_seconds']:.3f}s")
        print(f"saveas: {result['save_seconds']:.3f}s")
        print(f"re-read: {result['reread_seconds']:.3f}s")
        print(f"save+re-read: {result['save_plus_reread_seconds']:.3f}s")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
