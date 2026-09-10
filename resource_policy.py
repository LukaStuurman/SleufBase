from __future__ import annotations

from dataclasses import dataclass
from functools import lru_cache
import os
from pathlib import Path
from typing import Mapping


_MIB = 1024 * 1024
_GIB = 1024 * _MIB
_VALID_MODES = {"auto", "eco", "balanced", "performance"}


@dataclass(frozen=True)
class HardwareSnapshot:
    """Small dependency-free hardware snapshot used by the scheduler policy."""

    logical_cpus: int
    total_memory_bytes: int
    available_memory_bytes: int

    @property
    def total_memory_gib(self) -> float:
        return self.total_memory_bytes / _GIB if self.total_memory_bytes > 0 else 0.0

    @property
    def available_memory_gib(self) -> float:
        return self.available_memory_bytes / _GIB if self.available_memory_bytes > 0 else 0.0


@dataclass(frozen=True)
class ResourcePolicy:
    """Resolved concurrency and cache limits for the current machine.

    The defaults intentionally scale by both CPU and RAM. Network-heavy work can
    use more workers than raster work, while high-resolution TIFF preparation is
    kept memory-aware because each task can temporarily own several full-size
    RGBA/NumPy buffers.
    """

    mode: str
    logical_cpus: int
    total_memory_bytes: int
    available_memory_bytes: int
    network_workers: int
    local_geometry_workers: int
    light_map_workers: int
    template_asset_workers: int
    heavy_raster_workers: int
    virtual_tiff_pixel_budget: int
    tile_memory_cache_entries: int
    wms_cache_bytes: int

    @property
    def description(self) -> str:
        memory_gib = self.total_memory_bytes / _GIB if self.total_memory_bytes else 0.0
        memory_text = f"{memory_gib:.1f} GiB" if memory_gib else "onbekend RAM"
        return (
            f"{self.mode}: {self.logical_cpus} CPU-threads, {memory_text}, "
            f"net={self.network_workers}, maps={self.light_map_workers}, "
            f"raster={self.heavy_raster_workers}"
        )


def _positive_int(value: object, default: int) -> int:
    try:
        parsed = int(str(value).strip())
    except (TypeError, ValueError):
        return default
    return parsed if parsed > 0 else default


def _clamp(value: int, minimum: int, maximum: int) -> int:
    return max(minimum, min(maximum, int(value)))


def _read_linux_mem_available() -> int:
    try:
        text = Path("/proc/meminfo").read_text(encoding="ascii", errors="ignore")
    except OSError:
        return 0
    for line in text.splitlines():
        if line.startswith("MemAvailable:"):
            parts = line.split()
            if len(parts) >= 2:
                try:
                    return int(parts[1]) * 1024
                except ValueError:
                    return 0
    return 0


def _memory_status_windows() -> tuple[int, int]:
    if os.name != "nt":
        return 0, 0
    try:
        import ctypes

        class MEMORYSTATUSEX(ctypes.Structure):
            _fields_ = [
                ("dwLength", ctypes.c_ulong),
                ("dwMemoryLoad", ctypes.c_ulong),
                ("ullTotalPhys", ctypes.c_ulonglong),
                ("ullAvailPhys", ctypes.c_ulonglong),
                ("ullTotalPageFile", ctypes.c_ulonglong),
                ("ullAvailPageFile", ctypes.c_ulonglong),
                ("ullTotalVirtual", ctypes.c_ulonglong),
                ("ullAvailVirtual", ctypes.c_ulonglong),
                ("ullAvailExtendedVirtual", ctypes.c_ulonglong),
            ]

        status = MEMORYSTATUSEX()
        status.dwLength = ctypes.sizeof(status)
        if ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status)):
            return int(status.ullTotalPhys), int(status.ullAvailPhys)
    except Exception:
        pass
    return 0, 0


def _memory_status_posix() -> tuple[int, int]:
    try:
        page_size = int(os.sysconf("SC_PAGE_SIZE"))
        page_count = int(os.sysconf("SC_PHYS_PAGES"))
        total = page_size * page_count
    except (AttributeError, OSError, TypeError, ValueError):
        total = 0

    available = _read_linux_mem_available()
    if available <= 0:
        available = total
    return total, available


def detect_hardware() -> HardwareSnapshot:
    logical_cpus = max(1, int(os.cpu_count() or 1))
    total_memory, available_memory = _memory_status_windows()
    if total_memory <= 0:
        total_memory, available_memory = _memory_status_posix()
    if available_memory <= 0:
        available_memory = total_memory
    return HardwareSnapshot(
        logical_cpus=logical_cpus,
        total_memory_bytes=max(0, int(total_memory)),
        available_memory_bytes=max(0, int(available_memory)),
    )


def _auto_limits(snapshot: HardwareSnapshot) -> dict[str, int]:
    cpu = max(1, snapshot.logical_cpus)
    memory_gib = snapshot.total_memory_gib

    # Unknown RAM gets laptop-safe limits instead of assuming a workstation.
    if memory_gib <= 0:
        memory_gib = 8.0

    if memory_gib < 6:
        memory_tier = 0
    elif memory_gib < 12:
        memory_tier = 1
    elif memory_gib < 24:
        memory_tier = 2
    elif memory_gib < 48:
        memory_tier = 3
    elif memory_gib < 96:
        memory_tier = 4
    else:
        memory_tier = 5

    network_workers = _clamp(cpu * 2, 2, 24)
    local_geometry_workers = _clamp((cpu + 1) // 2, 1, 12)
    light_map_workers = _clamp((cpu + 1) // 2, 1, 12)
    template_asset_workers = _clamp((cpu + 1) // 2, 1, 10)

    memory_worker_caps = (2, 4, 6, 8, 10, 12)
    network_workers = min(network_workers, max(2, memory_worker_caps[memory_tier] * 2))
    local_geometry_workers = min(local_geometry_workers, memory_worker_caps[memory_tier])
    light_map_workers = min(light_map_workers, memory_worker_caps[memory_tier])
    template_asset_workers = min(template_asset_workers, memory_worker_caps[memory_tier])

    if memory_gib < 16 or cpu < 8:
        heavy_raster_workers = 1
    elif memory_gib < 32 or cpu < 16:
        heavy_raster_workers = 2
    elif memory_gib < 64 or cpu < 24:
        heavy_raster_workers = 3
    else:
        heavy_raster_workers = 4

    # 16M source pixels is the historical safe baseline. Larger machines may
    # overlap multiple heavy renders, but the cap grows much slower than RAM.
    pixel_budget_by_tier = (12_000_000, 16_000_000, 20_000_000, 32_000_000, 48_000_000, 64_000_000)
    virtual_tiff_pixel_budget = pixel_budget_by_tier[memory_tier]

    # A raw 256x256 RGBA tile is 256 KiB. Keep aggregate per-client memory
    # modest on laptops and let workstations retain a larger navigation cache.
    tile_cache_by_tier = (96, 160, 256, 512, 768, 1024)
    wms_cache_mib_by_tier = (24, 48, 64, 128, 192, 256)

    return {
        "network_workers": network_workers,
        "local_geometry_workers": local_geometry_workers,
        "light_map_workers": light_map_workers,
        "template_asset_workers": template_asset_workers,
        "heavy_raster_workers": heavy_raster_workers,
        "virtual_tiff_pixel_budget": virtual_tiff_pixel_budget,
        "tile_memory_cache_entries": tile_cache_by_tier[memory_tier],
        "wms_cache_bytes": wms_cache_mib_by_tier[memory_tier] * _MIB,
    }


def _apply_mode(limits: dict[str, int], mode: str) -> dict[str, int]:
    adjusted = dict(limits)
    worker_keys = (
        "network_workers",
        "local_geometry_workers",
        "light_map_workers",
        "template_asset_workers",
        "heavy_raster_workers",
    )

    if mode == "eco":
        for key in worker_keys:
            adjusted[key] = max(1, (adjusted[key] + 1) // 2)
        adjusted["heavy_raster_workers"] = 1
        adjusted["virtual_tiff_pixel_budget"] = min(adjusted["virtual_tiff_pixel_budget"], 16_000_000)
        adjusted["tile_memory_cache_entries"] = min(adjusted["tile_memory_cache_entries"], 160)
        adjusted["wms_cache_bytes"] = min(adjusted["wms_cache_bytes"], 48 * _MIB)
    elif mode == "balanced":
        for key in worker_keys:
            adjusted[key] = max(1, min(adjusted[key], 8))
        adjusted["heavy_raster_workers"] = min(adjusted["heavy_raster_workers"], 2)
        adjusted["virtual_tiff_pixel_budget"] = min(adjusted["virtual_tiff_pixel_budget"], 32_000_000)
        adjusted["tile_memory_cache_entries"] = min(adjusted["tile_memory_cache_entries"], 512)
        adjusted["wms_cache_bytes"] = min(adjusted["wms_cache_bytes"], 128 * _MIB)
    elif mode == "performance":
        adjusted["network_workers"] = min(32, max(adjusted["network_workers"], 8))
        adjusted["local_geometry_workers"] = min(16, max(adjusted["local_geometry_workers"], 4))
        adjusted["light_map_workers"] = min(16, max(adjusted["light_map_workers"], 4))
        adjusted["template_asset_workers"] = min(12, max(adjusted["template_asset_workers"], 4))
        # Keep the memory-derived heavy-raster count; forcing this upward on an
        # 8/16 GiB machine would trade throughput for paging.
    return adjusted


def build_resource_policy(
    snapshot: HardwareSnapshot | None = None,
    env: Mapping[str, str] | None = None,
) -> ResourcePolicy:
    snapshot = snapshot or detect_hardware()
    env = os.environ if env is None else env
    mode = str(env.get("SLEUFBASE_RESOURCE_MODE", "auto") or "auto").strip().casefold()
    if mode not in _VALID_MODES:
        mode = "auto"

    limits = _apply_mode(_auto_limits(snapshot), mode)

    global_cap = _positive_int(env.get("SLEUFBASE_MAX_WORKERS"), 0)
    override_map = {
        "network_workers": "SLEUFBASE_NETWORK_WORKERS",
        "local_geometry_workers": "SLEUFBASE_GEOMETRY_WORKERS",
        "light_map_workers": "SLEUFBASE_MAP_WORKERS",
        "template_asset_workers": "SLEUFBASE_TEMPLATE_WORKERS",
        "heavy_raster_workers": "SLEUFBASE_RASTER_WORKERS",
    }
    for key, env_name in override_map.items():
        explicit = _positive_int(env.get(env_name), 0)
        if explicit > 0:
            limits[key] = explicit
        if global_cap > 0:
            limits[key] = min(limits[key], global_cap)
        limits[key] = max(1, int(limits[key]))

    pixel_override = _positive_int(env.get("SLEUFBASE_TIFF_PIXEL_BUDGET"), 0)
    if pixel_override > 0:
        limits["virtual_tiff_pixel_budget"] = max(4_000_000, pixel_override)

    tile_override = _positive_int(env.get("SLEUFBASE_TILE_CACHE_ENTRIES"), 0)
    if tile_override > 0:
        limits["tile_memory_cache_entries"] = max(32, tile_override)

    cache_mb_override = _positive_int(env.get("SLEUFBASE_WMS_CACHE_MB"), 0)
    if cache_mb_override > 0:
        limits["wms_cache_bytes"] = max(8 * _MIB, cache_mb_override * _MIB)

    return ResourcePolicy(
        mode=mode,
        logical_cpus=max(1, int(snapshot.logical_cpus)),
        total_memory_bytes=max(0, int(snapshot.total_memory_bytes)),
        available_memory_bytes=max(0, int(snapshot.available_memory_bytes)),
        network_workers=int(limits["network_workers"]),
        local_geometry_workers=int(limits["local_geometry_workers"]),
        light_map_workers=int(limits["light_map_workers"]),
        template_asset_workers=int(limits["template_asset_workers"]),
        heavy_raster_workers=int(limits["heavy_raster_workers"]),
        virtual_tiff_pixel_budget=int(limits["virtual_tiff_pixel_budget"]),
        tile_memory_cache_entries=int(limits["tile_memory_cache_entries"]),
        wms_cache_bytes=int(limits["wms_cache_bytes"]),
    )


@lru_cache(maxsize=1)
def get_resource_policy() -> ResourcePolicy:
    """Return the process-wide policy; hardware detection is done only once."""

    return build_resource_policy()


def reset_resource_policy_cache() -> None:
    """Test/support hook for cases where environment overrides changed."""

    get_resource_policy.cache_clear()
