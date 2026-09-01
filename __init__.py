"""Desktop viewer for GeoTIFF proefsleuven with KLIC overlays."""

from .settings_ui import install_settings_ui_patch
from .settings_general_layout_patch import install_settings_general_layout_patch
from .settings_kickthemap_autosave_patch import (
    install_settings_kickthemap_autosave_patch,
)
from .dynamic_visibility_finalize_patch import install_dynamic_visibility_finalize_patch
from .template_bgt_fetch_patch import install_template_bgt_fetch_patch


install_settings_ui_patch()
install_settings_general_layout_patch()
install_settings_kickthemap_autosave_patch()
install_dynamic_visibility_finalize_patch()
install_template_bgt_fetch_patch()
