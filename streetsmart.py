from __future__ import annotations

from .source_migration import load_migrating_module


load_migrating_module("streetsmart", globals(), __file__)
