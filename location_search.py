from __future__ import annotations

import re
import threading
import time
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

    def __init__(self, timeout: int = 20, retries: int = 3) -> None:
        self.timeout = max(1, int(timeout))
        self.retries = max(1, int(retries))
        self._thread_local = threading.local()

    @staticmethod
    def _create_session() -> requests.Session:
        session = requests.Session()
        session.headers.update({"User-Agent": "SleufBase/1.4"})
        return session

    def _get_session(self) -> requests.Session:
        session = getattr(self._thread_local, "session", None)
        if session is None:
            session = self._create_session()
            self._thread_local.session = session
        return session

    def _request_json(self, url: str, params: dict[str, object]) -> dict:
        last_error: Exception | None = None
        for attempt in range(1, self.retries + 1):
            try:
                response = self._get_session().get(url, params=params, timeout=self.timeout)
                response.raise_for_status()
                payload = response.json()
                if not isinstance(payload, dict):
                    raise ValueError("PDOK locatieserver gaf geen JSON-object terug")
                return payload
            except (requests.RequestException, ValueError) as exc:
                last_error = exc
                if attempt < self.retries:
                    time.sleep(min(1.5, 0.2 * (2 ** (attempt - 1))))
        if last_error is not None:
            raise last_error
        raise RuntimeError("PDOK locatieserver gaf geen antwoord")

    def search(self, query: str, rows: int = 5) -> list[LocationResult]:
        normalized_query = str(query or "").strip()
        if not normalized_query:
            return []
        params = {
            "q": normalized_query,
            "rows": max(1, min(50, int(rows))),
            "fq": "type:(adres OR weg OR woonplaats)",
            "fl": "weergavenaam,centroide_rd,type",
        }
        docs = self._request_json(self.SEARCH_URL, params).get("response", {}).get("docs", [])
        results: list[LocationResult] = []
        for doc in docs:
            if not isinstance(doc, dict):
                continue
            point = self._parse_point(str(doc.get("centroide_rd", "")))
            if point is None:
                continue
            results.append(
                LocationResult(
                    label=str(doc.get("weergavenaam", normalized_query)),
                    x=point[0],
                    y=point[1],
                    location_type=str(doc.get("type", "")),
                )
            )
        return results

    def reverse_lookup(self, x: float, y: float, rows: int = 1) -> list[ReverseLocationResult]:
        params = {
            "X": f"{float(x):.3f}",
            "Y": f"{float(y):.3f}",
            "rows": max(1, min(50, int(rows))),
            "fl": "weergavenaam,type,afstand",
        }
        docs = self._request_json(self.REVERSE_URL, params).get("response", {}).get("docs", [])
        results: list[ReverseLocationResult] = []
        for doc in docs:
            if not isinstance(doc, dict):
                continue
            try:
                distance_m = float(doc.get("afstand", 0.0))
            except (TypeError, ValueError):
                distance_m = 0.0
            results.append(
                ReverseLocationResult(
                    label=str(doc.get("weergavenaam", "")),
                    distance_m=distance_m,
                    location_type=str(doc.get("type", "")),
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
