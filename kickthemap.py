from __future__ import annotations

import base64
import hashlib
import json
import os
import re
import shutil
import threading
import time
from collections.abc import Callable, Iterable
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from pathlib import Path

import boto3
from Crypto.Cipher import AES
import requests


LOGIN_TOKEN_PATTERN = re.compile(r'name="_token"\s+value="([^"]+)"', re.IGNORECASE)
CSRF_META_PATTERN = re.compile(r'<meta\s+name="csrf-token"\s+content="([^"]+)"', re.IGNORECASE)
PREFIX_PATTERN = re.compile(r"^(?P<email>.+)_(?P<date>\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2})$")


class KickTheMapError(RuntimeError):
    pass


def _safe_email_slug(email: str) -> str:
    sanitized = re.sub(r"[^A-Za-z0-9._-]+", "_", str(email or "").strip().lower())
    return sanitized.strip("._-") or "default"


def kickthemap_browser_sessions_dir() -> Path:
    return KickTheMapClient.default_download_dir() / "browser_sessions"


def kickthemap_browser_session_dir(email: str) -> Path:
    return kickthemap_browser_sessions_dir() / _safe_email_slug(email)


def clear_kickthemap_browser_session(email: str = "") -> list[Path]:
    normalized_email = str(email or "").strip().lower()
    target = kickthemap_browser_session_dir(normalized_email) if normalized_email else kickthemap_browser_sessions_dir()
    if not target.exists():
        return []
    shutil.rmtree(target)
    return [target]


@dataclass(frozen=True)
class KickTheMapJob:
    job_id: int
    title: str
    prefix: str
    project_mail: str
    project_date: str
    address: str
    client_date: str
    delivery_date: str
    status: int
    download_available: bool
    archived: int
    user_id: str = ""
    municipality: str = ""
    coordinates: str = ""

    @property
    def safe_file_stem(self) -> str:
        cleaned = re.sub(r"[^A-Za-z0-9._-]+", "_", self.title.strip())
        cleaned = cleaned.strip("._-")
        if not cleaned:
            cleaned = f"kickthemap_job_{self.job_id}"
        return cleaned


@dataclass(frozen=True)
class KickTheMapAwsConfig:
    access_key_id: str
    secret_access_key: str
    bucket: str
    region: str


class KickTheMapClient:
    BASE_URL = "https://www.my.kickthemap.com"
    SIGNIN_URL = BASE_URL + "/signin"
    LOGIN_URL = BASE_URL + "/login"
    JOBS_URL = BASE_URL + "/"
    AWS_DATA_URL = BASE_URL + "/main/adata"
    FILE_URL_URL = BASE_URL + "/jobs/get-file-url"
    COORD_FILE_URL = BASE_URL + "/jobs/getCoordFile"

    def __init__(self, timeout: int = 30) -> None:
        self.timeout = timeout
        self.session = requests.Session()
        self.session.headers.update({"User-Agent": "SleufBase/1.5"})
        self.logged_in_email: str | None = None
        self._csrf_token: str | None = None
        self._aws_config: KickTheMapAwsConfig | None = None
        self._recent_job_feature_paths: dict[int, tuple[Path, float]] = {}
        self._recent_job_feature_lock = threading.Lock()
        # A template export is commonly started immediately after loading all
        # cross-section start points. Reuse only files downloaded successfully
        # by this client and only for this short window.
        self.job_features_reuse_seconds = 120.0

    @property
    def is_logged_in(self) -> bool:
        return self.logged_in_email is not None

    @staticmethod
    def default_download_dir() -> Path:
        local_app_data = Path(os.environ.get("LOCALAPPDATA", str(Path.home())))
        return local_app_data / "SleufBase" / "KickTheMap"

    def login(self, email: str, password: str) -> list[KickTheMapJob]:
        with self._recent_job_feature_lock:
            self._recent_job_feature_paths.clear()
        signin_page = self.session.get(self.SIGNIN_URL, timeout=self.timeout)
        signin_page.raise_for_status()
        login_token = self._extract_login_token(signin_page.text)
        self.session.post(
            self.LOGIN_URL,
            data={
                "_token": login_token,
                "email": email,
                "password": password,
                "g-recaptcha-response": "",
            },
            timeout=self.timeout,
        ).raise_for_status()

        jobs_page = self.session.get(self.JOBS_URL, timeout=self.timeout)
        jobs_page.raise_for_status()
        if self._looks_like_login_page(jobs_page.text):
            raise KickTheMapError("Inloggen bij KickTheMap is mislukt.")

        self.logged_in_email = email
        self._csrf_token = self._extract_csrf_token(jobs_page.text)
        self._aws_config = None
        return self._parse_jobs_page(jobs_page.text)

    def logout(self) -> None:
        self.session.cookies.clear()
        self.logged_in_email = None
        self._csrf_token = None
        self._aws_config = None
        with self._recent_job_feature_lock:
            self._recent_job_feature_paths.clear()

    def fetch_jobs(self) -> list[KickTheMapJob]:
        self._ensure_logged_in()
        jobs_page = self.session.get(self.JOBS_URL, timeout=self.timeout)
        jobs_page.raise_for_status()
        self._csrf_token = self._extract_csrf_token(jobs_page.text)
        return self._parse_jobs_page(jobs_page.text)

    def download_tiff(self, job: KickTheMapJob, target_dir: Path | None = None) -> Path:
        self._ensure_logged_in()

        target_root = target_dir or self.default_download_dir()
        target_root.mkdir(parents=True, exist_ok=True)
        target_path = target_root / f"{job.safe_file_stem}_{job.job_id}.tiff"

        file_name = f"{job.safe_file_stem}.tiff"
        try:
            self._download_project_file(
                job,
                folder="cloud",
                remote_file_name=f"{self._job_storage_prefix(job)}.tiff",
                target_path=target_path,
                export_name=file_name,
                legacy_file_key=self._job_tiff_key(job),
            )
        except Exception as exc:
            raise KickTheMapError(f"GeoTIFF kon niet worden gedownload voor job '{job.title}' ({exc}).") from exc
        return target_path

    def download_tiffs(
        self,
        jobs: Iterable[KickTheMapJob],
        target_dir: Path | None = None,
        *,
        max_workers: int = 6,
        progress_callback: Callable[[int, int], None] | None = None,
    ) -> tuple[dict[int, Path], dict[int, Exception]]:
        self._ensure_logged_in()
        job_list = list(jobs)
        if not job_list:
            return {}, {}

        target_root = target_dir or self.default_download_dir()
        target_root.mkdir(parents=True, exist_ok=True)

        specs: list[tuple[KickTheMapJob, Path, str]] = []
        paths: dict[int, Path] = {}
        errors: dict[int, Exception] = {}
        completed = 0
        for job in job_list:
            target_path = target_root / f"{job.safe_file_stem}_{job.job_id}.tiff"
            file_name = f"{job.safe_file_stem}.tiff"
            try:
                signed_url = self._request_project_file_url(
                    job,
                    folder="cloud",
                    remote_file_name=f"{self._job_storage_prefix(job)}.tiff",
                    export_name=file_name,
                )
                specs.append((job, target_path, signed_url))
            except Exception as file_url_exc:
                try:
                    self._download_s3_object(self._job_tiff_key(job), target_path, file_name)
                    paths[job.job_id] = target_path
                except Exception as legacy_exc:
                    errors[job.job_id] = KickTheMapError(
                        f"GeoTIFF kon niet worden gedownload voor job '{job.title}' ({file_url_exc})."
                    )
                completed += 1
                if progress_callback is not None:
                    progress_callback(completed, len(job_list))

        def download_one(spec: tuple[KickTheMapJob, Path, str]) -> tuple[int, Path]:
            job, target_path, signed_url = spec
            self._download_url_to_file(signed_url, target_path)
            return job.job_id, target_path

        workers = max(1, min(max_workers, len(specs)))
        with ThreadPoolExecutor(max_workers=workers, thread_name_prefix="kickthemap-tiffs") as executor:
            future_map = {executor.submit(download_one, spec): spec[0] for spec in specs}
            for future in as_completed(future_map):
                job = future_map[future]
                try:
                    job_id, target_path = future.result()
                    paths[job_id] = target_path
                except Exception as exc:
                    errors[job.job_id] = KickTheMapError(
                        f"GeoTIFF kon niet worden gedownload voor job '{job.title}' ({exc})."
                    )
                completed += 1
                if progress_callback is not None:
                    progress_callback(completed, len(job_list))
        return paths, errors

    def download_coord_file(self, job: KickTheMapJob, target_dir: Path | None = None) -> Path:
        self._ensure_logged_in()
        if not self._request_coord_file(job):
            raise KickTheMapError(f"Geen coordinate-bestand beschikbaar voor job '{job.title}'.")

        target_root = target_dir or self.default_download_dir()
        target_root.mkdir(parents=True, exist_ok=True)
        target_path = target_root / f"{job.safe_file_stem}_{job.job_id}_coords.txt"

        file_name = f"{job.safe_file_stem}_coords.txt"
        try:
            self._download_project_file(
                job,
                folder="pictures",
                remote_file_name=f"{self._job_storage_prefix(job)}_coord.txt",
                target_path=target_path,
                export_name=file_name,
                legacy_file_key=self._job_coord_key(job),
            )
        except Exception as exc:
            raise KickTheMapError(f"Coordinate-bestand kon niet worden gedownload voor job '{job.title}' ({exc}).") from exc
        return target_path

    def download_job_features_file(self, job: KickTheMapJob, target_dir: Path | None = None) -> Path:
        self._ensure_logged_in()

        target_root = target_dir or self.default_download_dir()
        target_root.mkdir(parents=True, exist_ok=True)
        target_path = target_root / f"{job.safe_file_stem}_{job.job_id}_jobFeatures.json"

        file_name = f"{job.safe_file_stem}_jobFeatures.json"
        try:
            self._download_project_file(
                job,
                folder="cloud",
                remote_file_name="jobFeatures.json",
                target_path=target_path,
                export_name=file_name,
                legacy_file_key=self._job_features_key(job),
            )
        except Exception as exc:
            raise KickTheMapError(
                f"Geen opgeslagen objectpunten gevonden voor job '{job.title}'."
            ) from exc
        self._remember_job_features_path(job.job_id, target_path)
        return target_path

    def download_job_features_files(
        self,
        jobs: Iterable[KickTheMapJob],
        target_dir: Path | None = None,
        *,
        max_workers: int = 6,
        progress_callback: Callable[[int, int], None] | None = None,
    ) -> tuple[dict[int, Path], dict[int, Exception]]:
        self._ensure_logged_in()
        job_list = list(jobs)
        if not job_list:
            return {}, {}

        target_root = target_dir or self.default_download_dir()
        target_root.mkdir(parents=True, exist_ok=True)

        specs: list[tuple[KickTheMapJob, Path, str]] = []
        paths: dict[int, Path] = {}
        errors: dict[int, Exception] = {}
        completed = 0
        for job in job_list:
            target_path = target_root / f"{job.safe_file_stem}_{job.job_id}_jobFeatures.json"
            file_name = f"{job.safe_file_stem}_jobFeatures.json"
            if self._recent_job_features_path(job.job_id, target_path) is not None:
                paths[job.job_id] = target_path
                completed += 1
                if progress_callback is not None:
                    progress_callback(completed, len(job_list))
                continue
            specs.append((job, target_path, file_name))

        def download_one(spec: tuple[KickTheMapJob, Path, str]) -> tuple[int, Path]:
            job, target_path, file_name = spec
            worker_client = self._parallel_worker_client()
            signed_url = worker_client._request_project_file_url(
                job,
                folder="cloud",
                remote_file_name="jobFeatures.json",
                export_name=file_name,
            )
            worker_client._download_url_to_file(signed_url, target_path)
            return job.job_id, target_path

        workers = max(1, min(max_workers, len(specs)))
        with ThreadPoolExecutor(max_workers=workers, thread_name_prefix="kickthemap-features") as executor:
            future_map = {executor.submit(download_one, spec): spec[0] for spec in specs}
            for future in as_completed(future_map):
                job = future_map[future]
                try:
                    job_id, target_path = future.result()
                    paths[job_id] = target_path
                    self._remember_job_features_path(job_id, target_path)
                except Exception as exc:
                    errors[job.job_id] = exc
                completed += 1
                if progress_callback is not None:
                    progress_callback(completed, len(job_list))
        return paths, errors

    def _parallel_worker_client(self) -> "KickTheMapClient":
        """Create an authenticated client with an independent requests session."""
        worker = type(self)(timeout=self.timeout)
        worker.logged_in_email = self.logged_in_email
        worker._csrf_token = self._csrf_token
        worker._aws_config = self._aws_config
        worker.session.headers.update(dict(self.session.headers))
        worker.session.cookies.update(self.session.cookies)
        worker.job_features_reuse_seconds = 0.0
        return worker

    def _remember_job_features_path(self, job_id: int, target_path: Path) -> None:
        with self._recent_job_feature_lock:
            self._recent_job_feature_paths[int(job_id)] = (target_path.resolve(), time.monotonic())

    def _recent_job_features_path(self, job_id: int, target_path: Path) -> Path | None:
        reuse_seconds = max(0.0, float(getattr(self, "job_features_reuse_seconds", 0.0) or 0.0))
        if reuse_seconds <= 0.0:
            return None
        with self._recent_job_feature_lock:
            cached = self._recent_job_feature_paths.get(int(job_id))
        if cached is None:
            return None
        cached_path, downloaded_at = cached
        if time.monotonic() - downloaded_at > reuse_seconds:
            return None
        try:
            expected_path = target_path.resolve()
        except OSError:
            expected_path = target_path
        if cached_path != expected_path or not target_path.is_file():
            return None
        return target_path

    def _download_project_file(
        self,
        job: KickTheMapJob,
        *,
        folder: str,
        remote_file_name: str,
        target_path: Path,
        export_name: str,
        legacy_file_key: str,
    ) -> None:
        try:
            signed_url = self._request_project_file_url(
                job,
                folder=folder,
                remote_file_name=remote_file_name,
                export_name=export_name,
            )
            self._download_url_to_file(signed_url, target_path)
            return
        except Exception as file_url_exc:
            try:
                self._download_s3_object(legacy_file_key, target_path, export_name)
                return
            except Exception as legacy_exc:
                raise KickTheMapError(str(file_url_exc)) from legacy_exc

    def _request_project_file_url(
        self,
        job: KickTheMapJob,
        *,
        folder: str,
        remote_file_name: str,
        export_name: str,
    ) -> str:
        csrf_token = self._ensure_csrf_token()
        response = self.session.post(
            self.FILE_URL_URL,
            data={
                "projectId": str(job.job_id),
                "folder": str(folder),
                "fileName": str(remote_file_name),
                "exportName": str(export_name),
            },
            headers={"X-CSRF-TOKEN": csrf_token},
            timeout=self.timeout,
        )
        if response.status_code in {401, 403, 419}:
            self._csrf_token = None
            csrf_token = self._ensure_csrf_token()
            response = self.session.post(
                self.FILE_URL_URL,
                data={
                    "projectId": str(job.job_id),
                    "folder": str(folder),
                    "fileName": str(remote_file_name),
                    "exportName": str(export_name),
                },
                headers={"X-CSRF-TOKEN": csrf_token},
                timeout=self.timeout,
            )
        response.raise_for_status()
        try:
            payload = response.json()
        except ValueError as exc:
            raise KickTheMapError("KickTheMap gaf geen geldige downloadlink terug.") from exc
        if not payload.get("status", False):
            message = str(payload.get("message") or payload.get("data") or "").strip()
            detail = f": {message}" if message else ""
            raise KickTheMapError(f"KickTheMap gaf geen downloadlink terug{detail}.")
        signed_url = payload.get("data")
        if not isinstance(signed_url, str) or not signed_url.strip():
            raise KickTheMapError("KickTheMap gaf een lege downloadlink terug.")
        return signed_url.strip()

    def _download_url_to_file(self, signed_url: str, target_path: Path) -> None:
        temp_path = target_path.with_name(f"{target_path.name}.tmp")
        session = requests.Session()
        session.headers.update({"User-Agent": "SleufBase/1.5"})
        try:
            with session.get(signed_url, stream=True, timeout=self.timeout) as response:
                response.raise_for_status()
                with temp_path.open("wb") as handle:
                    for chunk in response.iter_content(chunk_size=1024 * 1024):
                        if chunk:
                            handle.write(chunk)
            temp_path.replace(target_path)
        except Exception:
            try:
                temp_path.unlink(missing_ok=True)
            except OSError:
                pass
            raise

    def _load_aws_config(self) -> KickTheMapAwsConfig:
        if self._aws_config is not None:
            return self._aws_config

        self._ensure_logged_in()
        csrf_token = self._ensure_csrf_token()
        response = self.session.post(
            self.AWS_DATA_URL,
            headers={"X-CSRF-TOKEN": csrf_token},
            timeout=self.timeout,
        )
        response.raise_for_status()
        payload = response.json()
        decrypted = self._decrypt_aws_payload(payload["adata"], payload["thg"])
        self._aws_config = KickTheMapAwsConfig(
            access_key_id=decrypted["aws_access_key"],
            secret_access_key=decrypted["aws_secret_access_key"],
            bucket=decrypted["bucket"],
            region=decrypted["region"],
        )
        return self._aws_config

    def _download_s3_object(self, file_key: str, target_path: Path, file_name: str) -> None:
        aws_config = self._load_aws_config()
        s3_client = boto3.client(
            "s3",
            region_name=aws_config.region,
            aws_access_key_id=aws_config.access_key_id,
            aws_secret_access_key=aws_config.secret_access_key,
        )
        signed_url = s3_client.generate_presigned_url(
            "get_object",
            Params={
                "Bucket": aws_config.bucket,
                "Key": file_key,
                "ResponseContentDisposition": f'attachment; filename="{file_name}"',
            },
            ExpiresIn=60 * 60 * 24,
        )

        with self.session.get(signed_url, stream=True, timeout=self.timeout) as response:
            response.raise_for_status()
            with target_path.open("wb") as handle:
                for chunk in response.iter_content(chunk_size=1024 * 1024):
                    if chunk:
                        handle.write(chunk)

    def _request_coord_file(self, job: KickTheMapJob) -> bool:
        csrf_token = self._ensure_csrf_token()
        response = self.session.post(
            self.COORD_FILE_URL,
            data={
                "key": f"_{job.project_date}",
                "prefix": job.project_mail,
                "job_name": job.title,
            },
            headers={"X-CSRF-TOKEN": csrf_token},
            timeout=self.timeout,
        )
        response.raise_for_status()
        payload = response.json()
        return bool(payload.get("data"))

    def _ensure_logged_in(self) -> None:
        if not self.is_logged_in:
            raise KickTheMapError("Log eerst in bij KickTheMap.")

    def _ensure_csrf_token(self) -> str:
        if self._csrf_token:
            return self._csrf_token
        jobs_page = self.session.get(self.JOBS_URL, timeout=self.timeout)
        jobs_page.raise_for_status()
        self._csrf_token = self._extract_csrf_token(jobs_page.text)
        return self._csrf_token

    def _parse_jobs_page(self, html: str) -> list[KickTheMapJob]:
        projects_json = self._extract_js_value(html, "var projects")
        raw_projects = json.loads(projects_json)
        jobs: list[KickTheMapJob] = []
        for project in raw_projects:
            prefix = str(project.get("Prefix", "")).strip()
            parsed_prefix = self._parse_prefix(prefix)
            if parsed_prefix is None:
                continue
            download_available = self._parse_bool(project.get("Download"))
            jobs.append(
                KickTheMapJob(
                    job_id=int(project.get("Id", 0)),
                    title=str(project.get("AboutProject", "")).strip() or f"Job {project.get('Id', '')}",
                    prefix=prefix,
                    project_mail=parsed_prefix[0],
                    project_date=parsed_prefix[1],
                    user_id=self._format_user_id(project),
                    address=self._format_address(project),
                    client_date=str(project.get("ClientDate", "")),
                    delivery_date=str(project.get("delivery_date", "")),
                    status=int(project.get("status", 0) or 0),
                    download_available=download_available,
                    archived=int(project.get("archived", 0) or 0),
                    municipality=self._format_municipality(project),
                    coordinates=self._format_coordinates(project),
                )
            )
        return jobs

    @staticmethod
    def _job_tiff_key(job: KickTheMapJob) -> str:
        prefix = KickTheMapClient._job_storage_prefix(job)
        return f"{job.project_mail}/{prefix}/cloud/{prefix}.tiff"

    @staticmethod
    def _job_coord_key(job: KickTheMapJob) -> str:
        prefix = KickTheMapClient._job_storage_prefix(job)
        return f"{job.project_mail}/{prefix}/pictures/{prefix}_coord.txt"

    @staticmethod
    def _job_features_key(job: KickTheMapJob) -> str:
        prefix = KickTheMapClient._job_storage_prefix(job)
        return f"{job.project_mail}/{prefix}/cloud/jobFeatures.json"

    @staticmethod
    def _job_storage_prefix(job: KickTheMapJob) -> str:
        return f"{job.project_mail}_{job.project_date}"

    @staticmethod
    def _extract_login_token(html: str) -> str:
        match = LOGIN_TOKEN_PATTERN.search(html)
        if match is None:
            raise KickTheMapError("KickTheMap login-token niet gevonden.")
        return match.group(1)

    @staticmethod
    def _extract_csrf_token(html: str) -> str:
        match = CSRF_META_PATTERN.search(html)
        if match is None:
            raise KickTheMapError("KickTheMap CSRF-token niet gevonden.")
        return match.group(1)

    @staticmethod
    def _looks_like_login_page(html: str) -> bool:
        return 'id="signin-form"' in html

    @staticmethod
    def _parse_prefix(prefix: str) -> tuple[str, str] | None:
        match = PREFIX_PATTERN.match(prefix)
        if match is None:
            return None
        return match.group("email"), match.group("date")

    @staticmethod
    def _format_address(project: dict) -> str:
        street = str(project.get("address_road", "")).strip()
        number = str(project.get("address_number", "")).strip()
        postcode = str(project.get("address_postcode", "")).strip()
        town = str(project.get("address_town", "")).strip()
        district = str(project.get("address_district", "")).strip()
        region = str(project.get("address_region", "")).strip()
        country = str(project.get("address_country", "")).strip()

        first_line = " ".join(part for part in (street, number) if part).strip()
        second_line = " ".join(part for part in (postcode, town) if part).strip()
        parts = [part for part in (first_line, second_line, district, region, country) if part]
        return ", ".join(parts) if parts else "-"

    @staticmethod
    def _format_user_id(project: dict) -> str:
        for key in ("user_id", "userId", "UserId", "userID", "UserID"):
            value = str(project.get(key, "")).strip()
            if value:
                return value
        return ""

    @staticmethod
    def _format_municipality(project: dict) -> str:
        for key in (
            "address_town",
            "address_city",
            "municipality",
            "gemeente",
            "address_municipality",
            "address_district",
        ):
            value = str(project.get(key, "")).strip()
            if value:
                return value
        return "-"

    @staticmethod
    def _format_coordinates(project: dict) -> str:
        coordinate_pairs = (
            ("x", "y"),
            ("X", "Y"),
            ("rd_x", "rd_y"),
            ("RD_X", "RD_Y"),
            ("longitude", "latitude"),
            ("lng", "lat"),
            ("Lon", "Lat"),
        )
        for first_key, second_key in coordinate_pairs:
            first = str(project.get(first_key, "")).strip()
            second = str(project.get(second_key, "")).strip()
            if first and second:
                return f"{first}, {second}"
        for key in ("coordinates", "coord", "location", "gps", "latlng"):
            value = str(project.get(key, "")).strip()
            if value:
                return value
        return "-"

    @staticmethod
    def _parse_bool(value: object) -> bool:
        if isinstance(value, bool):
            return value
        if isinstance(value, str):
            return value.strip().lower() == "true"
        return bool(value)

    @staticmethod
    def _extract_js_value(html: str, marker: str) -> str:
        marker_index = html.find(marker)
        if marker_index < 0:
            raise KickTheMapError("KickTheMap projectlijst niet gevonden.")
        value_start = html.find("[", marker_index)
        if value_start < 0:
            raise KickTheMapError("KickTheMap projectlijst heeft geen geldig formaat.")

        depth = 0
        in_string = False
        escape = False
        for index in range(value_start, len(html)):
            char = html[index]
            if in_string:
                if escape:
                    escape = False
                elif char == "\\":
                    escape = True
                elif char == '"':
                    in_string = False
                continue

            if char == '"':
                in_string = True
            elif char == "[":
                depth += 1
            elif char == "]":
                depth -= 1
                if depth == 0:
                    return html[value_start : index + 1]

        raise KickTheMapError("KickTheMap projectlijst kon niet worden uitgelezen.")

    @staticmethod
    def _decrypt_aws_payload(encrypted_payload: str, password: str) -> dict:
        envelope = json.loads(encrypted_payload)
        salt = bytes.fromhex(envelope["s"])
        ciphertext = base64.b64decode(envelope["ct"])
        key, iv = KickTheMapClient._evp_bytes_to_key(password.encode("utf-8"), salt, 32, 16)
        cipher = AES.new(key, AES.MODE_CBC, iv)
        plaintext = cipher.decrypt(ciphertext)
        plaintext = KickTheMapClient._pkcs7_unpad(plaintext)
        return json.loads(plaintext.decode("utf-8"))

    @staticmethod
    def _evp_bytes_to_key(password: bytes, salt: bytes, key_size: int, iv_size: int) -> tuple[bytes, bytes]:
        derived = b""
        block = b""
        while len(derived) < key_size + iv_size:
            block = hashlib.md5(block + password + salt).digest()
            derived += block
        return derived[:key_size], derived[key_size : key_size + iv_size]

    @staticmethod
    def _pkcs7_unpad(value: bytes) -> bytes:
        padding_length = value[-1]
        if padding_length < 1 or padding_length > 16:
            raise KickTheMapError("KickTheMap decryptie gaf ongeldige padding terug.")
        return value[:-padding_length]
