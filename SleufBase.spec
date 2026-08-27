# -*- mode: python ; coding: utf-8 -*-

import sys
from pathlib import Path
from PyInstaller.utils.hooks import collect_all, collect_submodules

ROOT = Path(SPECPATH).resolve()
PACKAGE_PARENT = ROOT.parent

if str(PACKAGE_PARENT) not in sys.path:
    sys.path.insert(0, str(PACKAGE_PARENT))


_EXCLUDED_HIDDEN_PREFIXES = (
    "Crypto.SelfTest",
    "shapely.conftest",
    "shapely.tests",
    "tkinter.test",
    "ezdxf.addons.browser",
    "ezdxf.tools.test",
)


def _collect(package: str, *, required: bool = False):
    try:
        datas, binaries, hiddenimports = collect_all(package)
        return datas, binaries, hiddenimports
    except Exception:
        if required:
            raise
        return [], [], []


def _production_hiddenimports(imports):
    return [
        name
        for name in imports
        if not any(
            name == prefix or name.startswith(prefix + ".")
            for prefix in _EXCLUDED_HIDDEN_PREFIXES
        )
    ]


datas = []
bytecode_dir = ROOT / "_bytecode"
if not bytecode_dir.exists():
    raise FileNotFoundError(f"Verplichte bytecode-map ontbreekt: {bytecode_dir}")
datas.append((str(bytecode_dir), "SleufBase/_bytecode"))

assets_dir = ROOT / "assets"
template_path = assets_dir / "cadastral_template.dxf"
if not template_path.exists():
    raise FileNotFoundError(f"Verplicht ingebouwd DXF-sjabloon ontbreekt: {template_path}")
datas.append((str(assets_dir), "assets"))

native_dir = ROOT / "native"
if native_dir.exists():
    datas.append((str(native_dir), "SleufBase/native"))

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
    extra_datas, extra_binaries, extra_hidden = _collect(
        package,
        required=(package == "tkinterdnd2"),
    )
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
    "tkinterdnd2",
    "tkinterdnd2.TkinterDnD",
    "mapbox_vector_tile",
    "boto3",
    "botocore",
    "Crypto",
    "Crypto.Cipher",
    "Crypto.Cipher.AES",
]
hiddenimports = _production_hiddenimports(hiddenimports)

icon = assets_dir / "sleufbase_icon.ico"
version_info = ROOT / "windows_version_info.txt"
runtime_probe = ROOT / "pyi_runtime_probe.py"
if not runtime_probe.exists():
    raise FileNotFoundError(f"PyInstaller runtime-probe ontbreekt: {runtime_probe}")

a = Analysis(
    [str(ROOT / "sleufbase_launcher.py")],
    pathex=[str(PACKAGE_PARENT), str(ROOT)],
    binaries=binaries,
    datas=datas,
    hiddenimports=sorted(set(hiddenimports)),
    hookspath=[str(ROOT)],
    hooksconfig={},
    runtime_hooks=[str(runtime_probe)],
    excludes=list(_EXCLUDED_HIDDEN_PREFIXES),
    noarchive=False,
    optimize=1,
)
pyz = PYZ(a.pure)

# Professional/default build: onedir deliberately avoids the one-file bootloader
# extraction penalty on every launch. The installer packages this folder.
exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name="SleufBase",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon=str(icon) if icon.exists() else None,
    version=str(version_info) if version_info.exists() else None,
)

coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=True,
    upx_exclude=[],
    name="SleufBase",
)
