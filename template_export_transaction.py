from __future__ import annotations

from contextlib import contextmanager
import os
from pathlib import Path
import shutil
import tempfile


@contextmanager
def staged_template_output(output_path: str | Path):
    """Publish a complete pair without changing an earlier DXF or its images.

    Each attempt owns a fresh asset bundle on the destination filesystem. Image
    references already point to their permanent location, so publication needs
    neither raster copies nor a second DXF rewrite. Retain older bundles: users
    may still have drawings open or have saved copies that reference them.
    """
    output = Path(output_path)
    output.parent.mkdir(parents=True, exist_ok=True)
    asset_root = output.parent / f"{output.stem}_assets"
    asset_root.mkdir(parents=True, exist_ok=True)
    bundle = Path(tempfile.mkdtemp(prefix="export-", dir=asset_root)).resolve()
    # Recursive cleanup below is limited to the directory this attempt created.
    if bundle.parent != asset_root.resolve():
        raise RuntimeError("DXF-exportmap ligt buiten de gekozen assetmap.")
    staged = bundle / output.name
    published = False
    try:
        yield staged
        if not staged.is_file() or staged.stat().st_size == 0:
            raise RuntimeError("DXF-sjabloonexport heeft geen compleet bestand gemaakt.")
        os.replace(staged, output)
        published = True
    finally:
        if not published:
            # Cleanup must not hide the original export/publication error.
            shutil.rmtree(bundle, ignore_errors=True)
