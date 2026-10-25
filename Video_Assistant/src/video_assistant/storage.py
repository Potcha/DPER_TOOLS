"""SQLite project, immutable document version and job registries."""

from __future__ import annotations

import hashlib
import json
import sqlite3
import threading
import uuid
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, Mapping


@dataclass(frozen=True, slots=True)
class ProjectRecord:
    id: str
    name: str
    created_utc: str


@dataclass(frozen=True, slots=True)
class DocumentVersion:
    project_id: str
    kind: str
    version: int
    parent_version: int | None
    content_hash: str
    actor: str
    summary: str
    created_utc: str
    data: Mapping[str, Any]


@dataclass(frozen=True, slots=True)
class JobRecord:
    id: str
    project_id: str
    kind: str
    status: str
    progress: float
    message: str | None
    cancel_requested: bool
    payload: Mapping[str, Any]


class SqliteStore:
    def __init__(self, path: str | Path) -> None:
        self.path = Path(path)
        self._connection: sqlite3.Connection | None = None
        self._lock = threading.RLock()

    def __enter__(self) -> "SqliteStore":
        self.path.parent.mkdir(parents=True, exist_ok=True)
        connection = sqlite3.connect(self.path, check_same_thread=False)
        connection.row_factory = sqlite3.Row
        connection.execute("PRAGMA foreign_keys = ON")
        connection.execute("PRAGMA journal_mode = WAL")
        self._connection = connection
        self._create_schema()
        return self

    def __exit__(self, exc_type: object, exc: object, traceback: object) -> None:
        connection = self._require_connection()
        if exc_type is None:
            connection.commit()
        else:
            connection.rollback()
        connection.close()
        self._connection = None

    def create_project(self, name: str) -> ProjectRecord:
        with self._lock:
            connection = self._require_connection()
            project_id = uuid.uuid4().hex
            created_utc = _utc_now()
            connection.execute("INSERT INTO projects(id, name, created_utc) VALUES (?, ?, ?)", (project_id, name, created_utc))
            return ProjectRecord(project_id, name, created_utc)

    def save_document(
        self,
        project_id: str,
        kind: str,
        data: Mapping[str, Any],
        *,
        actor: str,
        summary: str,
        parent_version: int | None = None,
    ) -> DocumentVersion:
        with self._lock:
            connection = self._require_connection()
            encoded = json.dumps(data, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
            content_hash = hashlib.sha256(encoded.encode("utf-8")).hexdigest()
            current = connection.execute(
                "SELECT version FROM documents WHERE project_id = ? AND kind = ? ORDER BY version DESC LIMIT 1",
                (project_id, kind),
            ).fetchone()
            expected_parent = int(current["version"]) if current else None
            if parent_version != expected_parent:
                raise ValueError(f"Expected parent_version={expected_parent!r}, got {parent_version!r}")
            version = (expected_parent or 0) + 1
            created_utc = _utc_now()
            connection.execute(
                """INSERT INTO documents(
                       project_id, kind, version, parent_version, content_hash,
                       actor, summary, created_utc, data_json
                   ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)""",
                (project_id, kind, version, parent_version, content_hash, actor, summary, created_utc, encoded),
            )
            return DocumentVersion(project_id, kind, version, parent_version, content_hash, actor, summary, created_utc, data)

    def latest_document(self, project_id: str, kind: str) -> DocumentVersion | None:
        with self._lock:
            row = self._require_connection().execute(
                """SELECT * FROM documents
                   WHERE project_id = ? AND kind = ? ORDER BY version DESC LIMIT 1""",
                (project_id, kind),
            ).fetchone()
        return _document_from_row(row) if row else None

    def list_documents(self, project_id: str, kind: str) -> tuple[DocumentVersion, ...]:
        with self._lock:
            rows = self._require_connection().execute(
                "SELECT * FROM documents WHERE project_id = ? AND kind = ? ORDER BY version",
                (project_id, kind),
            ).fetchall()
        return tuple(_document_from_row(row) for row in rows)

    def create_job(self, project_id: str, kind: str, payload: Mapping[str, Any]) -> JobRecord:
        with self._lock:
            connection = self._require_connection()
            job_id = uuid.uuid4().hex
            encoded = json.dumps(payload, ensure_ascii=False, sort_keys=True)
            connection.execute(
                "INSERT INTO jobs(id, project_id, kind, status, progress, message, cancel_requested, payload_json) "
                "VALUES (?, ?, ?, 'queued', 0.0, NULL, 0, ?)",
                (job_id, project_id, kind, encoded),
            )
            return JobRecord(job_id, project_id, kind, "queued", 0.0, None, False, payload)

    def update_job(
        self,
        job_id: str,
        *,
        status: str,
        progress: float,
        message: str | None = None,
    ) -> JobRecord:
        if not 0.0 <= progress <= 1.0:
            raise ValueError("Job progress must be in [0, 1]")
        with self._lock:
            connection = self._require_connection()
            cursor = connection.execute(
                "UPDATE jobs SET status = ?, progress = ?, message = ? WHERE id = ?",
                (status, progress, message, job_id),
            )
            if cursor.rowcount != 1:
                raise KeyError(f"Unknown job: {job_id}")
            record = self.get_job(job_id)
            assert record is not None
            return record

    def request_job_cancel(self, job_id: str) -> bool:
        with self._lock:
            connection = self._require_connection()
            row = connection.execute("SELECT status FROM jobs WHERE id = ?", (job_id,)).fetchone()
            if row is None:
                raise KeyError(f"Unknown job: {job_id}")
            if row["status"] in {"succeeded", "failed", "cancelled"}:
                return False
            connection.execute("UPDATE jobs SET cancel_requested = 1 WHERE id = ?", (job_id,))
            return True

    def get_job(self, job_id: str) -> JobRecord | None:
        with self._lock:
            row = self._require_connection().execute("SELECT * FROM jobs WHERE id = ?", (job_id,)).fetchone()
        return _job_from_row(row) if row else None

    def _require_connection(self) -> sqlite3.Connection:
        if self._connection is None:
            raise RuntimeError("SqliteStore must be used as a context manager")
        return self._connection

    def _create_schema(self) -> None:
        self._require_connection().executescript(
            """
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                created_utc TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS documents (
                project_id TEXT NOT NULL REFERENCES projects(id),
                kind TEXT NOT NULL,
                version INTEGER NOT NULL,
                parent_version INTEGER,
                content_hash TEXT NOT NULL,
                actor TEXT NOT NULL,
                summary TEXT NOT NULL,
                created_utc TEXT NOT NULL,
                data_json TEXT NOT NULL CHECK(json_valid(data_json)),
                PRIMARY KEY(project_id, kind, version)
            );
            CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL REFERENCES projects(id),
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                progress REAL NOT NULL CHECK(progress >= 0.0 AND progress <= 1.0),
                message TEXT,
                cancel_requested INTEGER NOT NULL DEFAULT 0,
                payload_json TEXT NOT NULL CHECK(json_valid(payload_json))
            );
            """
        )


def _document_from_row(row: sqlite3.Row) -> DocumentVersion:
    return DocumentVersion(
        project_id=row["project_id"],
        kind=row["kind"],
        version=int(row["version"]),
        parent_version=int(row["parent_version"]) if row["parent_version"] is not None else None,
        content_hash=row["content_hash"],
        actor=row["actor"],
        summary=row["summary"],
        created_utc=row["created_utc"],
        data=json.loads(row["data_json"]),
    )


def _job_from_row(row: sqlite3.Row) -> JobRecord:
    return JobRecord(
        id=row["id"],
        project_id=row["project_id"],
        kind=row["kind"],
        status=row["status"],
        progress=float(row["progress"]),
        message=row["message"],
        cancel_requested=bool(row["cancel_requested"]),
        payload=json.loads(row["payload_json"]),
    )


def _utc_now() -> str:
    return datetime.now(UTC).isoformat(timespec="seconds")

