from __future__ import annotations

from concurrent.futures import FIRST_COMPLETED, ThreadPoolExecutor, wait
from pathlib import Path
import logging
import os
import shutil
import threading
import time
from typing import Any, Iterable

from . import autocad_dynamic_visibility as dv
from . import dynamic_visibility_finalize_patch as finalize_patch
from . import template_dynamic_visibility_patch as dynamic_patch
from . import template_export_performance_patch as perf_patch
from . import template_reverse_patch as reverse_patch
from .template_asset_memory_patch import (
    TEMPLATE_UI_PUMP_INTERVAL_SECONDS,
    _contains_virtual_template_task,
    _pump_template_ui,
)


PATCH_VERSION = 1
MAX_ADAPTIVE_TIFF_WORKERS = 2
# Estimated source pixels allowed across concurrent high-resolution virtual-trench
# renders. A 4000x4000 render consumes the whole budget and therefore remains
# single-worker; two smaller renders may overlap.
VIRTUAL_TIFF_PIXEL_BUDGET = 16_000_000

_LOGGER = logging.getLogger("SleufBase")
_INSTALL_LOCK = threading.RLock()
_TEMPLATE_SLOT_CACHE_LOCK = threading.RLock()
_TEMPLATE_SLOT_CACHE: dict[tuple[str, int, int], tuple[Any, ...]] = {}


def _current_rss_bytes() -> int:
    """Best-effort current resident set size without an extra dependency."""

    if os.name == "nt":
        try:
            import ctypes
            from ctypes import wintypes

            class PROCESS_MEMORY_COUNTERS(ctypes.Structure):
                _fields_ = [
                    ("cb", wintypes.DWORD),
                    ("PageFaultCount", wintypes.DWORD),
                    ("PeakWorkingSetSize", ctypes.c_size_t),
                    ("WorkingSetSize", ctypes.c_size_t),
                    ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
                    ("QuotaPagedPoolUsage", ctypes.c_size_t),
                    ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                    ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
                    ("PagefileUsage", ctypes.c_size_t),
                    ("PeakPagefileUsage", ctypes.c_size_t),
                ]

            counters = PROCESS_MEMORY_COUNTERS()
            counters.cb = ctypes.sizeof(counters)
            process = ctypes.windll.kernel32.GetCurrentProcess()
            if ctypes.windll.psapi.GetProcessMemoryInfo(
                process,
                ctypes.byref(counters),
                counters.cb,
            ):
                return int(counters.WorkingSetSize)
        except Exception:
            return 0
        return 0

    try:
        statm = Path("/proc/self/statm")
        if statm.exists():
            pages = int(statm.read_text(encoding="ascii").split()[1])
            return pages * int(os.sysconf("SC_PAGE_SIZE"))
    except Exception:
        pass
    return 0


def _pair_cache() -> dict[str, Any] | None:
    cache = getattr(reverse_patch._EXPORT_CONTEXT, "pair_cache", None)
    return cache if isinstance(cache, dict) else None


def _sample_pair_rss() -> None:
    cache = _pair_cache()
    if cache is None:
        return
    current = _current_rss_bytes()
    if current <= 0:
        return
    cache["peak_rss_bytes"] = max(int(cache.get("peak_rss_bytes", 0) or 0), current)


def _add_pair_phase(name: str, elapsed: float) -> None:
    cache = _pair_cache()
    if cache is None:
        return
    phases = cache.setdefault("phases", {})
    phases[name] = float(phases.get(name, 0.0) or 0.0) + max(0.0, float(elapsed))
    _sample_pair_rss()


def _increment_pair_counter(name: str) -> None:
    cache = _pair_cache()
    if cache is None:
        return
    counters = cache.setdefault("counters", {})
    counters[name] = int(counters.get(name, 0) or 0) + 1


def _read_pairs_streaming(path: Path) -> tuple[list[tuple[int, str]], str, bool]:
    """Read ASCII DXF pairs without bytes+decoded-text+splitlines copies."""

    dxf_path = Path(path)
    try:
        with dxf_path.open("rb") as binary:
            had_bom = binary.read(3) == b"\xef\xbb\xbf"
    except OSError as exc:
        raise dv.DynamicVisibilityError(f"DXF kon niet worden gelezen: {dxf_path}: {exc}") from exc

    pairs: list[tuple[int, str]] = []
    newline = "\n"
    first_pair = True
    line_number = 0
    try:
        with dxf_path.open("r", encoding="utf-8-sig", newline="") as handle:
            while True:
                code_line = handle.readline()
                if code_line == "":
                    break
                line_number += 1
                value_line = handle.readline()
                if value_line == "":
                    raise dv.DynamicVisibilityError(
                        f"DXF heeft een oneven aantal regels en kan niet veilig worden aangepast: {dxf_path}"
                    )
                line_number += 1
                if first_pair:
                    newline = "\r\n" if code_line.endswith("\r\n") else "\n"
                    first_pair = False
                code_text = code_line.rstrip("\r\n")
                value = value_line.rstrip("\r\n")
                try:
                    code = int(code_text.strip())
                except ValueError as exc:
                    raise dv.DynamicVisibilityError(
                        f"Ongeldige DXF-groepcode op regel {line_number - 1}: {code_text!r}"
                    ) from exc
                pairs.append((code, value))
    except UnicodeError as exc:
        raise dv.DynamicVisibilityError(f"DXF is geen geldige UTF-8 tekst: {dxf_path}") from exc
    return pairs, newline, had_bom


def _write_pairs_streaming(
    path: Path,
    pairs: Iterable[tuple[int, str]],
    *,
    newline: str,
    had_bom: bool,
) -> None:
    """Write DXF pairs incrementally instead of constructing one giant string."""

    dxf_path = Path(path)
    temporary = dxf_path.with_name(f".{dxf_path.name}.dynamic.tmp")
    try:
        with temporary.open("w", encoding="utf-8", newline="") as handle:
            if had_bom:
                handle.write("\ufeff")
            for code, value in pairs:
                handle.write(f"{int(code):>3}{newline}")
                handle.write(str(value))
                handle.write(newline)
        temporary.replace(dxf_path)
    except Exception:
        try:
            temporary.unlink(missing_ok=True)
        except OSError:
            pass
        raise


def _iter_record_pairs(records: Iterable[Iterable[tuple[int, str]]]):
    for record in records:
        yield from record


def _set_handseed_in_records(records: list[list[tuple[int, str]]], value: int) -> None:
    for record in records:
        for index, (code, current) in enumerate(record[:-1]):
            if code != 9 or current.strip().upper() != "$HANDSEED":
                continue
            next_code, _next_value = record[index + 1]
            if next_code == 5:
                record[index + 1] = (5, f"{int(value):X}")
                return
    raise dv.DynamicVisibilityError("DXF-header bevat geen $HANDSEED.")


def _record_indexes(records: list[list[tuple[int, str]]], sections: list[str | None]):
    block_records: dict[str, tuple[int, list[tuple[int, str]]]] = {}
    nested_inserts: dict[str, dict[str, tuple[int, str]]] = {}
    object_by_handle: dict[str, tuple[int, list[tuple[int, str]]]] = {}
    object_children: dict[str, list[str]] = {}

    for index, (record, section) in enumerate(zip(records, sections)):
        record_type = dv._record_type(record)
        handle = (dv._record_handle(record) or "").upper()
        owner = (dv._record_owner(record) or "").upper()
        if section == "TABLES" and record_type == "BLOCK_RECORD":
            name = (dv._record_name(record) or "").upper()
            if name:
                block_records[name] = (index, record)
        elif section == "BLOCKS" and record_type == "INSERT" and owner:
            name = (dv._record_name(record) or "").upper()
            state = None
            if name.endswith("_NORMAAL_CONTENT"):
                state = reverse_patch.NORMAL_MODE
            elif name.endswith("_REVERSE_CONTENT"):
                state = reverse_patch.REVERSE_MODE
            if state and handle:
                nested_inserts.setdefault(owner, {})[state] = (index, handle)
        elif section == "OBJECTS" and handle:
            object_by_handle[handle] = (index, record)
            if owner:
                object_children.setdefault(owner, []).append(handle)

    return block_records, nested_inserts, object_by_handle, object_children


def _metadata_handles_from_xdict(xdict_handle: str, object_children: dict[str, list[str]]) -> set[str]:
    handles = {str(xdict_handle).upper()}
    pending = [str(xdict_handle).upper()]
    while pending:
        owner = pending.pop()
        for child in object_children.get(owner, ()):  # pragma: no branch - tiny BFS
            if child in handles:
                continue
            handles.add(child)
            pending.append(child)
    return handles


def _promote_records(
    records: list[list[tuple[int, str]]],
    sections: list[str | None],
    block_names: Iterable[str],
    *,
    next_handle: int,
) -> tuple[int, int]:
    donor = dv._discover_donor(records, sections)
    donor_handle_set = set(donor.metadata_handles)
    metadata_by_handle = {
        (dv._record_handle(record) or "").upper(): record
        for record, section in zip(records, sections)
        if section == "OBJECTS"
        and (dv._record_handle(record) or "").upper() in donor_handle_set
    }
    if set(metadata_by_handle) != donor_handle_set:
        missing = sorted(donor_handle_set - set(metadata_by_handle))
        raise dv.DynamicVisibilityError(
            f"Visibility-donor metadata is onvolledig; ontbrekende handles: {missing}"
        )

    block_records, nested_inserts, _object_by_handle, _object_children = _record_indexes(records, sections)
    insert_at = dv._objects_end_index(records, sections)
    promoted = 0

    for block_name in block_names:
        target = block_records.get(str(block_name).upper())
        if target is None:
            raise dv.DynamicVisibilityError(f"BLOCK_RECORD ontbreekt voor dynamic block {block_name!r}.")
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
        cloned_records = [
            dv._remap_metadata_record(
                metadata_by_handle[old_handle],
                handle_map=handle_map,
                donor=donor,
                target_block_record_handle=target_block_record_handle,
                normal_insert_handle=normal[1],
                reverse_insert_handle=reverse[1],
            )
            for old_handle in donor.metadata_handles
        ]
        records[insert_at:insert_at] = cloned_records
        sections[insert_at:insert_at] = ["OBJECTS"] * len(cloned_records)
        insert_at += len(cloned_records)
        promoted += 1

    return promoted, next_handle


def _selector_locations_from_document(document, block_names: Iterable[str]) -> dict[str, tuple[float, float, float]]:
    from ezdxf import bbox

    result: dict[str, tuple[float, float, float]] = {}
    for block_name in block_names:
        try:
            wrapper = document.blocks.get(block_name)
        except Exception:
            continue
        normal_ref = next(
            (
                entity
                for entity in wrapper.query("INSERT")
                if str(entity.dxf.name).upper().endswith("_NORMAAL_CONTENT")
            ),
            None,
        )
        if normal_ref is None:
            continue
        try:
            content = document.blocks.get(str(normal_ref.dxf.name))
        except Exception:
            continue
        all_entities = list(content)
        profile_entities = [entity for entity in all_entities if entity.dxftype() != "IMAGE"]
        candidates = profile_entities or all_entities
        if not candidates:
            continue
        try:
            bounds = bbox.extents(candidates, fast=True)
            if not bounds.has_data:
                continue
            minimum = bounds.extmin
            maximum = bounds.extmax
            width = max(0.0, float(maximum.x - minimum.x))
            height = max(0.0, float(maximum.y - minimum.y))
            span = max(width, height)
            margin = max(0.15, min(0.75, span * 0.08))
            insert = normal_ref.dxf.insert
            result[str(block_name)] = (
                float(maximum.x) + float(insert.x) + margin,
                float(maximum.y) + float(insert.y) + margin,
                float(maximum.z) + float(insert.z),
            )
        except Exception:
            continue
    return result


def _finalize_records(
    records: list[list[tuple[int, str]]],
    sections: list[str | None],
    block_names: Iterable[str],
    selector_locations: dict[str, tuple[float, float, float]],
) -> int:
    block_records, nested_inserts, object_by_handle, object_children = _record_indexes(records, sections)
    finalized = 0

    for block_name in block_names:
        target = block_records.get(str(block_name).upper())
        if target is None:
            raise dv.DynamicVisibilityError(f"BLOCK_RECORD ontbreekt voor dynamic block {block_name!r}.")
        block_index, block_record = target
        block_handle = (dv._record_handle(block_record) or "").upper()
        if not block_handle:
            raise dv.DynamicVisibilityError(
                f"Dynamic block {block_name!r} heeft geen BLOCK_RECORD-handle."
            )

        states = nested_inserts.get(block_handle, {})
        normal = states.get(reverse_patch.NORMAL_MODE)
        reverse = states.get(reverse_patch.REVERSE_MODE)
        if normal is None or reverse is None:
            raise dv.DynamicVisibilityError("Dynamic wrapper mist de NORMAAL/REVERSE state-INSERTs.")
        normal_index, _normal_handle = normal
        reverse_index, _reverse_handle = reverse
        records[normal_index] = finalize_patch._set_insert_initial_visibility(
            records[normal_index],
            state_index=0,
            visible=True,
        )
        records[reverse_index] = finalize_patch._set_insert_initial_visibility(
            records[reverse_index],
            state_index=1,
            visible=False,
        )

        target_grip = selector_locations.get(str(block_name))
        if target_grip is not None:
            xdict = finalize_patch._extension_dictionary_handle(records[block_index])
            if not xdict:
                raise dv.DynamicVisibilityError("Dynamic block mist een extension dictionary.")
            metadata_handles = _metadata_handles_from_xdict(xdict, object_children)
            parameter_index = None
            grip_index = None
            for handle in metadata_handles:
                item = object_by_handle.get(handle)
                if item is None:
                    continue
                index, record = item
                record_type = dv._record_type(record)
                if record_type == "BLOCKVISIBILITYPARAMETER":
                    parameter_index = index
                elif record_type == "BLOCKVISIBILITYGRIP":
                    grip_index = index
            if parameter_index is None or grip_index is None:
                raise dv.DynamicVisibilityError(
                    "Dynamic block mist de Visibility parameter of grip metadata."
                )
            current_grip = finalize_patch._point_1010(records[grip_index])
            if current_grip is not None:
                delta = (
                    target_grip[0] - current_grip[0],
                    target_grip[1] - current_grip[1],
                    target_grip[2] - current_grip[2],
                )
                records[grip_index] = finalize_patch._translate_point_1010(records[grip_index], delta)
                records[parameter_index] = finalize_patch._translate_point_1010(
                    records[parameter_index],
                    delta,
                )
        finalized += 1
    return finalized


def _inspect_records(
    records: list[list[tuple[int, str]]],
    sections: list[str | None],
    block_names: Iterable[str],
) -> dict[str, dict[str, object]]:
    block_records, _nested, object_by_handle, object_children = _record_indexes(records, sections)
    result: dict[str, dict[str, object]] = {}
    for block_name in block_names:
        name = str(block_name)
        target = block_records.get(name.upper())
        if target is None:
            raise dv.DynamicVisibilityError(f"BLOCK_RECORD ontbreekt voor dynamic block {name!r}.")
        _block_index, block_record = target
        block_handle = (dv._record_handle(block_record) or "").upper()
        block_rep_e_tag = tuple(
            (code, value.strip())
            for code, value in dv._xdata_payload(block_record, "AcDbBlockRepETag")
        )
        xdict = finalize_patch._extension_dictionary_handle(block_record)
        if not xdict:
            result[name] = {
                "block_name": name,
                "block_handle": block_handle,
                "is_dynamic": False,
                "property_name": None,
                "states": (),
                "block_rep_e_tag": block_rep_e_tag,
            }
            continue

        metadata_handles = _metadata_handles_from_xdict(xdict, object_children)
        visibility = None
        for handle in metadata_handles:
            item = object_by_handle.get(handle)
            if item is not None and dv._record_type(item[1]) == "BLOCKVISIBILITYPARAMETER":
                visibility = item[1]
                break
        if visibility is None:
            result[name] = {
                "block_name": name,
                "block_handle": block_handle,
                "is_dynamic": False,
                "property_name": None,
                "states": (),
                "block_rep_e_tag": block_rep_e_tag,
            }
            continue

        states = tuple(state_name for state_name, _refs in dv._visibility_states(visibility))
        result[name] = {
            "block_name": name,
            "block_handle": block_handle,
            "is_dynamic": True,
            "property_name": dv._first_value(visibility, 301),
            "states": states,
            "default_state": states[0] if states else None,
            "block_rep_e_tag": block_rep_e_tag,
        }
    return result


def inspect_dynamic_visibility_blocks(
    path: str | Path,
    block_names: Iterable[str],
) -> dict[str, dict[str, object]]:
    pairs, _newline, _had_bom = dv._read_pairs(Path(path))
    records = dv._split_records(pairs)
    del pairs
    sections = dv._record_sections(records)
    return _inspect_records(records, sections, list(block_names))


def inspect_dynamic_visibility_block(path: str | Path, block_name: str) -> dict[str, object]:
    return inspect_dynamic_visibility_blocks(path, [block_name])[str(block_name)]


def _promote_dynamic_visibility_blocks_optimized(
    path: str | Path,
    block_names: Iterable[str],
) -> int:
    names = list(block_names)
    dxf_path = Path(path)
    pairs, newline, had_bom = dv._read_pairs(dxf_path)
    next_handle = max(dv._max_handle_value(pairs) + 1, dv._header_handseed(pairs))
    records = dv._split_records(pairs)
    del pairs
    sections = dv._record_sections(records)
    promoted, next_handle = _promote_records(
        records,
        sections,
        names,
        next_handle=next_handle,
    )
    _set_handseed_in_records(records, next_handle)
    dv._write_pairs(
        dxf_path,
        _iter_record_pairs(records),
        newline=newline,
        had_bom=had_bom,
    )
    return promoted


def _finalize_dynamic_visibility_blocks_optimized(
    path: str | Path,
    block_names: Iterable[str],
) -> int:
    names = list(block_names)
    dxf_path = Path(path)
    # Standalone fallback: selector calculation still needs ezdxf, but the raw
    # finalization itself is a single streaming read/write pass.
    selector_locations = finalize_patch._selector_locations(dxf_path, names)
    pairs, newline, had_bom = dv._read_pairs(dxf_path)
    records = dv._split_records(pairs)
    del pairs
    sections = dv._record_sections(records)
    finalized = _finalize_records(records, sections, names, selector_locations)
    dv._write_pairs(
        dxf_path,
        _iter_record_pairs(records),
        newline=newline,
        had_bom=had_bom,
    )
    return finalized


def _validate_dynamic_details(details_by_name: dict[str, dict[str, object]], names: list[str]) -> None:
    for wrapper_name in names:
        details = details_by_name[wrapper_name]
        if not details.get("is_dynamic"):
            raise RuntimeError(f"Dynamic Visibility ontbreekt voor {wrapper_name}.")
        if details.get("property_name") != dynamic_patch.PROPERTY_NAME:
            raise RuntimeError(
                f"Dynamic property voor {wrapper_name} heet niet {dynamic_patch.PROPERTY_NAME!r}."
            )
        if tuple(details.get("states") or ()) != (
            dynamic_patch.NORMAL_STATE,
            dynamic_patch.REVERSE_STATE,
        ):
            raise RuntimeError(
                f"Visibility states voor {wrapper_name} zijn ongeldig: {details.get('states')!r}."
            )
        if details.get("default_state") != dynamic_patch.NORMAL_STATE:
            raise RuntimeError(
                f"{wrapper_name} start niet standaard in {dynamic_patch.NORMAL_STATE!r}."
            )


def _promote_exported_variants_one_pass(output_path: Path) -> list[str]:
    """Wrap, promote, finalize and validate all dynamic blocks with one raw parse."""

    import ezdxf

    start = time.perf_counter()
    document = ezdxf.readfile(output_path)
    wrapper_names = dynamic_patch._wrap_variant_pairs_as_static_blocks(document)
    for layer in document.layers:
        if dynamic_patch._variant_pair_key(layer.dxf.name) is not None:
            layer.on()

    selector_locations = _selector_locations_from_document(document, wrapper_names)
    working_path = dynamic_patch._working_dynamic_path(output_path)
    try:
        document.saveas(working_path)
        # Release the heavyweight ezdxf graph before building the raw record graph.
        del document

        pairs, newline, had_bom = dv._read_pairs(working_path)
        next_handle = max(dv._max_handle_value(pairs) + 1, dv._header_handseed(pairs))
        records = dv._split_records(pairs)
        del pairs
        sections = dv._record_sections(records)

        promoted, next_handle = _promote_records(
            records,
            sections,
            wrapper_names,
            next_handle=next_handle,
        )
        if promoted != len(wrapper_names):
            raise RuntimeError(
                f"Slechts {promoted} van {len(wrapper_names)} proefsleuven kregen Dynamic Visibility."
            )
        _set_handseed_in_records(records, next_handle)

        finalized = _finalize_records(
            records,
            sections,
            wrapper_names,
            selector_locations,
        )
        if finalized != len(wrapper_names):
            raise RuntimeError(
                f"Slechts {finalized} van {len(wrapper_names)} Dynamic Blocks zijn geïnitialiseerd."
            )

        details = _inspect_records(records, sections, wrapper_names)
        _validate_dynamic_details(details, wrapper_names)
        dv._write_pairs(
            working_path,
            _iter_record_pairs(records),
            newline=newline,
            had_bom=had_bom,
        )
        del records
        del sections

        # Preserve the former structural safety check, but do it once after all
        # raw mutations instead of before N separate inspections.
        validation_document = ezdxf.readfile(working_path)
        del validation_document
        working_path.replace(output_path)
    finally:
        try:
            working_path.unlink(missing_ok=True)
        except OSError:
            pass
    _add_pair_phase("dynamic_visibility", time.perf_counter() - start)
    return wrapper_names


def _make_reverse_image_definitions_unique_document(reverse_document) -> dict[str, str]:
    try:
        image_dict = reverse_document.rootdict.get_required_dict("ACAD_IMAGE_DICT")
    except Exception:
        return {}

    renamed: dict[str, str] = {}
    processed_handles: set[str] = set()
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
    return renamed


def _merge_reverse_variant_document_one_read(
    final_output_path: Path,
    reverse_source_path: Path,
) -> int:
    """Merge reverse content without the former rename-save-reopen cycle."""

    import ezdxf
    from ezdxf import xref

    started = time.perf_counter()
    target_document = ezdxf.readfile(final_output_path)
    reverse_document = ezdxf.readfile(reverse_source_path)
    _make_reverse_image_definitions_unique_document(reverse_document)
    reverse_patch._copy_reverse_image_assets(
        reverse_document,
        reverse_source_path,
        final_output_path,
    )

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
    _add_pair_phase("reverse_merge", time.perf_counter() - started)
    return len(reverse_inserts)


def _install_dynamic_visibility_merge_optimized() -> None:
    reverse_patch.install_template_reverse_export_patch()
    if getattr(reverse_patch, "_sleufbase_dynamic_visibility_patch", False):
        return

    def _merge_with_native_dynamic_visibility(final_output_path, reverse_source_path):
        merged_count = _merge_reverse_variant_document_one_read(
            Path(final_output_path),
            Path(reverse_source_path),
        )
        _promote_exported_variants_one_pass(Path(final_output_path))
        return merged_count

    reverse_patch._merge_reverse_variant_document = _merge_with_native_dynamic_visibility
    reverse_patch._sleufbase_dynamic_visibility_patch = True

    from .cadastral_export import CadastralDxfExporter

    CadastralDxfExporter.SLEUFBASE_DYNAMIC_VISIBILITY_DEFAULT = True
    CadastralDxfExporter.SLEUFBASE_DYNAMIC_VISIBILITY_PROPERTY = dynamic_patch.PROPERTY_NAME
    CadastralDxfExporter.SLEUFBASE_DYNAMIC_VISIBILITY_STATES = (
        dynamic_patch.NORMAL_STATE,
        dynamic_patch.REVERSE_STATE,
    )
    CadastralDxfExporter._sleufbase_dynamic_visibility_patch = True


def _estimate_virtual_tiff_pixels(task_kwargs: dict[str, object]) -> int:
    layer = task_kwargs.get("layer")
    image = getattr(layer, "image", None)
    try:
        width, height = image.size
        width = max(1, int(width))
        height = max(1, int(height))
    except Exception:
        return VIRTUAL_TIFF_PIXEL_BUDGET
    multiplier = float(
        getattr(
            task_kwargs.get("exporter"),
            "VIRTUAL_TRENCH_EXPORT_QUALITY_MULTIPLIER",
            2.5,
        )
        or 2.5
    )
    estimated_width = min(4000, max(width, int(round(width * multiplier))))
    estimated_height = min(4000, max(height, int(round(height * multiplier))))
    return max(1, estimated_width * estimated_height)


class _PixelBudget:
    def __init__(self, budget: int) -> None:
        self.budget = max(1, int(budget))
        self.used = 0
        self.condition = threading.Condition()

    def acquire(self, requested: int) -> int:
        weight = max(1, min(self.budget, int(requested)))
        with self.condition:
            while self.used and (self.used + weight) > self.budget:
                self.condition.wait()
            self.used += weight
        return weight

    def release(self, weight: int) -> None:
        with self.condition:
            self.used = max(0, self.used - int(weight))
            self.condition.notify_all()


def _build_heavy_with_budget(exporter, task_kwargs: dict[str, object], budget: _PixelBudget):
    local_kwargs = dict(task_kwargs)
    local_kwargs["exporter"] = exporter
    weight = budget.acquire(_estimate_virtual_tiff_pixels(local_kwargs))
    try:
        return perf_patch._build_heavy_template_raster(exporter, task_kwargs)
    finally:
        budget.release(weight)
        _sample_pair_rss()


def _install_overlapped_asset_pipeline(exporter_class, prepared_assets_type) -> None:
    previous_batch = exporter_class._prepare_template_slot_assets_batch

    def _prepare_template_slot_assets_batch_overlapped(
        self,
        tasks: list[tuple[int, dict[str, object]]],
        *,
        status_callback=None,
    ):
        if not tasks or not _contains_virtual_template_task(tasks):
            return previous_batch(self, tasks, status_callback=status_callback)

        started = time.perf_counter()
        light_workers = max(1, min(perf_patch.MAX_VIRTUAL_TEMPLATE_MAP_WORKERS, len(tasks)))
        heavy_workers = max(1, min(MAX_ADAPTIVE_TIFF_WORKERS, len(tasks)))
        budget = _PixelBudget(VIRTUAL_TIFF_PIXEL_BUDGET)
        light_assets: dict[int, Any] = {}
        raster_paths: dict[int, Any] = {}

        with (
            ThreadPoolExecutor(
                max_workers=light_workers,
                thread_name_prefix="template-maps-virtual",
            ) as light_executor,
            ThreadPoolExecutor(
                max_workers=heavy_workers,
                thread_name_prefix="template-tiff-virtual",
            ) as heavy_executor,
        ):
            future_info = {}
            for layer_index, task_kwargs in tasks:
                future_info[
                    light_executor.submit(perf_patch._prepare_light_template_assets, self, task_kwargs)
                ] = ("light", layer_index)
                future_info[
                    heavy_executor.submit(_build_heavy_with_budget, self, task_kwargs, budget)
                ] = ("heavy", layer_index)

            pending = set(future_info)
            completed_light = 0
            completed_heavy = 0
            while pending:
                done, pending = wait(
                    pending,
                    timeout=TEMPLATE_UI_PUMP_INTERVAL_SECONDS,
                    return_when=FIRST_COMPLETED,
                )
                if not done:
                    _pump_template_ui(status_callback)
                    _sample_pair_rss()
                    continue
                for future in done:
                    kind, layer_index = future_info[future]
                    result = future.result()
                    if kind == "light":
                        light_assets[layer_index] = result
                        completed_light += 1
                        perf_patch._safe_status(
                            status_callback,
                            f"Bereid sjabloonkaarten voor... {completed_light}/{len(tasks)}",
                        )
                    else:
                        raster_paths[layer_index] = result
                        completed_heavy += 1
                        perf_patch._safe_status(
                            status_callback,
                            f"Bereid hoge-res TIFF-afbeeldingen voor... {completed_heavy}/{len(tasks)}",
                        )
                _pump_template_ui(status_callback)
                _sample_pair_rss()

        prepared = {}
        for layer_index, _task_kwargs in tasks:
            light = light_assets[layer_index]
            prepared[layer_index] = prepared_assets_type(
                formatted_address=light["formatted_address"],
                comments_text=light["comments_text"],
                raster_path=raster_paths[layer_index],
                map_raster_path=light["map_raster_path"],
            )
        _add_pair_phase("template_assets", time.perf_counter() - started)
        return prepared

    exporter_class._prepare_template_slot_assets_batch = _prepare_template_slot_assets_batch_overlapped


def _bounds_key(value: object):
    try:
        return (
            round(float(value.min_x), 6),
            round(float(value.min_y), 6),
            round(float(value.max_x), 6),
            round(float(value.max_y), 6),
        )
    except Exception:
        return None


def _freeze(value: object):
    bounds = _bounds_key(value)
    if bounds is not None:
        return ("bounds", bounds)
    if value is None or isinstance(value, (bool, int, float, str)):
        return value
    if isinstance(value, Path):
        return str(value)
    if type(value).__name__ == "ProfileReferenceAnnotation":
        try:
            return (
                "reference_annotation",
                round(float(value.x), 6),
                round(float(value.y), 6),
            )
        except Exception:
            pass
    if isinstance(value, (tuple, list)):
        return tuple(_freeze(item) for item in value)
    if isinstance(value, dict):
        return tuple(sorted((str(key), _freeze(item)) for key, item in value.items()))
    return (type(value).__name__, id(value))


def _cache_lookup(key: object):
    cache = _pair_cache()
    if cache is None:
        return False, None
    values = cache.setdefault("values", {})
    if key in values:
        _increment_pair_counter("cache_hits")
        return True, values[key]
    _increment_pair_counter("cache_misses")
    return False, values


def _install_template_slot_cache(exporter_class) -> None:
    original_detect = exporter_class._detect_template_slots

    def _detect_template_slots_cached(self, document):
        filename = str(getattr(document, "filename", "") or "").strip()
        if not filename:
            return original_detect(self, document)
        path = Path(filename)
        try:
            stat = path.stat()
        except OSError:
            return original_detect(self, document)
        # The exporter may extend the in-memory template with additional pages
        # before asking for slots again.  File metadata stays unchanged during
        # that operation, so include document topology in the key; otherwise
        # the original slot list is returned forever and extension never ends.
        try:
            modelspace_entity_count = len(document.modelspace())
        except Exception:
            modelspace_entity_count = -1
        try:
            layout_count = len(document.layouts)
        except Exception:
            layout_count = -1
        key = (
            str(path.resolve()),
            int(stat.st_mtime_ns),
            int(stat.st_size),
            int(modelspace_entity_count),
            int(layout_count),
        )
        with _TEMPLATE_SLOT_CACHE_LOCK:
            cached = _TEMPLATE_SLOT_CACHE.get(key)
        if cached is not None:
            _increment_pair_counter("template_slot_cache_hits")
            return list(cached)
        slots = list(original_detect(self, document))
        with _TEMPLATE_SLOT_CACHE_LOCK:
            # Keep this deliberately tiny: normal and reverse use the same entry,
            # while template upgrades naturally invalidate by mtime/size.
            if len(_TEMPLATE_SLOT_CACHE) >= 2:
                _TEMPLATE_SLOT_CACHE.clear()
            _TEMPLATE_SLOT_CACHE[key] = tuple(slots)
        _increment_pair_counter("template_slot_cache_misses")
        return slots

    exporter_class._detect_template_slots = _detect_template_slots_cached


def _install_pair_preparation_cache(exporter_class) -> None:
    original_server = exporter_class._fetch_template_server_data_single
    original_paths = exporter_class._fetch_template_paths_for_bounds_with_empty
    original_address = exporter_class._reverse_geocoded_template_address
    original_map = exporter_class._build_template_map_raster

    def _fetch_template_server_data_single_cached(self, bounds, *args, **kwargs):
        orientation = kwargs.get("orientation_bounds")
        key = (
            "server",
            _bounds_key(bounds),
            _freeze(orientation),
            _freeze(args),
            _freeze({k: v for k, v in kwargs.items() if k != "status_callback"}),
        )
        hit, value = _cache_lookup(key)
        if hit:
            return value
        started = time.perf_counter()
        result = original_server(self, bounds, *args, **kwargs)
        if isinstance(value, dict):
            value[key] = result
        _add_pair_phase("server_data", time.perf_counter() - started)
        return result

    def _fetch_template_paths_cached(
        self,
        client,
        bounds_list,
        *,
        status_callback=None,
        status_label: str,
    ):
        bounds = list(bounds_list)
        key = ("paths", _freeze(bounds), str(status_label))
        hit, value = _cache_lookup(key)
        if hit:
            return value
        started = time.perf_counter()
        result = original_paths(
            self,
            client,
            bounds,
            status_callback=status_callback,
            status_label=status_label,
        )
        if isinstance(value, dict):
            value[key] = result
        _add_pair_phase("orientation_paths", time.perf_counter() - started)
        return result

    def _reverse_geocoded_template_address_cached(self, layer, location_client):
        key = ("address", id(layer))
        hit, value = _cache_lookup(key)
        if hit:
            return value
        started = time.perf_counter()
        result = original_address(self, layer, location_client)
        if isinstance(value, dict):
            value[key] = result
        _add_pair_phase("reverse_geocode", time.perf_counter() - started)
        return result

    def _build_template_map_raster_cached(
        self,
        asset_dir,
        layer,
        label,
        index,
        *args,
        **kwargs,
    ):
        key = (
            "map",
            id(layer),
            str(label),
            int(index),
            _freeze(args),
            _freeze(
                {
                    key: value
                    for key, value in kwargs.items()
                    if key not in {"page_exporter", "background_provider"}
                }
            ),
        )
        hit, value = _cache_lookup(key)
        if hit:
            source = Path(value)
            if source.exists():
                return source
        started = time.perf_counter()
        result = original_map(self, asset_dir, layer, label, index, *args, **kwargs)
        cache = _pair_cache()
        if cache is not None:
            cache.setdefault("values", {})[key] = Path(result)
        _add_pair_phase("map_raster", time.perf_counter() - started)
        return result

    exporter_class._fetch_template_server_data_single = _fetch_template_server_data_single_cached
    exporter_class._fetch_template_paths_for_bounds_with_empty = _fetch_template_paths_cached
    exporter_class._reverse_geocoded_template_address = _reverse_geocoded_template_address_cached
    exporter_class._build_template_map_raster = _build_template_map_raster_cached


def _install_reverse_pair_session() -> None:
    original_call = reverse_patch._call_original_export
    original_cleanup = reverse_patch._cleanup_reverse_source
    original_ensure = reverse_patch._ensure_variant_container

    def _call_original_export_with_pair_session(
        original_export,
        instance,
        call_arguments,
        *,
        mode,
        output_path,
        reverse_cross_sections,
    ):
        normalized_mode = str(mode).upper()
        if normalized_mode == reverse_patch.NORMAL_MODE:
            previous_cache = getattr(reverse_patch._EXPORT_CONTEXT, "pair_cache", None)
            cache = {
                "values": {},
                "phases": {},
                "counters": {},
                "started": time.perf_counter(),
                "rss_start_bytes": _current_rss_bytes(),
                "peak_rss_bytes": 0,
                "exporter": instance,
                "previous_cache": previous_cache,
            }
            reverse_patch._EXPORT_CONTEXT.pair_cache = cache
            _sample_pair_rss()
        previous_container_cache = getattr(reverse_patch._EXPORT_CONTEXT, "container_cache", None)
        reverse_patch._EXPORT_CONTEXT.container_cache = {}
        started = time.perf_counter()
        try:
            return original_call(
                original_export,
                instance,
                call_arguments,
                mode=mode,
                output_path=output_path,
                reverse_cross_sections=reverse_cross_sections,
            )
        except Exception:
            if normalized_mode == reverse_patch.NORMAL_MODE:
                cache = _pair_cache()
                previous_cache = cache.get("previous_cache") if cache else None
                if previous_cache is None:
                    reverse_patch._EXPORT_CONTEXT.__dict__.pop("pair_cache", None)
                else:
                    reverse_patch._EXPORT_CONTEXT.pair_cache = previous_cache
            raise
        finally:
            if previous_container_cache is None:
                reverse_patch._EXPORT_CONTEXT.__dict__.pop("container_cache", None)
            else:
                reverse_patch._EXPORT_CONTEXT.container_cache = previous_container_cache
            _add_pair_phase(
                "normal_export" if normalized_mode == reverse_patch.NORMAL_MODE else "reverse_export",
                time.perf_counter() - started,
            )

    def _cleanup_reverse_source_and_finish_metrics(reverse_source_path: Path) -> None:
        try:
            original_cleanup(reverse_source_path)
        finally:
            cache = _pair_cache()
            if cache is None:
                return
            _sample_pair_rss()
            elapsed = time.perf_counter() - float(cache.get("started", time.perf_counter()))
            exporter = cache.get("exporter")
            metrics = {
                "total_seconds": elapsed,
                "phases": dict(cache.get("phases") or {}),
                "counters": dict(cache.get("counters") or {}),
                "rss_start_bytes": int(cache.get("rss_start_bytes", 0) or 0),
                "peak_rss_bytes": int(cache.get("peak_rss_bytes", 0) or 0),
            }
            if exporter is not None:
                try:
                    exporter._sleufbase_last_template_pair_metrics = metrics
                except Exception:
                    pass
            _LOGGER.info(
                "DXF-sjabloon export klaar in %.3fs; peak RSS %.1f MiB; cache %s",
                elapsed,
                metrics["peak_rss_bytes"] / (1024.0 * 1024.0) if metrics["peak_rss_bytes"] else 0.0,
                metrics["counters"],
            )
            previous_cache = cache.get("previous_cache")
            if previous_cache is None:
                reverse_patch._EXPORT_CONTEXT.__dict__.pop("pair_cache", None)
            else:
                reverse_patch._EXPORT_CONTEXT.pair_cache = previous_cache

    def _ensure_variant_container_cached(document, modelspace, label, slot_index, mode):
        cache = getattr(reverse_patch._EXPORT_CONTEXT, "container_cache", None)
        if cache is None:
            cache = {}
            reverse_patch._EXPORT_CONTEXT.container_cache = cache
        layer_name = reverse_patch.variant_layer_name(label, slot_index, mode)
        block_name = reverse_patch.variant_block_name(label, slot_index, mode)
        key = (id(document), block_name, layer_name)
        cached = cache.get(key)
        if cached is not None:
            block, insert = cached
            if getattr(insert, "is_alive", True):
                return block, insert
        block, insert = original_ensure(document, modelspace, label, slot_index, mode)
        cache[key] = (block, insert)
        return block, insert

    reverse_patch._call_original_export = _call_original_export_with_pair_session
    reverse_patch._cleanup_reverse_source = _cleanup_reverse_source_and_finish_metrics
    reverse_patch._ensure_variant_container = _ensure_variant_container_cached


def get_last_template_export_metrics(exporter) -> dict[str, object]:
    metrics = getattr(exporter, "_sleufbase_last_template_pair_metrics", None)
    return dict(metrics) if isinstance(metrics, dict) else {}


def install_dxf_template_pipeline_patch() -> None:
    """Install DXF template speed and memory optimizations as one coherent layer."""

    from .cadastral_export import CadastralDxfExporter, PreparedTemplateSlotAssets

    with _INSTALL_LOCK:
        if int(getattr(CadastralDxfExporter, "_sleufbase_dxf_template_pipeline_version", 0) or 0) >= PATCH_VERSION:
            return

        # Replace full-file temporary copies in the raw DXF editor.
        dv._read_pairs = _read_pairs_streaming
        dv._write_pairs = _write_pairs_streaming
        dv.promote_dynamic_visibility_blocks = _promote_dynamic_visibility_blocks_optimized
        dv.inspect_dynamic_visibility_block = inspect_dynamic_visibility_block
        dv.inspect_dynamic_visibility_blocks = inspect_dynamic_visibility_blocks

        # Imported function globals in template_dynamic_visibility_patch must be
        # replaced explicitly; assigning only on dv would not affect them.
        dynamic_patch.promote_dynamic_visibility_blocks = _promote_dynamic_visibility_blocks_optimized
        dynamic_patch.inspect_dynamic_visibility_block = inspect_dynamic_visibility_block
        dynamic_patch._promote_exported_variants_to_dynamic_blocks = _promote_exported_variants_one_pass
        dynamic_patch.install_template_dynamic_visibility_patch = _install_dynamic_visibility_merge_optimized
        finalize_patch.finalize_dynamic_visibility_blocks = _finalize_dynamic_visibility_blocks_optimized

        _install_overlapped_asset_pipeline(CadastralDxfExporter, PreparedTemplateSlotAssets)
        _install_template_slot_cache(CadastralDxfExporter)
        _install_pair_preparation_cache(CadastralDxfExporter)
        _install_reverse_pair_session()

        CadastralDxfExporter._sleufbase_dxf_template_pipeline_version = PATCH_VERSION
        CadastralDxfExporter.SLEUFBASE_ADAPTIVE_TIFF_PIXEL_BUDGET = VIRTUAL_TIFF_PIXEL_BUDGET
        CadastralDxfExporter.SLEUFBASE_ADAPTIVE_TIFF_WORKERS = MAX_ADAPTIVE_TIFF_WORKERS
