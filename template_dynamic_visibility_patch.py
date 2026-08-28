from __future__ import annotations

from pathlib import Path

from .autocad_dynamic_visibility import (
    NORMAL_STATE,
    PROPERTY_NAME,
    REVERSE_STATE,
    inspect_dynamic_visibility_block,
    promote_dynamic_visibility_blocks,
)
from . import template_reverse_patch as reverse_patch


DYNAMIC_BLOCK_SUFFIX = "_VERSIE"


def _variant_pair_key(layer_name: object) -> tuple[str, str] | None:
    name = str(layer_name or "").upper()
    normal_suffix = f"_{reverse_patch.NORMAL_MODE}"
    reverse_suffix = f"_{reverse_patch.REVERSE_MODE}"
    if name.startswith(reverse_patch.VARIANT_LAYER_PREFIX) and name.endswith(normal_suffix):
        return name[: -len(normal_suffix)], reverse_patch.NORMAL_MODE
    if name.startswith(reverse_patch.VARIANT_LAYER_PREFIX) and name.endswith(reverse_suffix):
        return name[: -len(reverse_suffix)], reverse_patch.REVERSE_MODE
    return None


def dynamic_block_name(label: object, slot_index: int) -> str:
    normal_layer = reverse_patch.variant_layer_name(
        label,
        slot_index,
        reverse_patch.NORMAL_MODE,
    )
    key, _mode = _variant_pair_key(normal_layer) or (normal_layer, reverse_patch.NORMAL_MODE)
    return f"{key}{DYNAMIC_BLOCK_SUFFIX}"


def _dynamic_name_from_pair_key(pair_key: str) -> str:
    return f"{pair_key}{DYNAMIC_BLOCK_SUFFIX}"


def _blockref_dxfattribs(entity) -> dict[str, object]:
    attributes: dict[str, object] = {"layer": "0"}
    for name, default in (
        ("xscale", 1.0),
        ("yscale", 1.0),
        ("zscale", 1.0),
        ("rotation", 0.0),
    ):
        try:
            attributes[name] = getattr(entity.dxf, name)
        except Exception:
            attributes[name] = default
    return attributes


def _make_reverse_image_definitions_unique(reverse_source_path: Path) -> dict[str, str]:
    """Give reverse IMAGEDEF dictionary entries names that cannot alias normal images.

    Normal and reverse exports deliberately use the same logical image names
    (for example ``PS1_tiff``).  During the later xref merge the KEEP conflict
    policy would otherwise reuse the normal IMAGEDEF and silently make the
    reverse block point at the normal raster.  Renaming only the reverse
    dictionary entries keeps the actual reverse TIFF/map resources distinct.
    """

    import ezdxf

    source_path = Path(reverse_source_path)
    document = ezdxf.readfile(source_path)
    try:
        image_dict = document.rootdict.get_required_dict("ACAD_IMAGE_DICT")
    except Exception:
        return {}

    renamed: dict[str, str] = {}
    processed_handles: set[str] = set()
    for block in document.blocks:
        block_name = str(getattr(block, "name", "") or "").upper()
        if not block_name.startswith(reverse_patch.VARIANT_LAYER_PREFIX) or not block_name.endswith(
            f"_{reverse_patch.REVERSE_MODE}{reverse_patch.VARIANT_BLOCK_SUFFIX}"
        ):
            continue

        for image in block.query("IMAGE"):
            try:
                image_def = image.image_def
            except Exception:
                image_def = None
            if image_def is None:
                continue

            handle = str(getattr(image_def.dxf, "handle", "") or id(image_def)).upper()
            if handle in processed_handles:
                continue
            processed_handles.add(handle)

            original_name = str(image_dict.find_key(image_def) or "").strip()
            if not original_name:
                continue

            base_name = f"{original_name}_{reverse_patch.REVERSE_MODE}"
            candidate = base_name
            sequence = 2
            while True:
                existing = image_dict.get(candidate)
                if existing is None or existing is image_def:
                    break
                candidate = f"{base_name}_{sequence}"
                sequence += 1

            if candidate == original_name:
                continue
            image_dict.add(candidate, image_def)
            image_dict.discard(original_name)
            renamed[original_name] = candidate

    if renamed:
        document.saveas(source_path)
    return renamed


def _wrap_variant_pairs_as_static_blocks(document) -> list[str]:
    """Replace each normal/reverse top-level pair with one two-child wrapper.

    The wrapper itself is still a plain DXF block at this stage. A separate raw
    DXF promotion step attaches a cloned, AutoCAD-generated visibility graph so
    ezdxf never has to construct undocumented dynamic-block object types.
    """

    modelspace = document.modelspace()
    groups: dict[str, dict[str, object]] = {}
    for entity in list(modelspace):
        if entity.dxftype() != "INSERT":
            continue
        pair = _variant_pair_key(entity.dxf.layer)
        if pair is None:
            continue
        key, mode = pair
        group = groups.setdefault(key, {})
        if mode in group:
            raise RuntimeError(f"Dubbele {mode}-variant gevonden voor {key}.")
        group[mode] = entity

    wrappers: list[str] = []
    for key in sorted(groups):
        group = groups[key]
        normal_insert = group.get(reverse_patch.NORMAL_MODE)
        reverse_insert = group.get(reverse_patch.REVERSE_MODE)
        if normal_insert is None or reverse_insert is None:
            missing = (
                reverse_patch.NORMAL_MODE
                if normal_insert is None
                else reverse_patch.REVERSE_MODE
            )
            raise RuntimeError(f"Proefsleuf {key} mist de {missing}-variant.")

        wrapper_name = _dynamic_name_from_pair_key(key)
        try:
            existing = document.blocks.get(wrapper_name)
        except Exception:
            existing = None
        if existing is not None:
            raise RuntimeError(f"Dynamic wrapperblock bestaat al: {wrapper_name}")

        wrapper = document.blocks.new(
            name=wrapper_name,
            base_point=(0.0, 0.0, 0.0),
        )
        wrapper.add_blockref(
            str(normal_insert.dxf.name),
            tuple(normal_insert.dxf.insert),
            dxfattribs=_blockref_dxfattribs(normal_insert),
        )
        wrapper.add_blockref(
            str(reverse_insert.dxf.name),
            tuple(reverse_insert.dxf.insert),
            dxfattribs=_blockref_dxfattribs(reverse_insert),
        )

        modelspace.delete_entity(normal_insert)
        modelspace.delete_entity(reverse_insert)
        modelspace.add_blockref(
            wrapper_name,
            insert=(0.0, 0.0, 0.0),
            dxfattribs={"layer": "0"},
        )
        wrappers.append(wrapper_name)

    if not wrappers:
        raise RuntimeError("Geen NORMAAL/REVERSE proefsleufparen gevonden voor Dynamic Blocks.")
    return wrappers


def _working_dynamic_path(output_path: Path) -> Path:
    return output_path.with_name(
        f".{output_path.stem}.sleufbase-dynamic-working{output_path.suffix or '.dxf'}"
    )


def _promote_exported_variants_to_dynamic_blocks(output_path: Path) -> list[str]:
    import ezdxf

    document = ezdxf.readfile(output_path)
    wrapper_names = _wrap_variant_pairs_as_static_blocks(document)

    # The old variant layers are now only legacy scaffolding; visibility is
    # controlled by the Dynamic Block parameter. Keep both on so layer state can
    # never accidentally hide one of the nested visibility states.
    for layer in document.layers:
        pair = _variant_pair_key(layer.dxf.name)
        if pair is not None:
            layer.on()

    working_path = _working_dynamic_path(output_path)
    try:
        document.saveas(working_path)
        promoted = promote_dynamic_visibility_blocks(working_path, wrapper_names)
        if promoted != len(wrapper_names):
            raise RuntimeError(
                f"Slechts {promoted} van {len(wrapper_names)} proefsleuven kregen Dynamic Visibility."
            )

        # Re-open through ezdxf to reject structurally damaged files, then check
        # the cloned AutoCAD metadata itself. AutoCAD remains the owner of the
        # dynamic-block semantics; ezdxf only has to preserve the objects.
        ezdxf.readfile(working_path)
        for wrapper_name in wrapper_names:
            details = inspect_dynamic_visibility_block(working_path, wrapper_name)
            if not details.get("is_dynamic"):
                raise RuntimeError(f"Dynamic Visibility ontbreekt voor {wrapper_name}.")
            if details.get("property_name") != PROPERTY_NAME:
                raise RuntimeError(
                    f"Dynamic property voor {wrapper_name} heet niet {PROPERTY_NAME!r}."
                )
            if tuple(details.get("states") or ()) != (NORMAL_STATE, REVERSE_STATE):
                raise RuntimeError(
                    f"Visibility states voor {wrapper_name} zijn ongeldig: {details.get('states')!r}."
                )
            if details.get("default_state") != NORMAL_STATE:
                raise RuntimeError(
                    f"{wrapper_name} start niet standaard in {NORMAL_STATE!r}."
                )

        working_path.replace(output_path)
    finally:
        try:
            working_path.unlink(missing_ok=True)
        except OSError:
            pass
    return wrapper_names


def install_template_dynamic_visibility_patch() -> None:
    """Add native AutoCAD `Versie: Normaal/Reverse` visibility per proefsleuf."""

    reverse_patch.install_template_reverse_export_patch()
    if getattr(reverse_patch, "_sleufbase_dynamic_visibility_patch", False):
        return

    original_merge = reverse_patch._merge_reverse_variant_document

    def _merge_with_native_dynamic_visibility(final_output_path, reverse_source_path):
        _make_reverse_image_definitions_unique(Path(reverse_source_path))
        merged_count = original_merge(final_output_path, reverse_source_path)
        _promote_exported_variants_to_dynamic_blocks(Path(final_output_path))
        return merged_count

    reverse_patch._merge_reverse_variant_document = _merge_with_native_dynamic_visibility
    reverse_patch._sleufbase_dynamic_visibility_patch = True

    from .cadastral_export import CadastralDxfExporter

    CadastralDxfExporter.SLEUFBASE_DYNAMIC_VISIBILITY_DEFAULT = True
    CadastralDxfExporter.SLEUFBASE_DYNAMIC_VISIBILITY_PROPERTY = PROPERTY_NAME
    CadastralDxfExporter.SLEUFBASE_DYNAMIC_VISIBILITY_STATES = (
        NORMAL_STATE,
        REVERSE_STATE,
    )
    CadastralDxfExporter._sleufbase_dynamic_visibility_patch = True
