from __future__ import annotations

from contextlib import contextmanager
import os
from pathlib import Path
import shutil
import threading
import time
from typing import Any, Iterable

from . import autocad_dynamic_visibility as dv
from . import dxf_template_pipeline_patch as pipeline
from . import template_dynamic_visibility_patch as dynamic_patch
from . import template_reverse_patch as reverse_patch


PATCH_VERSION = 1
_INDEX_REUSE_CONTEXT = threading.local()
_ORIGINAL_RECORD_INDEXES = pipeline._record_indexes
_ORIGINAL_NEW_MODELSPACE_ENTITIES = reverse_patch._new_modelspace_entities


def _increment(name: str) -> None:
    try:
        pipeline._increment_pair_counter(name)
    except Exception:
        pass


def _normalized_path(value: object) -> Path:
    return Path(value).expanduser().resolve(strict=False)


def _same_path(left: object, right: object) -> bool:
    try:
        return os.path.normcase(str(_normalized_path(left))) == os.path.normcase(str(_normalized_path(right)))
    except Exception:
        return False


def _record_indexes_v3(records, sections):
    """Reuse an unchanged raw-DXF index graph inside a tightly scoped phase."""

    if not bool(getattr(_INDEX_REUSE_CONTEXT, "enabled", False)):
        return _ORIGINAL_RECORD_INDEXES(records, sections)
    key = (id(records), id(sections), len(records))
    cached_key = getattr(_INDEX_REUSE_CONTEXT, "key", None)
    cached = getattr(_INDEX_REUSE_CONTEXT, "value", None)
    if cached_key == key and cached is not None:
        _increment("dynamic_record_index_reuse_hits")
        return cached
    value = _ORIGINAL_RECORD_INDEXES(records, sections)
    _INDEX_REUSE_CONTEXT.key = key
    _INDEX_REUSE_CONTEXT.value = value
    return value


@contextmanager
def _reuse_record_indexes():
    previous_enabled = getattr(_INDEX_REUSE_CONTEXT, "enabled", False)
    previous_key = getattr(_INDEX_REUSE_CONTEXT, "key", None)
    previous_value = getattr(_INDEX_REUSE_CONTEXT, "value", None)
    _INDEX_REUSE_CONTEXT.enabled = True
    _INDEX_REUSE_CONTEXT.key = None
    _INDEX_REUSE_CONTEXT.value = None
    try:
        yield
    finally:
        _INDEX_REUSE_CONTEXT.enabled = previous_enabled
        _INDEX_REUSE_CONTEXT.key = previous_key
        _INDEX_REUSE_CONTEXT.value = previous_value


def _promote_records_v3(
    records: list[list[tuple[int, str]]],
    sections: list[str | None],
    block_names: Iterable[str],
    *,
    next_handle: int,
) -> tuple[int, int]:
    """Promote wrappers with one record index and one OBJECTS insertion."""

    donor = dv._discover_donor(records, sections)
    block_records, nested_inserts, object_by_handle, _object_children = pipeline._record_indexes(
        records, sections
    )

    metadata_by_handle: dict[str, list[tuple[int, str]]] = {}
    missing: list[str] = []
    for old_handle in donor.metadata_handles:
        item = object_by_handle.get(str(old_handle).upper())
        if item is None:
            missing.append(str(old_handle))
            continue
        metadata_by_handle[str(old_handle)] = item[1]
    if missing:
        raise dv.DynamicVisibilityError(
            f"Visibility-donor metadata is onvolledig; ontbrekende handles: {sorted(missing)}"
        )
    _increment("dynamic_metadata_full_scans_saved")

    insert_at = dv._objects_end_index(records, sections)
    promoted = 0
    all_cloned_records: list[list[tuple[int, str]]] = []

    for block_name in block_names:
        target = block_records.get(str(block_name).upper())
        if target is None:
            raise dv.DynamicVisibilityError(
                f"BLOCK_RECORD ontbreekt voor dynamic block {block_name!r}."
            )
        block_index, block_record = target
        target_block_record_handle = (dv._record_handle(block_record) or "").upper()
        if not target_block_record_handle:
            raise dv.DynamicVisibilityError(f"Dynamic block {block_name!r} heeft geen handle.")

        states = nested_inserts.get(target_block_record_handle, {})
        normal = states.get(reverse_patch.NORMAL_MODE)
        reverse = states.get(reverse_patch.REVERSE_MODE)
        if normal is None or reverse is None:
            raise dv.DynamicVisibilityError(
                "Dynamic wrapper bevat niet exact de verwachte NORMAAL/REVERSE blockrefs."
            )

        handle_map: dict[str, str] = {}
        for old_handle in donor.metadata_handles:
            handle_map[old_handle] = f"{next_handle:X}"
            next_handle += 1

        records[block_index] = dv._inject_dynamic_block_record_data(
            block_record,
            xdictionary_handle=handle_map[donor.xdictionary_handle],
            block_name=str(block_name),
            block_rep_e_tag=donor.block_rep_e_tag,
        )
        all_cloned_records.extend(
            dv._remap_metadata_record(
                metadata_by_handle[str(old_handle)],
                handle_map=handle_map,
                donor=donor,
                target_block_record_handle=target_block_record_handle,
                normal_insert_handle=normal[1],
                reverse_insert_handle=reverse[1],
            )
            for old_handle in donor.metadata_handles
        )
        promoted += 1

    if all_cloned_records:
        records[insert_at:insert_at] = all_cloned_records
        sections[insert_at:insert_at] = ["OBJECTS"] * len(all_cloned_records)
        _increment("dynamic_batched_object_insertions")
    return promoted, next_handle


def _promote_exported_variants_one_pass_v3(output_path: Path) -> list[str]:
    """Keep v2 semantics while reusing the post-promotion raw index graph."""

    import ezdxf

    started = time.perf_counter()
    document = ezdxf.readfile(output_path)
    wrapper_names = dynamic_patch._wrap_variant_pairs_as_static_blocks(document)
    for layer in document.layers:
        if dynamic_patch._variant_pair_key(layer.dxf.name) is not None:
            layer.on()

    selector_locations = pipeline._selector_locations_from_document(document, wrapper_names)
    working_path = dynamic_patch._working_dynamic_path(output_path)
    try:
        document.saveas(working_path)
        del document

        pairs, newline, had_bom = dv._read_pairs(working_path)
        next_handle = max(dv._max_handle_value(pairs) + 1, dv._header_handseed(pairs))
        records = dv._split_records(pairs)
        del pairs
        sections = dv._record_sections(records)

        promoted, next_handle = pipeline._promote_records(
            records,
            sections,
            wrapper_names,
            next_handle=next_handle,
        )
        if promoted != len(wrapper_names):
            raise RuntimeError(
                f"Slechts {promoted} van {len(wrapper_names)} proefsleuven kregen Dynamic Visibility."
            )
        pipeline._set_handseed_in_records(records, next_handle)

        # Finalization changes values but does not add/remove records. Reuse the
        # same index graph for validation instead of scanning the full DXF again.
        with _reuse_record_indexes():
            finalized = pipeline._finalize_records(
                records,
                sections,
                wrapper_names,
                selector_locations,
            )
            if finalized != len(wrapper_names):
                raise RuntimeError(
                    f"Slechts {finalized} van {len(wrapper_names)} Dynamic Blocks zijn geïnitialiseerd."
                )
            details = pipeline._inspect_records(records, sections, wrapper_names)
        pipeline._validate_dynamic_details(details, wrapper_names)

        dv._write_pairs(
            working_path,
            pipeline._iter_record_pairs(records),
            newline=newline,
            had_bom=had_bom,
        )
        del records
        del sections

        # Retain the structural ezdxf safety check from pipeline v2.
        validation_document = ezdxf.readfile(working_path)
        del validation_document
        working_path.replace(output_path)
    finally:
        try:
            working_path.unlink(missing_ok=True)
        except OSError:
            pass
    pipeline._add_pair_phase("dynamic_visibility", time.perf_counter() - started)
    return wrapper_names


def _path_is_within(path: Path, root: Path) -> bool:
    try:
        path.resolve(strict=False).relative_to(root.resolve(strict=False))
        return True
    except (OSError, ValueError):
        return False


def _relocate_reverse_asset(source: Path, destination: Path, temporary_root: Path) -> str:
    """Move disposable assets, hardlink shared assets, and copy only as fallback."""

    source = source.resolve(strict=False)
    destination.parent.mkdir(parents=True, exist_ok=True)
    if _same_path(source, destination):
        return "existing"
    try:
        destination.unlink(missing_ok=True)
    except OSError:
        pass

    if _path_is_within(source, temporary_root):
        try:
            os.replace(source, destination)
            return "moved"
        except OSError:
            shutil.copy2(source, destination)
            return "copied"

    try:
        os.link(source, destination)
        return "linked"
    except OSError:
        shutil.copy2(source, destination)
        return "copied"


def _transfer_reverse_image_assets(
    reverse_document,
    reverse_source_path: Path,
    final_output_path: Path,
) -> Path:
    target_dir = final_output_path.parent / f"{final_output_path.stem}_reverse_assets"
    target_dir.mkdir(parents=True, exist_ok=True)
    temporary_root = reverse_patch._source_asset_dir(reverse_source_path).resolve(strict=False)
    processed_defs: set[str] = set()
    transferred_by_source: dict[str, Path] = {}

    for block in reverse_document.blocks:
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
            handle = str(getattr(image_def.dxf, "handle", "") or id(image_def))
            if handle in processed_defs:
                continue
            processed_defs.add(handle)
            filename = str(getattr(image_def.dxf, "filename", "") or "").strip()
            if not filename:
                continue
            source = Path(filename)
            if not source.is_absolute():
                source = reverse_source_path.parent / source
            source = source.resolve(strict=False)
            if not source.exists():
                raise FileNotFoundError(f"Reverse rasterbestand ontbreekt: {source}")

            source_key = os.path.normcase(str(source))
            destination = transferred_by_source.get(source_key)
            if destination is None:
                destination = target_dir / source.name
                mode = _relocate_reverse_asset(source, destination, temporary_root)
                transferred_by_source[source_key] = destination
                _increment(f"reverse_assets_{mode}")
            image_def.dxf.filename = str(destination.resolve(strict=False))
    return target_dir


def _captured_reverse_document(reverse_source_path: Path):
    cache = pipeline._pair_cache()
    if cache is None:
        return None
    captured_path = cache.get("reverse_document_path")
    document = cache.get("reverse_document")
    if document is None or not _same_path(captured_path, reverse_source_path):
        return None
    cache.pop("reverse_document", None)
    cache.pop("reverse_document_path", None)
    _increment("reverse_dxf_reads_skipped")
    return document


def _merge_reverse_variant_document_v3(
    final_output_path: Path,
    reverse_source_path: Path,
) -> int:
    """Merge the captured reverse document and avoid its temporary DXF write/read."""

    import ezdxf
    from ezdxf import xref

    started = time.perf_counter()
    target_document = ezdxf.readfile(final_output_path)
    reverse_document = _captured_reverse_document(reverse_source_path)
    if reverse_document is None:
        reverse_document = ezdxf.readfile(reverse_source_path)
    pipeline._make_reverse_image_definitions_unique_document(reverse_document)
    _transfer_reverse_image_assets(reverse_document, reverse_source_path, final_output_path)

    reverse_inserts = [
        entity
        for entity in reverse_document.modelspace()
        if entity.dxftype() == "INSERT"
        and reverse_patch._is_variant_layer(entity.dxf.layer, reverse_patch.REVERSE_MODE)
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
    del reverse_document

    for layer in target_document.layers:
        layer_name = str(layer.dxf.name)
        if reverse_patch._is_variant_layer(layer_name, reverse_patch.NORMAL_MODE):
            layer.on()
        elif reverse_patch._is_variant_layer(layer_name, reverse_patch.REVERSE_MODE):
            layer.off()
    target_document.saveas(final_output_path)
    del target_document
    pipeline._add_pair_phase("reverse_merge", time.perf_counter() - started)
    return len(reverse_inserts)


def _new_modelspace_entities_tail(modelspace, before_ids: set[int]):
    """Use ezdxf layout slicing to avoid the second complete modelspace scan."""

    start = len(before_ids)
    try:
        if len(modelspace) >= start:
            tail = list(modelspace[start:])
            if all(id(entity) not in before_ids for entity in tail):
                _increment("variant_tail_slices")
                return tail
    except Exception:
        pass
    return _ORIGINAL_NEW_MODELSPACE_ENTITIES(modelspace, before_ids)


def _install_reverse_save_capture() -> None:
    from ezdxf.document import Drawing

    if int(getattr(Drawing, "_sleufbase_reverse_save_capture_version", 0) or 0) >= PATCH_VERSION:
        return
    original_saveas = Drawing.saveas

    def saveas_with_reverse_capture(self, filename, *args, **kwargs):
        capture = bool(getattr(reverse_patch._EXPORT_CONTEXT, "capture_reverse_save", False))
        expected_path = getattr(reverse_patch._EXPORT_CONTEXT, "capture_reverse_path", None)
        if capture and expected_path is not None and _same_path(filename, expected_path):
            cache = pipeline._pair_cache()
            if cache is not None:
                try:
                    self.filename = str(filename)
                except Exception:
                    pass
                cache["reverse_document"] = self
                cache["reverse_document_path"] = Path(filename)
                _increment("reverse_dxf_writes_skipped")
                return None
        return original_saveas(self, filename, *args, **kwargs)

    Drawing.saveas = saveas_with_reverse_capture
    Drawing._sleufbase_reverse_save_capture_version = PATCH_VERSION


def _install_reverse_call_capture() -> None:
    if int(getattr(reverse_patch, "_sleufbase_reverse_call_capture_version", 0) or 0) >= PATCH_VERSION:
        return
    previous_call = reverse_patch._call_original_export

    def _call_original_export_v3(
        original_export,
        instance,
        call_arguments,
        *,
        mode,
        output_path,
        reverse_cross_sections,
    ):
        if str(mode).upper() != reverse_patch.REVERSE_MODE:
            return previous_call(
                original_export,
                instance,
                call_arguments,
                mode=mode,
                output_path=output_path,
                reverse_cross_sections=reverse_cross_sections,
            )

        previous_enabled = getattr(reverse_patch._EXPORT_CONTEXT, "capture_reverse_save", None)
        previous_path = getattr(reverse_patch._EXPORT_CONTEXT, "capture_reverse_path", None)
        reverse_patch._EXPORT_CONTEXT.capture_reverse_save = True
        reverse_patch._EXPORT_CONTEXT.capture_reverse_path = Path(output_path)
        try:
            return previous_call(
                original_export,
                instance,
                call_arguments,
                mode=mode,
                output_path=output_path,
                reverse_cross_sections=reverse_cross_sections,
            )
        finally:
            if previous_enabled is None:
                reverse_patch._EXPORT_CONTEXT.__dict__.pop("capture_reverse_save", None)
            else:
                reverse_patch._EXPORT_CONTEXT.capture_reverse_save = previous_enabled
            if previous_path is None:
                reverse_patch._EXPORT_CONTEXT.__dict__.pop("capture_reverse_path", None)
            else:
                reverse_patch._EXPORT_CONTEXT.capture_reverse_path = previous_path

    reverse_patch._call_original_export = _call_original_export_v3
    reverse_patch._sleufbase_reverse_call_capture_version = PATCH_VERSION


def install_dxf_template_pipeline_v3_patch() -> None:
    """Install the next safe DXF-template hot-path optimizations."""

    from .cadastral_export import CadastralDxfExporter

    pipeline.install_dxf_template_pipeline_patch()
    if int(
        getattr(CadastralDxfExporter, "_sleufbase_dxf_template_pipeline_v3_version", 0) or 0
    ) >= PATCH_VERSION:
        return

    _install_reverse_save_capture()
    _install_reverse_call_capture()

    pipeline._record_indexes = _record_indexes_v3
    pipeline._promote_records = _promote_records_v3
    pipeline._promote_exported_variants_one_pass = _promote_exported_variants_one_pass_v3
    pipeline._merge_reverse_variant_document_one_read = _merge_reverse_variant_document_v3
    dynamic_patch._promote_exported_variants_to_dynamic_blocks = _promote_exported_variants_one_pass_v3
    reverse_patch._new_modelspace_entities = _new_modelspace_entities_tail

    CadastralDxfExporter._sleufbase_dxf_template_pipeline_v3_version = PATCH_VERSION
    CadastralDxfExporter.SLEUFBASE_REVERSE_DXF_IN_MEMORY = True
