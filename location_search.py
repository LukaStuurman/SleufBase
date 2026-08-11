from __future__ import annotations

import re
from dataclasses import dataclass

import requests


POINT_PATTERN = re.compile(r"POINT\(([-0-9.]+)\s+([-0-9.]+)\)")
@dataclass(frozen=True)
class LocationResult:
    label: str
    x: float
    y: float
    location_type: str


@dataclass(frozen=True)
class ReverseLocationResult:
    label: str
    distance_m: float
    location_type: str


class PdokLocationClient:
    SEARCH_URL = "https://api.pdok.nl/bzk/locatieserver/search/v3_1/free"
    REVERSE_URL = "https://api.pdok.nl/bzk/locatieserver/search/v3_1/reverse"

    def __init__(self, timeout: int = 20) -> None:
        self.timeout = timeout
        self.session = requests.Session()
        self.session.headers.update({"User-Agent": "SleufBase/1.4"})

    def search(self, query: str, rows: int = 5) -> list[LocationResult]:
        params = {
            "q": query,
            "rows": rows,
            "fq": "type:(adres OR weg OR woonplaats)",
            "fl": "weergavenaam,centroide_rd,type",
        }
        response = self.session.get(self.SEARCH_URL, params=params, timeout=self.timeout)
        response.raise_for_status()
        docs = response.json().get("response", {}).get("docs", [])
        results: list[LocationResult] = []
        for doc in docs:
            point = self._parse_point(doc.get("centroide_rd", ""))
            if point is None:
                continue
            results.append(
                LocationResult(
                    label=doc.get("weergavenaam", query),
                    x=point[0],
                    y=point[1],
                    location_type=doc.get("type", ""),
                )
            )
        return results

    def reverse_lookup(self, x: float, y: float, rows: int = 1) -> list[ReverseLocationResult]:
        params = {
            "X": f"{x:.3f}",
            "Y": f"{y:.3f}",
            "rows": rows,
            "fl": "weergavenaam,type,afstand",
        }
        response = self.session.get(self.REVERSE_URL, params=params, timeout=self.timeout)
        response.raise_for_status()
        docs = response.json().get("response", {}).get("docs", [])
        results: list[ReverseLocationResult] = []
        for doc in docs:
            try:
                distance_m = float(doc.get("afstand", 0.0))
            except (TypeError, ValueError):
                distance_m = 0.0
            results.append(
                ReverseLocationResult(
                    label=doc.get("weergavenaam", ""),
                    distance_m=distance_m,
                    location_type=doc.get("type", ""),
                )
            )
        return results

    @staticmethod
    def parse_rd_input(query: str) -> tuple[float, float] | None:
        matches = re.findall(r"-?\d+(?:[.,]\d+)?", query)
        if len(matches) != 2:
            return None
        values = [float(match.replace(",", ".")) for match in matches]
        if values[0] < 1000 or values[1] < 1000:
            return None
        return values[0], values[1]

    @staticmethod
    def _parse_point(value: str) -> tuple[float, float] | None:
        match = POINT_PATTERN.match(value)
        if match is None:
            return None
        return float(match.group(1)), float(match.group(2))
