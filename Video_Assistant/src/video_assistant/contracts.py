"""Stable interfaces between deterministic core and optional intelligence."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Mapping, Protocol, Sequence

from .edit_plan import EditDocument
from .ir import VideoIRDocument


@dataclass(frozen=True, slots=True)
class Message:
    role: str
    content: str


@dataclass(frozen=True, slots=True)
class LLMRequest:
    task: str
    messages: tuple[Message, ...]
    response_schema: Mapping[str, Any]
    temperature: float = 0.2


class LLMProvider(Protocol):
    """Provider-independent structured-output boundary for AI Director."""

    @property
    def provider_id(self) -> str: ...

    def complete_json(self, request: LLMRequest) -> Mapping[str, Any]: ...


class StoryboardGenerator(Protocol):
    def generate(self, scenario: str, project: Mapping[str, Any]) -> VideoIRDocument: ...


@dataclass(frozen=True, slots=True)
class MediaAsset:
    id: str
    path: str
    kind: str
    duration_seconds: float | None
    metadata: Mapping[str, Any]


@dataclass(frozen=True, slots=True)
class MediaCandidate:
    asset_id: str
    source_in_seconds: float
    source_out_seconds: float
    score: float
    score_breakdown: Mapping[str, float]
    explanation: str


@dataclass(frozen=True, slots=True)
class MatchResult:
    request_id: str
    selected: MediaCandidate | None
    alternatives: tuple[MediaCandidate, ...]


class MediaAnalyzer(Protocol):
    """Deterministic indexing step; optional semantic enrichment is layered on top."""

    def analyze(self, asset: MediaAsset) -> Mapping[str, Any]: ...


class MediaMatcher(Protocol):
    def find(self, requests: Sequence[Any], catalog: Sequence[MediaAsset]) -> tuple[MatchResult, ...]: ...


class EditCompiler(Protocol):
    def compile(
        self,
        storyboard: VideoIRDocument,
        matches: Sequence[MatchResult],
    ) -> EditDocument: ...


@dataclass(frozen=True, slots=True)
class Job:
    id: str
    kind: str
    status: str
    progress: float
    message: str | None


class JobRunner(Protocol):
    def submit(self, kind: str, payload: Mapping[str, Any]) -> Job: ...

    def cancel(self, job_id: str) -> bool: ...

    def status(self, job_id: str) -> Job: ...

