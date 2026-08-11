from __future__ import annotations

import os
import subprocess
import sys
import time
from pathlib import Path
from urllib.parse import urlparse

import requests


DEFAULT_VLLM_BASE_URL = "http://127.0.0.1:8000/v1"
DEFAULT_VLLM_MODEL = "ciocan/gemma-4-E4B-it-W4A16"
DEFAULT_VLLM_PORT = 8000


class VllmRuntimeError(RuntimeError):
    """Raised when SleufBase cannot start or reach the local vLLM runtime."""


def ensure_vllm_server(
    *,
    base_url: str | None = None,
    model: str | None = None,
    timeout_seconds: int = 420,
) -> str:
    root_url = (base_url or os.environ.get("SLEUFBASE_VLLM_BASE_URL") or DEFAULT_VLLM_BASE_URL).rstrip("/")
    model_name = (model or os.environ.get("SLEUFBASE_VLLM_MODEL") or DEFAULT_VLLM_MODEL).strip()
    if _server_ready(root_url):
        return root_url
    if os.environ.get("SLEUFBASE_VLLM_AUTOSTART", "1").strip().lower() in {"0", "false", "no", "nee"}:
        raise VllmRuntimeError(f"vLLM draait niet op {root_url}.")
    if not _is_local_url(root_url):
        raise VllmRuntimeError(f"vLLM draait niet op {root_url}; automatisch starten kan alleen lokaal.")

    root = _workspace_root()
    vllm_exe = root / ".tools" / "vllm" / "Scripts" / "vllm.exe"
    if not vllm_exe.exists():
        raise VllmRuntimeError(f"vLLM is niet geinstalleerd: {vllm_exe}")

    port = _port_from_url(root_url) or DEFAULT_VLLM_PORT
    logs_dir = root / ".tools" / "vllm" / "logs"
    cache_dir = root / ".tools" / "vllm" / "hf_home"
    logs_dir.mkdir(parents=True, exist_ok=True)
    cache_dir.mkdir(parents=True, exist_ok=True)
    log_path = logs_dir / "vllm-server.log"

    model_path = _local_model_path(root, model_name)
    quantization = _quantization_for_model(model_name)
    command = [
        str(vllm_exe),
        "serve",
        str(model_path),
        "--host",
        "127.0.0.1",
        "--port",
        str(port),
        "--served-model-name",
        model_name,
        "--max-model-len",
        os.environ.get("SLEUFBASE_VLLM_MAX_MODEL_LEN", "4096"),
        "--dtype",
        os.environ.get("SLEUFBASE_VLLM_DTYPE", "half"),
        "--gpu-memory-utilization",
        os.environ.get("SLEUFBASE_VLLM_GPU_MEMORY_UTILIZATION", "0.78"),
        "--cpu-offload-gb",
        os.environ.get("SLEUFBASE_VLLM_CPU_OFFLOAD_GB", "4"),
        "--max-num-seqs",
        os.environ.get("SLEUFBASE_VLLM_MAX_NUM_SEQS", "1"),
        "--safetensors-load-strategy",
        os.environ.get("SLEUFBASE_VLLM_SAFETENSORS_LOAD_STRATEGY", "lazy"),
        "--trust-remote-code",
    ]
    if quantization:
        command.extend(["--quantization", quantization])
    if os.environ.get("SLEUFBASE_VLLM_ENFORCE_EAGER", "1").strip().lower() not in {"0", "false", "no", "nee"}:
        command.append("--enforce-eager")
    extra_args = os.environ.get("SLEUFBASE_VLLM_EXTRA_ARGS", "").strip()
    if extra_args:
        command.extend(extra_args.split())

    env = os.environ.copy()
    env.setdefault("HF_HOME", str(cache_dir))
    env.setdefault("HUGGINGFACE_HUB_CACHE", str(cache_dir / "hub"))
    env.setdefault("TRANSFORMERS_CACHE", str(cache_dir / "transformers"))
    env.setdefault("VLLM_WORKER_MULTIPROC_METHOD", "spawn")
    env.setdefault("PYTHONUTF8", "1")
    env.setdefault("PYTHONIOENCODING", "utf-8")
    env.setdefault("PYTORCH_CUDA_ALLOC_CONF", "expandable_segments:True")
    site_packages = root / ".tools" / "vllm" / "Lib" / "site-packages"
    cuda_root = site_packages / "nvidia" / "cu13"
    _ensure_flashinfer_cuda_dll(site_packages, cuda_root)
    if cuda_root.exists():
        env.setdefault("CUDA_LIB_PATH", str(cuda_root))
    path_parts = [
        str(vllm_exe.parent),
        str(cuda_root / "bin"),
        str(cuda_root / "bin" / "x86_64"),
        str(site_packages / "torch" / "lib"),
    ]
    env["PATH"] = os.pathsep.join(path_parts + [env.get("PATH", "")])

    with log_path.open("ab") as log_file:
        log_file.write(f"\n\n=== SleufBase vLLM start {time.strftime('%Y-%m-%d %H:%M:%S')} ===\n".encode("utf-8"))
        log_file.write((" ".join(command) + "\n").encode("utf-8", errors="replace"))
        flags = 0
        if hasattr(subprocess, "CREATE_NO_WINDOW"):
            flags |= subprocess.CREATE_NO_WINDOW
        subprocess.Popen(
            command,
            cwd=str(root),
            env=env,
            stdout=log_file,
            stderr=subprocess.STDOUT,
            stdin=subprocess.DEVNULL,
            creationflags=flags,
        )

    deadline = time.monotonic() + max(1, int(timeout_seconds))
    while time.monotonic() < deadline:
        if _server_ready(root_url):
            return root_url
        time.sleep(2.0)
    raise VllmRuntimeError(f"vLLM is gestart maar werd niet bereikbaar op {root_url}. Log: {log_path}")


def _server_ready(root_url: str) -> bool:
    try:
        response = requests.get(f"{root_url.rstrip('/')}/models", timeout=2)
        return 200 <= response.status_code < 300
    except requests.RequestException:
        return False


def _is_local_url(root_url: str) -> bool:
    parsed = urlparse(root_url)
    host = (parsed.hostname or "").lower()
    return host in {"127.0.0.1", "localhost", "::1"}


def _port_from_url(root_url: str) -> int | None:
    try:
        return urlparse(root_url).port
    except ValueError:
        return None


def _workspace_root() -> Path:
    if getattr(sys, "frozen", False):
        executable_root = Path(sys.executable).resolve().parent
        if (executable_root / ".tools").exists():
            return executable_root
        return executable_root.parent
    return Path(__file__).resolve().parent.parent


def _local_model_path(root: Path, model_name: str) -> Path | str:
    slug = model_name.replace("/", "-").replace("\\", "-")
    candidate = root / ".tools" / "vllm" / "models" / slug
    if candidate.exists():
        return candidate
    return model_name


def _quantization_for_model(model_name: str) -> str | None:
    override = os.environ.get("SLEUFBASE_VLLM_QUANTIZATION", "").strip()
    if override:
        return override
    normalized = model_name.lower()
    if "w4a16" in normalized or "gptq" in normalized:
        return "gptq"
    if "awq" in normalized:
        return "awq"
    return None


def _ensure_flashinfer_cuda_dll(site_packages: Path, cuda_root: Path) -> None:
    target = site_packages / "bin" / "cudart64_13.dll"
    if target.exists():
        return
    candidates = [
        cuda_root / "bin" / "x86_64" / "cudart64_13.dll",
        site_packages / "torch" / "lib" / "cudart64_13.dll",
    ]
    for source in candidates:
        if not source.exists():
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(source.read_bytes())
        return
