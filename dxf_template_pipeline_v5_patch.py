from __future__ import annotations

from dataclasses import replace
from math import floor, hypot
from typing import Any

from . import dxf_template_pipeline_patch as pipeline
from . import dxf_template_pipeline_v4_patch as pipeline_v4
from . import template_reverse_patch as reverse_patch


PATCH_VERSION = 1
_CONTEXT_ATTR = "_sleufbase_profile_reuse_context_v5"
_UNSUPPORTED = object()


def _state(exporter: Any) -> dict[str, Any] | None:
    session = pipeline_v4._session(exporter)
    if session is None:
        return None
    lock = session.get("lock")
    if lock is None:
        return None
    with lock:
        return session.setdefault(
            "profile_v5",
            {
                "dataset_layers": {},
                "ready_profiles": {},
                "normal_base_profiles": {},
                "normal_final_profiles": {},
                "fingerprints": {},
            },
        )


def _rounded(value: object) -> float | None:
    if value is None:
        return None
    try:
        return round(float(value), 8)
    except (TypeError, ValueError):
        return None


def _dataset_fingerprint(dataset: Any, state: dict[str, Any] | None = None) -> tuple[Any, ...]:
    cache = state.get("fingerprints") if isinstance(state, dict) else None
    cache_key = id(dataset)
    if isinstance(cache, dict) and cache_key in cache:
        return cache[cache_key]

    points = tuple(
        (
            str(getattr(point, "object_name", "") or ""),
            str(getattr(point, "source_name", "") or ""),
            _rounded(getattr(point, "x", None)),
            _rounded(getattr(point, "y", None)),
            _rounded(getattr(point, "z", None)),
            str(getattr(point, "attribute_1", "") or ""),
            str(getattr(point, "attribute_2", "") or ""),
            str(getattr(point, "attribute_3", "") or ""),
        )
        for point in tuple(getattr(dataset, "points", ()) or ())
    )
    polylines = tuple(
        (
            str(getattr(polyline, "object_name", "") or ""),
            str(getattr(polyline, "source_name", "") or ""),
            tuple(
                (
                    _rounded(getattr(vertex, "x", None)),
                    _rounded(getattr(vertex, "y", None)),
                    _rounded(getattr(vertex, "z", None)),
                )
                for vertex in tuple(getattr(polyline, "vertices", ()) or ())
            ),
            str(getattr(polyline, "attribute_1", "") or ""),
            str(getattr(polyline, "attribute_2", "") or ""),
            str(getattr(polyline, "attribute_3", "") or ""),
        )
        for polyline in tuple(getattr(dataset, "polylines", ()) or ())
    )
    result = (points, polylines)
    if isinstance(cache, dict):
        cache[cache_key] = result
    return result


def _reverse_profile_from_normal(profile: Any):
    """Create the mathematically identical opposite-direction profile.

    This is exact for the normal core path without a forced start point: the core
    exporter only swaps the selected axis endpoints and all projected chainages
    become ``axis_length - chainage``.
    """

    from .cadastral_export import TemplateCrossSectionProfile

    axis_length = float(profile.axis_length)
    mirrored_points = [
        replace(point, chainage=axis_length - float(point.chainage))
        for point in tuple(profile.points)
    ]
    mirrored_points.sort(
        key=lambda item: (item.chainage, 1 if item.is_endpoint else 0, item.description)
    )
    return TemplateCrossSectionProfile(
        start_point=profile.end_point,
        end_point=profile.start_point,
        axis_dx=-float(profile.axis_dx),
        axis_dy=-float(profile.axis_dy),
        axis_length=axis_length,
        reference_level=float(profile.reference_level),
        points=tuple(mirrored_points),
    )


def _build_forced_profile_fast(
    exporter: Any,
    dataset: Any,
    layer_rules: tuple[Any, ...],
    fallback_marker_scale: float,
):
    """Core-equivalent profile builder for datasets with an explicit start point.

    The normal core implementation still calculates a preferred road/terrain
    projection even though a forced start makes that result unusable. MarXact
    always supplies such a start point. This path deliberately skips only that
    dead orientation work and otherwise mirrors the core algorithm.
    """

    from .cadastral_export import TemplateCrossSectionPoint, TemplateCrossSectionProfile
    from .kickthemap_dxf_export import KickTheMapObjectPoint

    points_with_z = [point for point in dataset.points if point.z is not None]
    if len(points_with_z) < 2:
        return None

    endpoint_candidates = exporter._cross_section_endpoint_candidates(points_with_z, layer_rules)
    if len(endpoint_candidates) < 2:
        endpoint_candidates = points_with_z
    forced_start_point = exporter._dataset_forced_cross_section_start_point(
        dataset, endpoint_candidates
    )
    if forced_start_point is None:
        return _UNSUPPORTED

    start_point = forced_start_point
    remaining_candidates = [
        point
        for point in endpoint_candidates
        if not exporter._same_job_point(point, start_point)
    ]
    if not remaining_candidates:
        return None
    end_point = max(
        remaining_candidates,
        key=lambda point: hypot(point.x - start_point.x, point.y - start_point.y),
    )
    base_axis_dx = float(end_point.x) - float(start_point.x)
    base_axis_dy = float(end_point.y) - float(start_point.y)
    base_axis_length = hypot(base_axis_dx, base_axis_dy)
    if base_axis_length <= 1e-6:
        return None

    unit_dx = base_axis_dx / base_axis_length
    unit_dy = base_axis_dy / base_axis_length
    features_with_z: list[Any] = list(points_with_z)
    features_with_z.extend(
        polyline
        for polyline in dataset.polylines
        if exporter._cross_section_feature_position(polyline) is not None
    )

    raw_entries: list[tuple[Any, float, float]] = []
    feature_chainage_ranges: list[tuple[float, float]] = []
    for point in features_with_z:
        feature_position = exporter._cross_section_feature_position(point)
        if feature_position is None:
            continue
        feature_x, feature_y, feature_z = feature_position
        chainage = (
            ((float(feature_x) - float(start_point.x)) * base_axis_dx)
            + ((float(feature_y) - float(start_point.y)) * base_axis_dy)
        ) / base_axis_length
        raw_entries.append((point, chainage, float(feature_z)))
        chainage_range = exporter._cross_section_feature_chainage_range(
            point,
            axis_start_x=float(start_point.x),
            axis_start_y=float(start_point.y),
            axis_dx=base_axis_dx,
            axis_dy=base_axis_dy,
            axis_length=base_axis_length,
            fallback_marker_scale=fallback_marker_scale,
        )
        if chainage_range is not None:
            feature_chainage_ranges.append(chainage_range)

    if not raw_entries:
        return None
    if feature_chainage_ranges:
        max_feature_chainage = max(item[1] for item in feature_chainage_ranges)
    else:
        max_feature_chainage = max(entry[1] for entry in raw_entries)

    boundary_start_chainage = 0.0
    boundary_end_chainage = max(base_axis_length, max_feature_chainage)
    axis_length = boundary_end_chainage
    if axis_length <= 1e-6:
        return None

    synthetic_start_point = KickTheMapObjectPoint(
        object_name=start_point.object_name,
        source_name=start_point.source_name,
        x=float(start_point.x),
        y=float(start_point.y),
        z=start_point.z,
        attribute_1=start_point.attribute_1,
        attribute_2=start_point.attribute_2,
        attribute_3=start_point.attribute_3,
    )
    synthetic_end_point = KickTheMapObjectPoint(
        object_name=end_point.object_name,
        source_name=end_point.source_name,
        x=float(start_point.x + (unit_dx * boundary_end_chainage)),
        y=float(start_point.y + (unit_dy * boundary_end_chainage)),
        z=end_point.z,
        attribute_1=end_point.attribute_1,
        attribute_2=end_point.attribute_2,
        attribute_3=end_point.attribute_3,
    )
    axis_dx = float(synthetic_end_point.x) - float(synthetic_start_point.x)
    axis_dy = float(synthetic_end_point.y) - float(synthetic_start_point.y)

    ordered_points: list[Any] = [
        TemplateCrossSectionPoint(
            point=synthetic_start_point,
            chainage=0.0,
            point_z=float(synthetic_start_point.z or 0.0),
            layer_name="0",
            color=7,
            description=exporter._cross_section_description(synthetic_start_point, "0"),
            is_endpoint=True,
        ),
        TemplateCrossSectionPoint(
            point=synthetic_end_point,
            chainage=axis_length,
            point_z=float(synthetic_end_point.z or 0.0),
            layer_name="0",
            color=7,
            description=exporter._cross_section_description(synthetic_end_point, "0"),
            is_endpoint=True,
        ),
    ]

    for point, raw_chainage, feature_z in raw_entries:
        if exporter._same_job_point(point, start_point) or exporter._same_job_point(
            point, end_point
        ):
            continue
        matched_rule = exporter._resolve_cross_section_point_rule(point, layer_rules)
        if matched_rule is None:
            if exporter._is_dekband_feature(point):
                layer_name, color, profile_label = (
                    "0",
                    exporter.TEMPLATE_DEKBAND_COLOR,
                    "Dekband",
                )
            else:
                layer_name, color, profile_label = "0", 7, ""
        else:
            layer_name = matched_rule.target_layer
            color = matched_rule.color
            profile_label = matched_rule.profile_label
        ordered_points.append(
            TemplateCrossSectionPoint(
                point=point,
                chainage=float(raw_chainage - boundary_start_chainage),
                point_z=float(feature_z),
                layer_name=layer_name,
                color=color,
                description=exporter._cross_section_description(
                    point, layer_name, profile_label
                ),
                is_endpoint=False,
            )
        )

    ordered_points.sort(
        key=lambda item: (item.chainage, 1 if item.is_endpoint else 0, item.description)
    )
    if not ordered_points:
        return None
    min_z = min(item.point_z for item in ordered_points)
    reference_level = round(
        (floor(min_z * 10.0) / 10.0) - exporter.TEMPLATE_PROFILE_REFERENCE_MARGIN,
        2,
    )
    return TemplateCrossSectionProfile(
        start_point=synthetic_start_point,
        end_point=synthetic_end_point,
        axis_dx=axis_dx,
        axis_dy=axis_dy,
        axis_length=axis_length,
        reference_level=reference_level,
        points=tuple(ordered_points),
    )


def _polyline_stays_inside_profile(exporter: Any, profile: Any, polyline: Any) -> bool:
    if float(profile.axis_length) <= 1e-9:
        return False
    chainage_range = exporter._cross_section_feature_chainage_range(
        polyline,
        axis_start_x=float(profile.start_point.x),
        axis_start_y=float(profile.start_point.y),
        axis_dx=float(profile.axis_dx),
        axis_dy=float(profile.axis_dy),
        axis_length=float(profile.axis_length),
        fallback_marker_scale=0.005,
    )
    if chainage_range is None:
        return False
    tolerance = 1e-6
    return (
        float(chainage_range[0]) >= -tolerance
        and float(chainage_range[1]) <= float(profile.axis_length) + tolerance
    )


def _augment_profile_with_polylines(
    exporter: Any,
    profile: Any,
    polylines: tuple[Any, ...],
    layer_rules: tuple[Any, ...],
):
    """Add newly generated deckband features without rebuilding the whole axis."""

    from .cadastral_export import TemplateCrossSectionPoint, TemplateCrossSectionProfile

    if not polylines:
        return profile
    if any(not _polyline_stays_inside_profile(exporter, profile, item) for item in polylines):
        return None

    ordered_points = list(profile.points)
    axis_length = float(profile.axis_length)
    for point in polylines:
        feature_position = exporter._cross_section_feature_position(point)
        if feature_position is None:
            return None
        feature_x, feature_y, feature_z = feature_position
        chainage = (
            ((float(feature_x) - float(profile.start_point.x)) * float(profile.axis_dx))
            + ((float(feature_y) - float(profile.start_point.y)) * float(profile.axis_dy))
        ) / axis_length
        matched_rule = exporter._resolve_cross_section_point_rule(point, layer_rules)
        if matched_rule is None:
            if exporter._is_dekband_feature(point):
                layer_name, color, profile_label = (
                    "0",
                    exporter.TEMPLATE_DEKBAND_COLOR,
                    "Dekband",
                )
            else:
                layer_name, color, profile_label = "0", 7, ""
        else:
            layer_name = matched_rule.target_layer
            color = matched_rule.color
            profile_label = matched_rule.profile_label
        ordered_points.append(
            TemplateCrossSectionPoint(
                point=point,
                chainage=float(chainage),
                point_z=float(feature_z),
                layer_name=layer_name,
                color=color,
                description=exporter._cross_section_description(
                    point, layer_name, profile_label
                ),
                is_endpoint=False,
            )
        )

    ordered_points.sort(
        key=lambda item: (item.chainage, 1 if item.is_endpoint else 0, item.description)
    )
    min_z = min(item.point_z for item in ordered_points)
    reference_level = round(
        (floor(min_z * 10.0) / 10.0) - exporter.TEMPLATE_PROFILE_REFERENCE_MARGIN,
        2,
    )
    return TemplateCrossSectionProfile(
        start_point=profile.start_point,
        end_point=profile.end_point,
        axis_dx=float(profile.axis_dx),
        axis_dy=float(profile.axis_dy),
        axis_length=axis_length,
        reference_level=reference_level,
        points=tuple(ordered_points),
    )


def install_dxf_template_pipeline_v5_patch() -> None:
    """Reuse profile geometry between normal and reverse DXF template variants."""

    from .cadastral_export import CadastralDxfExporter

    pipeline_v4.install_dxf_template_pipeline_v4_patch()
    if int(
        getattr(CadastralDxfExporter, "_sleufbase_dxf_template_pipeline_v5_version", 0)
        or 0
    ) >= PATCH_VERSION:
        return

    previous_profile = CadastralDxfExporter._build_template_cross_section_profile
    previous_dekband = CadastralDxfExporter._dataset_with_template_dekbanden

    def _build_template_cross_section_profile_v5(
        self,
        dataset,
        layer_rules,
        road_centerline_paths,
        terrain_boundary_paths,
        fallback_marker_scale,
        reverse_profile_direction=False,
    ):
        state = _state(self)
        mode = pipeline_v4._mode(self)
        if state is None or mode not in {reverse_patch.NORMAL_MODE, reverse_patch.REVERSE_MODE}:
            return previous_profile(
                self,
                dataset,
                layer_rules,
                road_centerline_paths,
                terrain_boundary_paths,
                fallback_marker_scale,
                reverse_profile_direction,
            )

        ready_key = (id(dataset), bool(reverse_profile_direction))
        with pipeline_v4._session(self)["lock"]:
            ready = state["ready_profiles"].get(ready_key, _UNSUPPORTED)
        if ready is not _UNSUPPORTED:
            pipeline._increment_pair_counter("template_profile_dekband_rebuild_skipped")
            return ready

        context = getattr(self, _CONTEXT_ATTR, None)
        layer_id = None
        is_base_profile = False
        if isinstance(context, dict):
            layer_id = context.get("layer_id")
            is_base_profile = bool(context.get("is_base_profile"))
        if layer_id is None:
            with pipeline_v4._session(self)["lock"]:
                layer_id = state["dataset_layers"].get(id(dataset))

        forced_result = _UNSUPPORTED
        if getattr(dataset, "cross_section_start_xy", None) is not None:
            forced_result = _build_forced_profile_fast(
                self,
                dataset,
                tuple(layer_rules),
                float(fallback_marker_scale),
            )
        if forced_result is not _UNSUPPORTED:
            profile = forced_result
            pipeline._increment_pair_counter(
                "forced_profile_fast_path_reverse"
                if mode == reverse_patch.REVERSE_MODE
                else "forced_profile_fast_path_normal"
            )
        elif mode == reverse_patch.REVERSE_MODE and bool(reverse_profile_direction) and layer_id is not None:
            cache_name = "normal_base_profiles" if is_base_profile else "normal_final_profiles"
            with pipeline_v4._session(self)["lock"]:
                cached = state[cache_name].get(layer_id)
            if cached is not None and cached[0] == _dataset_fingerprint(dataset, state):
                profile = _reverse_profile_from_normal(cached[1]) if cached[1] is not None else None
                pipeline._increment_pair_counter("reverse_profile_derived_from_normal")
            else:
                profile = previous_profile(
                    self,
                    dataset,
                    layer_rules,
                    road_centerline_paths,
                    terrain_boundary_paths,
                    fallback_marker_scale,
                    reverse_profile_direction,
                )
                pipeline._increment_pair_counter("reverse_profile_reuse_fallback")
        else:
            profile = previous_profile(
                self,
                dataset,
                layer_rules,
                road_centerline_paths,
                terrain_boundary_paths,
                fallback_marker_scale,
                reverse_profile_direction,
            )

        if isinstance(context, dict) and is_base_profile:
            context["base_profile"] = profile

        if mode == reverse_patch.NORMAL_MODE and layer_id is not None:
            cache_name = "normal_base_profiles" if is_base_profile else "normal_final_profiles"
            cached_value = (_dataset_fingerprint(dataset, state), profile)
            with pipeline_v4._session(self)["lock"]:
                state[cache_name][layer_id] = cached_value
        return profile

    def _dataset_with_template_dekbanden_v5(
        self,
        layer,
        dataset,
        layer_rules,
        road_centerline_paths,
        terrain_boundary_paths,
        fallback_marker_scale,
        reverse_profile_direction=False,
    ):
        state = _state(self)
        mode = pipeline_v4._mode(self)
        if state is None or mode not in {reverse_patch.NORMAL_MODE, reverse_patch.REVERSE_MODE}:
            return previous_dekband(
                self,
                layer,
                dataset,
                layer_rules,
                road_centerline_paths,
                terrain_boundary_paths,
                fallback_marker_scale,
                reverse_profile_direction,
            )

        previous_context = getattr(self, _CONTEXT_ATTR, None)
        context = {
            "layer_id": id(layer),
            "is_base_profile": True,
            "base_profile": None,
        }
        setattr(self, _CONTEXT_ATTR, context)
        try:
            result = previous_dekband(
                self,
                layer,
                dataset,
                layer_rules,
                road_centerline_paths,
                terrain_boundary_paths,
                fallback_marker_scale,
                reverse_profile_direction,
            )
        finally:
            if previous_context is None:
                try:
                    delattr(self, _CONTEXT_ATTR)
                except Exception:
                    pass
            else:
                setattr(self, _CONTEXT_ATTR, previous_context)

        with pipeline_v4._session(self)["lock"]:
            state["dataset_layers"][id(result)] = id(layer)

        base_profile = context.get("base_profile")
        source_polylines = tuple(getattr(dataset, "polylines", ()) or ())
        result_polylines = tuple(getattr(result, "polylines", ()) or ())
        if (
            base_profile is not None
            and result is not dataset
            and len(result_polylines) > len(source_polylines)
            and result_polylines[: len(source_polylines)] == source_polylines
        ):
            added_polylines = result_polylines[len(source_polylines) :]
            augmented = _augment_profile_with_polylines(
                self,
                base_profile,
                tuple(added_polylines),
                tuple(layer_rules),
            )
            if augmented is not None:
                ready_key = (id(result), bool(reverse_profile_direction))
                with pipeline_v4._session(self)["lock"]:
                    state["ready_profiles"][ready_key] = augmented
                    if mode == reverse_patch.NORMAL_MODE:
                        state["normal_final_profiles"][id(layer)] = (
                            _dataset_fingerprint(result, state),
                            augmented,
                        )
                pipeline._increment_pair_counter("template_profile_dekband_augmented")
        return result

    CadastralDxfExporter._build_template_cross_section_profile = (
        _build_template_cross_section_profile_v5
    )
    CadastralDxfExporter._dataset_with_template_dekbanden = (
        _dataset_with_template_dekbanden_v5
    )
    CadastralDxfExporter._sleufbase_dxf_template_pipeline_v5_version = PATCH_VERSION
    CadastralDxfExporter.SLEUFBASE_REVERSE_PROFILE_REUSE = True
    CadastralDxfExporter.SLEUFBASE_FORCED_PROFILE_FAST_PATH = True
    CadastralDxfExporter.SLEUFBASE_DEKBAND_PROFILE_AUGMENT = True
