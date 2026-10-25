"""Deterministic hybrid media matching with explainable candidates."""

from __future__ import annotations

import math
import unicodedata
from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass

from .contracts import MediaCandidate, MatchResult
from .ir import SelectionRequest, ShotType


@dataclass(frozen=True, slots=True)
class IndexSegment:
    asset_id: str
    start_seconds: float
    end_seconds: float
    text: str = ""
    keywords: tuple[str, ...] = ()
    shot_type: str | None = None
    quality: float = 0.5

    @property
    def duration_seconds(self) -> float:
        return max(0.0, self.end_seconds - self.start_seconds)


def normalize_text(value: str) -> str:
    decomposed = unicodedata.normalize("NFKD", value.casefold())
    return " ".join("".join(character for character in decomposed if not unicodedata.combining(character)).split())


class ExplainableMediaMatcher:
    """Scores indexed segments without coupling the matcher to one embedding model."""

    def __init__(self, alternatives_limit: int = 5) -> None:
        self.alternatives_limit = max(1, alternatives_limit)

    def find(
        self,
        requests: Sequence[SelectionRequest],
        catalog: Sequence[IndexSegment],
    ) -> tuple[MatchResult, ...]:
        return tuple(self._find_one(request, catalog) for request in requests)

    def _find_one(self, request: SelectionRequest, catalog: Sequence[IndexSegment]) -> MatchResult:
        scored: list[MediaCandidate] = []
        request_text = normalize_text(f"{request.need.description} {' '.join(request.need.search_terms)}")
        for segment in catalog:
            candidate = self._score_segment(request, request_text, segment)
            if candidate is not None:
                scored.append(candidate)
        scored.sort(key=lambda item: (-item.score, item.asset_id, item.source_in_seconds))
        selected = scored[0] if scored and scored[0].score >= self._minimum_score(request) else None
        alternatives = tuple(scored[1 : self.alternatives_limit + 1])
        return MatchResult(request_id=request.request_id, selected=selected, alternatives=alternatives)

    @staticmethod
    def _minimum_score(request: SelectionRequest) -> float:
        return 0.35 if request.need.required_elements else 0.25

    def _score_segment(
        self,
        request: SelectionRequest,
        request_text: str,
        segment: IndexSegment,
    ) -> MediaCandidate | None:
        duration = segment.duration_seconds
        if duration <= 0:
            return None
        wanted_duration = (
            request.need.shot_duration.max_seconds
            if request.need.shot_duration is not None
            else duration
        )
        segment_text = normalize_text(f"{segment.text} {' '.join(segment.keywords)}")
        request_tokens = set(request_text.split())
        segment_tokens = set(segment_text.split())
        term_coverage = _coverage(request_tokens, segment_tokens)
        required_coverage = _element_coverage(request.need.required_elements, segment_text)
        shot_score = _shot_score(request.need.preferred_shots, segment.shot_type)
        duration_score = _duration_score(duration, wanted_duration)
        quality_score = max(0.0, min(1.0, segment.quality))

        breakdown = {
            "term_coverage": term_coverage,
            "required_elements": required_coverage,
            "shot_type": shot_score,
            "duration_fit": duration_score,
            "quality": quality_score,
        }
        score = (
            0.45 * term_coverage
            + 0.25 * required_coverage
            + 0.15 * shot_score
            + 0.10 * duration_score
            + 0.05 * quality_score
        )
        if not request_tokens and not request.need.required_elements and shot_score == 0:
            return None
        explanation = ", ".join(
            f"{name}={value:.2f}"
            for name, value in sorted(breakdown.items(), key=lambda item: -item[1])
        )
        return MediaCandidate(
            asset_id=segment.asset_id,
            source_in_seconds=segment.start_seconds,
            source_out_seconds=segment.end_seconds,
            score=round(score, 4),
            score_breakdown={key: round(value, 4) for key, value in breakdown.items()},
            explanation=f"Matched {request.purpose}: {explanation}",
        )


def _coverage(query: set[str], subject: set[str]) -> float:
    if not query:
        return 1.0
    matched = sum(1 for token in query if token in subject or any(item.startswith(token) for item in subject))
    return matched / len(query)


def _element_coverage(elements: Iterable[str], normalized_segment: str) -> float:
    values = tuple(elements)
    if not values:
        return 1.0
    matched = sum(1 for element in values if normalize_text(element) in normalized_segment)
    return matched / len(values)


def _shot_score(preferred: tuple[ShotType, ...], observed: str | None) -> float:
    if not preferred:
        return 0.5
    if observed is None:
        return 0.25
    if observed in {shot.value for shot in preferred}:
        return 1.0
    return 0.15


def _duration_score(observed: float, wanted: float) -> float:
    if wanted <= 0:
        return 0.5
    if observed >= wanted:
        return 1.0
    return max(0.0, 1.0 - abs(math.log(max(observed, 0.01) / wanted)))

