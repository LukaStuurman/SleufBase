from __future__ import annotations

from .source_migration import load_source_or_legacy


load_source_or_legacy("streetsmart", globals(), __file__)
