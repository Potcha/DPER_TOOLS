"""Deterministic FFmpeg/ffprobe media intelligence and index extraction."""

from __future__ import annotations

import json
import re
import subprocess
from collections.abc import Callable, Mapping, Sequence
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, Protocol

from .contracts import MediaAsset
from .errors import MediaIntelligenceError

CommandRunner = Callable[[tuple[str, ...]], "subprocess.CompletedProcess[str]"]


@dataclass(frozen=True, slots=True)
class AudioInterval:
    start_seconds: float
    end_seconds: float


@dataclass(frozen=True, slots=True)
class MediaAnalysis:
    asset_id: str
    path: str
    kind: str
    duration_seconds: float | None
    size_bytes: int | None
    codec_video: str | None
    codec_audio: str | None
    width: int | None
    height: int | None
    fps: str | None
    channels: int | None
    sample_rate: int | None
    start_time_seconds: float | None

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


class MediaAnalyzer(Protocol):
    def analyze(self, asset: MediaAsset) -> Mapping[str, Any]: ...


def default_runner(command: tuple[str, ...]) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(command, capture_output=True, text=True, check=False, timeout=120)
    except OSError as exc:
        raise MediaIntelligenceError(f"Cannot execute {command[0]}: {exc}") from exc
    except subprocess.TimeoutExpired as exc:
        raise MediaIntelligenceError(f"Media analysis timed out: {' '.join(command[:3])}") from exc


class FfprobeAnalyzer:
    def __init__(self, runner: CommandRunner = default_runner) -> None:
        self._runner = runner

    def analyze(self, asset: MediaAsset) -> Mapping[str, Any]:
        path = Path(asset.path).expanduser().resolve()
        if not path.is_file():
            raise MediaIntelligenceError(f"Media file is missing: {asset.path}")
        completed = self._runner(
            (
                "ffprobe",
                "-v",
                "error",
                "-show_format",
                "-show_streams",
                "-of",
                "json",
                str(path),
            )
        )
        if completed.returncode != 0:
            detail = completed.stderr.strip() or "unknown ffprobe error"
            raise MediaIntelligenceError(f"ffprobe rejected {path.name}: {detail}")
        try:
            raw = json.loads(completed.stdout)
        except json.JSONDecodeError as exc:
            raise MediaIntelligenceError(f"ffprobe returned invalid JSON for {path.name}") from exc
        return parse_ffprobe(asset_id=asset.id, path=str(path), raw=raw).to_dict()

    def detect_silences(
        self,
        asset: MediaAsset,
        *,
        noise_db: float = -35.0,
        min_duration_seconds: float = 0.4,
    ) -> tuple[AudioInterval, ...]:
        path = Path(asset.path).expanduser().resolve()
        if not path.is_file():
            raise MediaIntelligenceError(f"Media file is missing: {asset.path}")
        completed = self._runner(
            (
                "ffmpeg",
                "-hide_banner",
                "-i",
                str(path),
                "-map",
                "0:a:0",
                "-af",
                f"silencedetect=noise={noise_db}dB:d={min_duration_seconds}",
                "-f",
                "null",
                "-",
            )
        )
        if completed.returncode != 0:
            detail = completed.stderr.strip() or "unknown ffmpeg error"
            raise MediaIntelligenceError(f"Silence analysis failed for {path.name}: {detail}")
        return parse_silencedetect(completed.stderr)

    def detect_scene_changes(self, asset: MediaAsset, *, threshold: float = 0.3) -> tuple[float, ...]:
        path = Path(asset.path).expanduser().resolve()
        if not path.is_file():
            raise MediaIntelligenceError(f"Media file is missing: {asset.path}")
        completed = self._runner(
            (
                "ffmpeg",
                "-hide_banner",
                "-i",
                str(path),
                "-vf",
                f"select='gt(scene,{threshold})',showinfo",
                "-an",
                "-f",
                "null",
                "-",
            )
        )
        if completed.returncode != 0:
            detail = completed.stderr.strip() or "unknown ffmpeg error"
            raise MediaIntelligenceError(f"Scene analysis failed for {path.name}: {detail}")
        return parse_scene_change_times(completed.stderr)


def parse_ffprobe(*, asset_id: str, path: str, raw: Mapping[str, Any]) -> MediaAnalysis:
    format_info = raw.get("format", {})
    streams = raw.get("streams", [])
    video = next(
        (
            stream
            for stream in streams
            if stream.get("codec_type") == "video" and stream.get("disposition", {}).get("attached_pic") != 1
        ),
        None,
    )
    audio = next((stream for stream in streams if stream.get("codec_type") == "audio"), None)
    duration = _optional_float(format_info.get("duration"))
    if duration is None and video is not None:
        duration = _optional_float(video.get("duration"))
    suffix = Path(path).suffix.casefold()
    kind = "video" if video is not None else "audio" if audio is not None else "image"
    if kind == "video" and duration in {None, 0.0} and suffix in {".png", ".jpg", ".jpeg", ".webp", ".tif", ".tiff"}:
        kind = "image"
    fps = _parse_rate(video.get("avg_frame_rate")) if video else None
    return MediaAnalysis(
        asset_id=asset_id,
        path=path,
        kind=kind,
        duration_seconds=duration,
        size_bytes=_optional_int(format_info.get("size")),
        codec_video=video.get("codec_name") if video else None,
        codec_audio=audio.get("codec_name") if audio else None,
        width=_optional_int(video.get("width")) if video else None,
        height=_optional_int(video.get("height")) if video else None,
        fps=fps,
        channels=_optional_int(audio.get("channels")) if audio else None,
        sample_rate=_optional_int(audio.get("sample_rate")) if audio else None,
        start_time_seconds=_optional_float(format_info.get("start_time")),
    )


_SILENCE_START = re.compile(r"silence_start:\s*(-?[0-9.]+)")
_SILENCE_END = re.compile(r"silence_end:\s*(-?[0-9.]+)")
_SCENE_TIME = re.compile(r"pts_time:\s*([0-9.]+)")


def parse_silencedetect(stderr: str) -> tuple[AudioInterval, ...]:
    starts: list[float] = []
    intervals: list[AudioInterval] = []
    for line in stderr.splitlines():
        start_match = _SILENCE_START.search(line)
        if start_match:
            starts.append(float(start_match.group(1)))
            continue
        end_match = _SILENCE_END.search(line)
        if end_match:
            end = float(end_match.group(1))
            start = starts.pop(0) if starts else max(0.0, end - 1.0)
            intervals.append(AudioInterval(start_seconds=start, end_seconds=end))
    return tuple(intervals)


def parse_scene_change_times(stderr: str) -> tuple[float, ...]:
    return tuple(float(match.group(1)) for match in _SCENE_TIME.finditer(stderr))


def _optional_float(value: object) -> float | None:
    if value is None:
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def _optional_int(value: object) -> int | None:
    number = _optional_float(value)
    return int(number) if number is not None else None


def _parse_rate(value: object) -> str | None:
    if not isinstance(value, str) or value in {"", "0/0"}:
        return None
    return value

