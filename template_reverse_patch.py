from __future__ import annotations

import inspect
import re
import shutil
import threading
from pathlib import Path


NORMAL_MODE = "NORMAAL"
REVERSE_MODE = "REVERSE"
VARIANT_LAYER_PREFIX = "SLEUFBASE_"
VARIANT_BLOCK_SUFFIX = "_CONTENT"
_INVALID_DXF_NAME_CHARS = re.compile(r'[<>/\\\":;?*|=]')
_EXPORT_CONTEXT = threading.local()


def _sanitize_dxf_component(value: object) -> str:
    text = str(value or "").strip().upper()
    text = _INVALID_DXF_NAME_CHARS.sub("_", text)
    text = re.sub(r"\s+", "_", text)
    text = re.sub(r"_+", "_", text).strip("_.")
    return (text or "PS")[:120]


def variant_layer_name(label: object, slot_index: int, mode: str) -> str:
    normalized_mode = REVERSE_MODE if str(mode).upper() == REVERSE_MODE else NORMAL_MODE
    safe_label = _sanitize_dxf_component(label)
    return f"{VARIANT_LAYER_PREFIX}{safe_label}_VAK{max(1, int(slot_index)):02d}_{normalized_mode}"


def variant_block_name(label: object, slot_index: int, mode: str) -> str:
    return f"{variant_layer_name(label, slot_index, mode)}{VARIANT_BLOCK_SUFFIX}"


def _is_variant_layer(layer_name: object, mode: str) -> bool:
    name = str(layer_name or "").upper()
    suffix = f"_{REVERSE_MODE if str(mode).upper() == REVERSE_MODE else NORMAL_MODE}"
    return name.startswith(VARIANT_LAYER_PREFIX) and name.endswith(suffix)


def _ensure_layer(document, layer_name: str, *, visible: bool):
    try:
        layer = document.layers.get(layer_name)
    except Exception:
        layer = document.layers.add(layer_name)
    if visible:
        layer.on()
    else:
        layer.off()
    return layer


def _ensure_variant_container(document, modelspace, label: object, slot_index: int, mode: str):
    layer_name = variant_layer_name(label, slot_index, mode)
    block_name = variant_block_name(label, slot_index, mode)
    _ensure_layer(document, layer_name, visible=(str(mode).upper() != REVERSE_MODE))

    try:
        block = document.blocks.get(block_name)
    except Exception:
        block = document.blocks.new(name=block_name, base_point=(0.0, 0.0, 0.0))

    insert = None
    for entity in modelspace:
        if entity.dxftype() != "INSERT":
            continue
        if str(entity.dxf.name) == block_name and str(entity.dxf.layer) == layer_name:
            insert = entity
            break
    if insert is None:
        insert = modelspace.add_blockref(
            block_name,
            insert=(0.0, 0.0, 0.0),
            dxfattribs={"layer": layer_name},
        )
    return block, insert


def _move_entities_to_variant_container(
    document,
    modelspace,
    entities,
    *,
    label: object,
    slot_index: int,
    mode: str,
) -> None:
    movable = [entity for entity in entities if getattr(entity, "is_alive", True)]
    if not movable:
        return
    block, _insert = _ensure_variant_container(document, modelspace, label, slot_index, mode)
    for entity in movable:
        modelspace.move_to_layout(entity, block)


def _bounds_values(bounds) -> tuple[float, float, float, float] | None:
    try:
        return (
            float(bounds.min_x),
            float(bounds.min_y),
            float(bounds.max_x),
            float(bounds.max_y),
        )
    except (AttributeError, TypeError, ValueError):
        return None


def _bounds_equal(left, right, tolerance: float = 1e-6) -> bool:
    left_values = _bounds_values(left)
    right_values = _bounds_values(right)
    if left_values is None or right_values is None:
        return False
    return all(abs(a - b) <= tolerance for a, b in zip(left_values, right_values))


def _template_slots(exporter, document):
    cache = getattr(_EXPORT_CONTEXT, "slot_cache", None)
    if cache is None:
        cache = {}
        _EXPORT_CONTEXT.slot_cache = cache
    key = id(document)
    if key not in cache:
        cache[key] = list(exporter._detect_template_slots(document))
    return cache[key]


def _slot_index_for_box(exporter, document, box_bounds) -> int:
    for index, slot in enumerate(_template_slots(exporter, document), start=1):
        if _bounds_equal(box_bounds, getattr(slot, "tiff_box", None)):
            return index
        if _bounds_equal(box_bounds, getattr(slot, "map_box", None)):
            return index
    fallback = int(getattr(_EXPORT_CONTEXT, "fallback_slot_index", 0) or 0) + 1
    _EXPORT_CONTEXT.fallback_slot_index = fallback
    return fallback


def _label_from_image_name(image_name: object) -> str | None:
    text = str(image_name or "")
    lowered = text.casefold()
    for suffix in ("_tiff", "_kaart"):
        if lowered.endswith(suffix):
            return text[: -len(suffix)]
    return None


def _new_modelspace_entities(modelspace, before_ids: set[int]):
    return [entity for entity in modelspace if id(entity) not in before_ids]


def _resolved_export_call(original_export, instance, args, kwargs) -> dict[str, object]:
    signature = inspect.signature(original_export)
    bound = signature.bind(instance, *args, **kwargs)
    bound.apply_defaults()
    arguments = dict(bound.arguments)
    arguments.pop("self", None)
    return arguments


def _call_original_export(
    original_export,
    instance,
    call_arguments: dict[str, object],
    *,
    mode: str,
    output_path: Path,
    reverse_cross_sections: bool,
):
    previous_mode = getattr(_EXPORT_CONTEXT, "mode", None)
    previous_slot_cache = getattr(_EXPORT_CONTEXT, "slot_cache", None)
    previous_fallback = getattr(_EXPORT_CONTEXT, "fallback_slot_index", None)
    previous_current_slot = getattr(_EXPORT_CONTEXT, "current_slot_index", None)
    previous_current_label = getattr(_EXPORT_CONTEXT, "current_label", None)

    local_arguments = dict(call_arguments)
    local_arguments["output_path"] = output_path
    local_arguments["reverse_cross_sections"] = bool(reverse_cross_sections)
    if str(mode).upper() == REVERSE_MODE:
        callback = local_arguments.get("status_callback")
        if callable(callback):
            local_arguments["status_callback"] = lambda message: callback(f"Reverse: {message}")

    _EXPORT_CONTEXT.mode = mode
    _EXPORT_CONTEXT.slot_cache = {}
    _EXPORT_CONTEXT.fallback_slot_index = 0
    _EXPORT_CONTEXT.current_slot_index = None
    _EXPORT_CONTEXT.current_label = None
    try:
        return original_export(instance, **local_arguments)
    finally:
        _EXPORT_CONTEXT.mode = previous_mode
        if previous_slot_cache is None:
            _EXPORT_CONTEXT.__dict__.pop("slot_cache", None)
        else:
            _EXPORT_CONTEXT.slot_cache = previous_slot_cache
        if previous_fallback is None:
            _EXPORT_CONTEXT.__dict__.pop("fallback_slot_index", None)
        else:
            _EXPORT_CONTEXT.fallback_slot_index = previous_fallback
        if previous_current_slot is None:
            _EXPORT_CONTEXT.__dict__.pop("current_slot_index", None)
        else:
            _EXPORT_CONTEXT.current_slot_index = previous_current_slot
        if previous_current_label is None:
            _EXPORT_CONTEXT.__dict__.pop("current_label", None)
        else:
            _EXPORT_CONTEXT.current_label = previous_current_label


def _reverse_source_path(output_path: Path) -> Path:
    return output_path.with_name(
        f".{output_path.stem}.sleufbase-reverse-source{output_path.suffix or '.dxf'}"
    )


def _source_asset_dir(source_path: Path) -> Path:
    return source_path.parent / f"{source_path.stem}_assets"


def _copy_reverse_image_assets(reverse_document, reverse_source_path: Path, final_output_path: Path) -> Path:
    target_dir = final_output_path.parent / f"{final_output_path.stem}_reverse_assets"
    target_dir.mkdir(parents=True, exist_ok=True)
    copied_defs: set[str] = set()

    for block in reverse_document.blocks:
        block_name = str(getattr(block, "name", "") or "").upper()
        if not block_name.startswith(VARIANT_LAYER_PREFIX) or not block_name.endswith(
            f"_{REVERSE_MODE}{VARIANT_BLOCK_SUFFIX}"
        ):
            continue
        for image in block.query("IMAGE"):
            try:
                image_def = image.image_def
            except Exception:
                image_def = None
            if image_def is None:
                continue
            handle = str(getattr(image_def.dxf, "handle", "") or id(image_def))
            if handle in copied_defs:
                continue
            copied_defs.add(handle)
            filename = str(getattr(image_def.dxf, "filename", "") or "").strip()
            if not filename:
                continue
            source_file = Path(filename)
            if not source_file.is_absolute():
                source_file = reverse_source_path.parent / source_file
            if not source_file.exists():
                raise FileNotFoundError(f"Reverse rasterbestand ontbreekt: {source_file}")
            destination = target_dir / source_file.name
            shutil.copy2(source_file, destination)
            image_def.dxf.filename = str(destination.resolve())
    return target_dir


def _merge_reverse_variant_document(final_output_path: Path, reverse_source_path: Path) -> int:
    import ezdxf
    from ezdxf import xref

    target_document = ezdxf.readfile(final_output_path)
    reverse_document = ezdxf.readfile(reverse_source_path)
    _copy_reverse_image_assets(reverse_document, reverse_source_path, final_output_path)

    reverse_inserts = [
        entity
        for entity in reverse_document.modelspace()
        if entity.dxftype() == "INSERT" and _is_variant_layer(entity.dxf.layer, REVERSE_MODE)
    ]
    if not reverse_inserts:
        raise RuntimeError("De reverse export bevat geen proefsleuf-variantblokken.")

    reverse_handles = {str(entity.dxf.handle) for entity in reverse_inserts}

    def include_reverse_variant(entity) -> bool:
        return str(getattr(entity.dxf, "handle", "")) in reverse_handles

    xref.load_modelspace(
        reverse_document,
        target_document,
        filter_fn=include_reverse_variant,
        conflict_policy=xref.ConflictPolicy.KEEP,
    )

    for layer in target_document.layers:
        layer_name = str(layer.dxf.name)
        if _is_variant_layer(layer_name, NORMAL_MODE):
            layer.on()
        elif _is_variant_layer(layer_name, REVERSE_MODE):
            layer.off()

    target_document.saveas(final_output_path)
    return len(reverse_inserts)


def _cleanup_reverse_source(reverse_source_path: Path) -> None:
    try:
        reverse_source_path.unlink(missing_ok=True)
    except OSError:
        pass
    shutil.rmtree(_source_asset_dir(reverse_source_path), ignore_errors=True)


def install_template_reverse_export_patch() -> None:
    """Export normal + reverse content in the same template slot.

    Each slot gets two standard DXF layer-controlled container blocks. The normal
    layer starts enabled and the reverse layer starts disabled. This deliberately
    avoids undocumented AutoCAD dynamic-block internals while preserving all
    original child entity layers, colors, linetypes and image definitions.
    """

    from .cadastral_export import CadastralDxfExporter, CadastralExportError

    exporter_class = CadastralDxfExporter
    if getattr(exporter_class, "_sleufbase_reverse_variant_export_patch", False):
        return

    original_export = exporter_class.export_template_sheet
    original_add_box_image = exporter_class._add_box_image
    original_add_cross_section = exporter_class._add_template_cross_section

    def _add_box_image_with_variant_container(self, *args, **kwargs):
        mode = getattr(_EXPORT_CONTEXT, "mode", None)
        modelspace = kwargs.get("modelspace")
        if modelspace is None and len(args) >= 2:
            modelspace = args[1]
        document = kwargs.get("document")
        if document is None and args:
            document = args[0]
        box_bounds = kwargs.get("box_bounds")
        if box_bounds is None and len(args) >= 4:
            box_bounds = args[3]
        image_name = kwargs.get("image_name")
        if image_name is None and len(args) >= 5:
            image_name = args[4]
        label = _label_from_image_name(image_name)

        if mode not in {NORMAL_MODE, REVERSE_MODE} or modelspace is None or document is None or label is None:
            return original_add_box_image(self, *args, **kwargs)

        before_ids = {id(entity) for entity in modelspace}
        result = original_add_box_image(self, *args, **kwargs)
        created = _new_modelspace_entities(modelspace, before_ids)
        slot_index = _slot_index_for_box(self, document, box_bounds)
        _EXPORT_CONTEXT.current_slot_index = slot_index
        _EXPORT_CONTEXT.current_label = label
        _move_entities_to_variant_container(
            document,
            modelspace,
            created,
            label=label,
            slot_index=slot_index,
            mode=mode,
        )
        return result

    def _add_cross_section_with_variant_container(self, *args, **kwargs):
        mode = getattr(_EXPORT_CONTEXT, "mode", None)
        modelspace = kwargs.get("modelspace")
        if modelspace is None and len(args) >= 2:
            modelspace = args[1]
        document = kwargs.get("document")
        if document is None and args:
            document = args[0]
        label = kwargs.get("label")
        if label is None and len(args) >= 5:
            label = args[4]

        if mode not in {NORMAL_MODE, REVERSE_MODE} or modelspace is None or document is None:
            return original_add_cross_section(self, *args, **kwargs)

        before_ids = {id(entity) for entity in modelspace}
        result = original_add_cross_section(self, *args, **kwargs)
        created = _new_modelspace_entities(modelspace, before_ids)
        slot_index = int(getattr(_EXPORT_CONTEXT, "current_slot_index", 1) or 1)
        effective_label = label or getattr(_EXPORT_CONTEXT, "current_label", None) or f"PS{slot_index}"
        _move_entities_to_variant_container(
            document,
            modelspace,
            created,
            label=effective_label,
            slot_index=slot_index,
            mode=mode,
        )
        return result

    def _export_template_sheet_with_reverse_pair(self, *args, **kwargs):
        call_arguments = _resolved_export_call(original_export, self, args, kwargs)
        final_output_path = Path(call_arguments["output_path"])
        reverse_source_path = _reverse_source_path(final_output_path)
        callback = call_arguments.get("status_callback")

        # The old boolean selected one orientation. Reverse is now always
        # included, while the exported drawing starts in normal mode.
        normal_result = _call_original_export(
            original_export,
            self,
            call_arguments,
            mode=NORMAL_MODE,
            output_path=final_output_path,
            reverse_cross_sections=False,
        )
        try:
            _call_original_export(
                original_export,
                self,
                call_arguments,
                mode=REVERSE_MODE,
                output_path=reverse_source_path,
                reverse_cross_sections=True,
            )
            if callable(callback):
                callback("Voeg normale en reverse proefsleufversies samen...")
            merged_count = _merge_reverse_variant_document(final_output_path, reverse_source_path)
            if merged_count <= 0:
                raise CadastralExportError("Er zijn geen reverse proefsleufversies aan de DXF toegevoegd.")
        except CadastralExportError:
            raise
        except Exception as exc:
            raise CadastralExportError(
                f"Normaal/reverse DXF kon niet worden samengesteld: {exc}"
            ) from exc
        finally:
            _cleanup_reverse_source(reverse_source_path)
        return normal_result

    exporter_class._add_box_image = _add_box_image_with_variant_container
    exporter_class._add_template_cross_section = _add_cross_section_with_variant_container
    exporter_class.export_template_sheet = _export_template_sheet_with_reverse_pair
    exporter_class.SLEUFBASE_REVERSE_VARIANTS_DEFAULT = True
    exporter_class.SLEUFBASE_VARIANT_LAYER_PREFIX = VARIANT_LAYER_PREFIX
    exporter_class._sleufbase_reverse_variant_export_patch = True
