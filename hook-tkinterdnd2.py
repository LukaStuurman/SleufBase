"""PyInstaller hook for tkinterdnd2/TkDnD runtime assets."""

from PyInstaller.utils.hooks import collect_data_files

# tkinterdnd2 imports Python code normally, but its native Tcl/Tk TkDnD payload
# lives in package data. Without these files the frozen app can import the
# module yet still fail when drag-and-drop is initialized.
datas = collect_data_files("tkinterdnd2")
