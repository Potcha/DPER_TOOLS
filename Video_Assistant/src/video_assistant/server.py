"""Dependency-free local HTTP API and static application host."""

from __future__ import annotations

import argparse
import json
import traceback
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Sequence
from urllib.parse import urlparse

from .errors import MediaIntelligenceError, VideoAssistantError
from .service import VideoAssistantService

ROOT = Path(__file__).resolve().parents[2]
WEB_ROOT = ROOT / "web"


class RequestHandler(BaseHTTPRequestHandler):
    service: VideoAssistantService

    def do_GET(self) -> None:  # noqa: N802
        path = urlparse(self.path).path
        if path == "/" or path == "/index.html":
            return self._file(WEB_ROOT / "index.html", "text/html; charset=utf-8")
        if path == "/app.css":
            return self._file(WEB_ROOT / "app.css", "text/css; charset=utf-8")
        if path == "/app.js":
            return self._file(WEB_ROOT / "app.js", "text/javascript; charset=utf-8")
        if path == "/api/health":
            return self._json({"status": "ok", "resolve_adapter": "unverified"})
        self._json({"error": {"code": "not_found", "message": "Route introuvable"}}, HTTPStatus.NOT_FOUND)

    def do_POST(self) -> None:  # noqa: N802
        path = urlparse(self.path).path
        try:
            payload = self._body()
            if path == "/api/projects":
                return self._json(self.service.create_project(payload), HTTPStatus.CREATED)
            if path == "/api/media/analyze":
                return self._json({"items": self.service.analyze_paths(payload.get("paths", []))})
            if path.endswith("/storyboard"):
                return self._json(self.service.generate_storyboard(_project_id(path), payload))
            if path.endswith("/style"):
                return self._json(self.service.choose_style(_project_id(path), payload.get("style", {})))
            if path.endswith("/match"):
                return self._json(self.service.match_media(_project_id(path), payload))
            if path.endswith("/compile"):
                return self._json(self.service.compile_plan(_project_id(path)))
            self._json({"error": {"code": "not_found", "message": "Route introuvable"}}, HTTPStatus.NOT_FOUND)
        except (ValueError, KeyError, json.JSONDecodeError) as exc:
            self._json({"error": {"code": "invalid_request", "message": str(exc)}}, HTTPStatus.BAD_REQUEST)
        except (VideoAssistantError, MediaIntelligenceError, OSError) as exc:
            self._json({"error": {"code": "operation_failed", "message": str(exc)}}, HTTPStatus.UNPROCESSABLE_ENTITY)
        except Exception as exc:
            traceback.print_exc()
            self._json(
                {
                    "error": {
                        "code": "internal_error",
                        "message": f"{type(exc).__name__}: {exc}",
                    }
                },
                HTTPStatus.INTERNAL_SERVER_ERROR,
            )

    def _body(self) -> dict[str, Any]:
        length = int(self.headers.get("Content-Length", "0"))
        if length == 0:
            return {}
        value = json.loads(self.rfile.read(length).decode("utf-8"))
        return value if isinstance(value, dict) else {}

    def _json(self, value: Any, status: HTTPStatus | int = HTTPStatus.OK) -> None:
        encoded = json.dumps(value, ensure_ascii=False, default=str).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(encoded)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(encoded)

    def _file(self, path: Path, content_type: str) -> None:
        if not path.is_file() or path.resolve().parent != WEB_ROOT.resolve():
            return self._json({"error": {"code": "not_found", "message": "Fichier introuvable"}}, HTTPStatus.NOT_FOUND)
        data = path.read_bytes()
        self.send_response(HTTPStatus.OK)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, format: str, *args: object) -> None:
        print(f"[{self.log_date_time_string()}] {format % args}")


def _project_id(path: str) -> str:
    parts = [part for part in path.split("/") if part]
    return parts[2] if len(parts) >= 3 else ""


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Video Assistant local application")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--database", default=str(ROOT / "data" / "video-assistant.db"))
    args = parser.parse_args(argv)
    service = VideoAssistantService(args.database)
    RequestHandler.service = service
    server = ThreadingHTTPServer((args.host, args.port), RequestHandler)
    print(f"Video Assistant running at http://{args.host}:{args.port}")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
        service.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

