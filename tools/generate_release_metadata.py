from __future__ import annotations

import hashlib
import importlib.metadata
import json
import os
from datetime import datetime, timezone
from pathlib import Path
import sys


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main(argv: list[str]) -> int:
    if len(argv) < 3:
        raise SystemExit("Gebruik: generate_release_metadata.py OUTPUT_DIR ARTIFACT [ARTIFACT ...]")
    output_dir = Path(argv[1])
    output_dir.mkdir(parents=True, exist_ok=True)
    artifacts = [Path(item) for item in argv[2:]]
    missing = [str(path) for path in artifacts if not path.is_file()]
    if missing:
        raise SystemExit("Release-artifacts ontbreken: " + ", ".join(missing))

    checksum_lines = []
    artifact_records = []
    for path in artifacts:
        digest = sha256_file(path)
        checksum_lines.append(f"{digest}  {path.name}")
        artifact_records.append({"name": path.name, "size": path.stat().st_size, "sha256": digest})
    (output_dir / "SHA256SUMS.txt").write_text("\n".join(checksum_lines) + "\n", encoding="utf-8")

    components = []
    for distribution in sorted(importlib.metadata.distributions(), key=lambda dist: (dist.metadata.get("Name") or "").casefold()):
        name = distribution.metadata.get("Name")
        if not name:
            continue
        components.append({"type": "library", "name": name, "version": distribution.version})

    sbom = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "component": {"type": "application", "name": "SleufBase", "version": os.environ.get("SLEUFBASE_RELEASE_VERSION", "unknown")},
            "properties": [
                {"name": "git.commit", "value": os.environ.get("GITHUB_SHA", "unknown")},
                {"name": "python.version", "value": sys.version.split()[0]},
            ],
        },
        "components": components,
        "properties": [{"name": "release.artifacts", "value": json.dumps(artifact_records, separators=(",", ":"))}],
    }
    (output_dir / "sbom.cdx.json").write_text(json.dumps(sbom, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"Release metadata gemaakt voor {len(artifacts)} artifacts en {len(components)} packages")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
