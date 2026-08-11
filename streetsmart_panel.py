from __future__ import annotations

import marshal
import sys
from pathlib import Path


def _load_cached_module() -> None:
    cache_tag = sys.implementation.cache_tag
    if not cache_tag:
        raise ImportError("Python cache tag is niet beschikbaar.")
    pyc_path = Path(__file__).with_name("_bytecode") / f"streetsmart_panel.{cache_tag}.pyc"
    if not pyc_path.exists():
        raise ImportError(f"Bytecode voor app.streetsmart_panel niet gevonden: {pyc_path}")
    code = marshal.loads(pyc_path.read_bytes()[16:])
    exec(code, globals())


_load_cached_module()
