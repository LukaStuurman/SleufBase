# -*- mode: python ; coding: utf-8 -*-

import sys
from pathlib import Path
from PyInstaller.utils.hooks import collect_all, collect_submodules

ROOT = Path(SPECPATH).resolve()
PACKAGE_PARENT = ROOT.parent

# The source tree itself is the SleufBase package. Add its parent so hook
# helpers can import/discover every package module while building.
if str(PACKAGE_PARENT) not in sys.path:
    sys.path.insert(0, str(PACKAGE_PARENT))


def _collect(package: str):
    try:
        datas, binaries, hiddenimports = collect_all(package)
        return datas, binaries, hiddenimports
    except Exception:
        return [], [], []


datas = [
    # app.py executes version-specific bytecode at runtime, so this directory
    # must exist alongside the frozen SleufBase.app module.
    (str(ROOT / "_bytecode"), "SleufBase/_bytecode"),
    # app.py and the jobs window resolve branding resources from _MEIPASS/assets.
    (str(ROOT / "assets"), "assets"),
]
binaries = []

# app.py executes cached bytecode with exec(). Imports inside that bytecode are
# invisible to PyInstaller's static analysis, therefore include every module in
# the SleufBase package explicitly.
hiddenimports = collect_submodules("SleufBase")

# Packages with runtime backends/plugins that PyInstaller does not always
# discover completely from normal imports.
for package in ("PIL", "pyproj", "webview", "shapely", "ezdxf"):
    extra_datas, extra_binaries, extra_hidden = _collect(package)
    datas += extra_datas
    binaries += extra_binaries
    hiddenimports += extra_hidden

icon = ROOT / "assets" / "sleufbase_icon.ico"

a = Analysis(
    [str(ROOT / "sleufbase_launcher.py")],
    pathex=[str(PACKAGE_PARENT), str(ROOT)],
    binaries=binaries,
    datas=datas,
    hiddenimports=sorted(set(hiddenimports)),
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],
    noarchive=False,
    optimize=0,
)
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.datas,
    [],
    name="SleufBase",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon=str(icon) if icon.exists() else None,
)
