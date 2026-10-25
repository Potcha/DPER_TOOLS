"""Dependency-free validation CLI used by CI and local development."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any, Sequence

from .errors import DocumentValidationError
from .ir import VideoIRDocument


def _load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except OSError as exc:
        raise SystemExit(f"Cannot read {path}: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise SystemExit(f"Invalid JSON in {path}:{exc.lineno}:{exc.colno}: {exc.msg}") from exc


def validate(path: Path) -> int:
    try:
        document = VideoIRDocument.from_dict(_load_json(path))
    except DocumentValidationError as exc:
        for error in exc.errors:
            print(error)
        return 2
    scene_count = len(document.scenes)
    duration = sum(scene.editing.target_seconds for scene in document.scenes)
    print(f"VALID {path} | {scene_count} scenes | target {duration:.2f}s")
    return 0


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="video-assistant")
    subparsers = parser.add_subparsers(dest="command", required=True)
    validate_parser = subparsers.add_parser("validate", help="validate a Video IR JSON document")
    validate_parser.add_argument("path", type=Path)
    args = parser.parse_args(argv)
    if args.command == "validate":
        return validate(args.path)
    parser.error(f"unknown command: {args.command}")


if __name__ == "__main__":
    raise SystemExit(main())

