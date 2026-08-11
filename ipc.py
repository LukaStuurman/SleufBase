from __future__ import annotations

import json
import socket
import threading
from collections.abc import Callable


HOST = "127.0.0.1"
PORT = 51437
TOKEN = "klic-tiff-kaarten-v1"


def send_paths_to_running_instance(paths: list[str], timeout: float = 1.5) -> bool:
    if not paths:
        return False
    payload = json.dumps({"token": TOKEN, "paths": paths}).encode("utf-8")
    try:
        with socket.create_connection((HOST, PORT), timeout=timeout) as connection:
            connection.sendall(payload)
            connection.shutdown(socket.SHUT_WR)
            try:
                connection.settimeout(timeout)
                response = connection.recv(16)
            except OSError:
                response = b""
        return response.strip() == b"OK"
    except OSError:
        return False


class InstanceMessageServer:
    def __init__(self, on_paths: Callable[[list[str]], None]) -> None:
        self.on_paths = on_paths
        self._socket: socket.socket | None = None
        self._running = False
        self._thread: threading.Thread | None = None

    def start(self) -> bool:
        if self._running:
            return True
        server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            server.bind((HOST, PORT))
            server.listen(5)
        except OSError:
            server.close()
            return False
        self._socket = server
        self._running = True
        self._thread = threading.Thread(target=self._serve, daemon=True)
        self._thread.start()
        return True

    def stop(self) -> None:
        self._running = False
        if self._socket is not None:
            try:
                self._socket.close()
            except OSError:
                pass
            self._socket = None

    def _serve(self) -> None:
        while self._running and self._socket is not None:
            try:
                connection, _address = self._socket.accept()
            except OSError:
                break
            with connection:
                try:
                    chunks: list[bytes] = []
                    while True:
                        data = connection.recv(65536)
                        if not data:
                            break
                        chunks.append(data)
                    message = json.loads(b"".join(chunks).decode("utf-8"))
                    if message.get("token") != TOKEN:
                        continue
                    paths = [str(path) for path in message.get("paths", []) if path]
                    if paths:
                        self.on_paths(paths)
                    connection.sendall(b"OK")
                except (OSError, UnicodeDecodeError, json.JSONDecodeError, TypeError, ValueError):
                    continue
