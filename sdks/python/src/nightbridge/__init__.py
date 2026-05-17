"""Small Python facade for the NightBridge local HTTP API."""

from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Any, Optional
from urllib import error, request

__version__ = "0.1.0"


class Client:
    """HTTP client for the daemon's `/api/v1` control plane."""

    def __init__(
        self,
        base_url: str = "http://127.0.0.1:53501",
        token: Optional[str] = None,
        token_from_file: bool = False,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.token = token or (_load_token() if token_from_file else "")
        self.peers = PeersClient(self)
        self.transfers = TransfersClient(self)

    def get(self, path: str) -> Any:
        return self._json("GET", path)

    def post(self, path: str, payload: dict[str, Any]) -> Any:
        return self._json("POST", path, payload)

    def _json(self, method: str, path: str, payload: Optional[dict[str, Any]] = None) -> Any:
        body = None if payload is None else json.dumps(payload).encode("utf-8")
        headers = {"Accept": "application/json", "Authorization": f"Bearer {self.token}"}
        if body is not None:
            headers["Content-Type"] = "application/json"
        req = request.Request(f"{self.base_url}{path}", data=body, headers=headers, method=method)
        try:
            with request.urlopen(req, timeout=10) as response:
                return json.loads(response.read().decode("utf-8"))
        except error.HTTPError as exc:
            body = exc.read().decode("utf-8", errors="replace")
            raise RuntimeError(f"{exc.code} {exc.reason}: {body}") from exc


class PeersClient:
    def __init__(self, client: Client) -> None:
        self._client = client

    def list_trusted(self) -> list[dict[str, Any]]:
        return self._client.get("/api/v1/peers/trusted")["peers"]


class TransfersClient:
    def __init__(self, client: Client) -> None:
        self._client = client

    def send(self, **payload: Any) -> dict[str, Any]:
        if "paths" in payload:
            payload["paths"] = [str(Path(path).expanduser().resolve()) for path in payload["paths"]]
        return self._client.post("/api/v1/transfers/send", payload)


def _load_token() -> str:
    override = os.environ.get("LOCALSEND_IMPROVED_API_TOKEN_FILE")
    candidates = [
        Path(override) if override else None,
        Path.home() / ".config" / "night-bridge" / "api.token",
        Path.home() / "Library" / "Application Support" / "dev.nightbridge.night-bridge" / "api.token",
    ]
    for candidate in candidates:
        if candidate and candidate.exists():
            return candidate.read_text(encoding="utf-8").strip()
    raise FileNotFoundError("api.token not found; pass token=... or set LOCALSEND_IMPROVED_API_TOKEN_FILE")


__all__ = ["Client", "__version__"]
