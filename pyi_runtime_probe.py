"""Minimal PyInstaller runtime probe for frozen CI diagnostics.

This hook intentionally imports only ``os`` so it can prove that the Python
runtime has started before SleufBase's launcher or application modules run.
Outside CI/smoke mode it is a no-op.
"""

import os


trace_path = os.environ.get("SLEUFBASE_SMOKE_TRACE")
if trace_path:
    try:
        parent = os.path.dirname(trace_path)
        if parent:
            os.makedirs(parent, exist_ok=True)
        with open(trace_path, "a", encoding="utf-8") as handle:
            handle.write("pyinstaller-runtime:ok\n")
            handle.flush()
            try:
                os.fsync(handle.fileno())
            except OSError:
                pass
    except OSError:
        # Diagnostics must never break a production startup.
        pass
