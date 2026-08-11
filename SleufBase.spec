# -*- mode: python ; coding: utf-8 -*-

from pathlib import Path
from PyInstaller.utils.hooks import collect_all

ROOT = Path(SPECPATH).resolve()
PACKAGE_PARENT = ROOT.parent


def _collect(package: str):
    try:
        datas, binaries, hiddenimports = collect_all(package)
        return datas, binaries, hiddenimports
    except Exception:
        return [], [], []


datas = [
    (str(ROOT / "_bytecode"), "SleufBase/_bytecode"),
    (str(ROOT / "assets"), "assets"),
]
binaries = []
hiddenimports = [
    "SleufBase",
    "SleufBase.app",
    "SleufBase.kickthemap_browser",
    "SleufBase.kickthemap_jobs_browser",
]

# Packages with runtime backends/plugins that PyInstaller does not always
# discover from normal imports.
for package in ("PIL", "pyproj", "webview", "shapely"):
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
