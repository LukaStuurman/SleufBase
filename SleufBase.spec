# -*- mode: python ; coding: utf-8 -*-

import sys
from pathlib import Path
from PyInstaller.utils.hooks import collect_all, collect_submodules

ROOT = Path(SPECPATH).resolve()
PACKAGE_PARENT = ROOT.parent

if str(PACKAGE_PARENT) not in sys.path:
    sys.path.insert(0, str(PACKAGE_PARENT))


def _collect(package: str):
    try:
        datas, binaries, hiddenimports = collect_all(package)
        return datas, binaries, hiddenimports
    except Exception:
        return [], [], []


datas = []
bytecode_dir = ROOT / "_bytecode"
if not bytecode_dir.exists():
    raise FileNotFoundError(f"Verplichte bytecode-map ontbreekt: {bytecode_dir}")
datas.append((str(bytecode_dir), "SleufBase/_bytecode"))

assets_dir = ROOT / "assets"
if assets_dir.exists():
    datas.append((str(assets_dir), "assets"))

binaries = []
hiddenimports = collect_submodules("SleufBase")
hiddenimports += collect_submodules("tkinter")

for package in (
    "PIL",
    "pyproj",
    "webview",
    "shapely",
    "ezdxf",
    "tkinterdnd2",
    "mapbox_vector_tile",
    "boto3",
    "botocore",
    "Crypto",
):
    extra_datas, extra_binaries, extra_hidden = _collect(package)
    datas += extra_datas
    binaries += extra_binaries
    hiddenimports += extra_hidden

hiddenimports += [
    "tkinter.colorchooser",
    "tkinter.commondialog",
    "tkinter.constants",
    "tkinter.dialog",
    "tkinter.dnd",
    "tkinter.filedialog",
    "tkinter.font",
    "tkinter.messagebox",
    "tkinter.scrolledtext",
    "tkinter.simpledialog",
    "tkinter.ttk",
    "mapbox_vector_tile",
    "boto3",
    "botocore",
    "Crypto",
    "Crypto.Cipher",
    "Crypto.Cipher.AES",
]

icon = assets_dir / "sleufbase_icon.ico"

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
