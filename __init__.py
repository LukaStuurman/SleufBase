"""Desktop viewer for GeoTIFF proefsleuven with KLIC overlays."""

from .settings_ui import install_settings_ui_patch
from .settings_general_layout_patch import install_settings_general_layout_patch


install_settings_ui_patch()
install_settings_general_layout_patch()
