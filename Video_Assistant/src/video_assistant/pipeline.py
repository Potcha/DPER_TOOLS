"""End-to-end application services joining deterministic production modules."""

from __future__ import annotations

from dataclasses import is_dataclass, asdict
from typing import Any, Mapping, Sequence

from .compiler import CompilationResult, DeterministicEditCompiler
from .contracts import MatchResult, StoryboardGenerator
from .director import RecipeStoryboardDirector
from .ir import VideoIRDocument
from .matching import ExplainableMediaMatcher, IndexSegment
from .storage import DocumentVersion, ProjectRecord, SqliteStore
from .style import StyleProfile


class ProductionPipeline:
    def __init__(
        self,
        store: SqliteStore,
        *,
        director: StoryboardGenerator | None = None,
        matcher: ExplainableMediaMatcher | None = None,
        compiler: DeterministicEditCompiler | None = None,
    ) -> None:
        self.store = store
        self.director = director or RecipeStoryboardDirector()
        self.matcher = matcher or ExplainableMediaMatcher()
        self.compiler = compiler or DeterministicEditCompiler()

    def create_project(self, name: str) -> ProjectRecord:
        return self.store.create_project(name)

    def generate_storyboard(
        self,
        project_id: str,
        scenario: str,
        project_settings: Mapping[str, Any],
        *,
        actor: str = "user",
    ) -> tuple[VideoIRDocument, DocumentVersion]:
        storyboard = self.director.generate(scenario, project_settings)
        latest = self.store.latest_document(project_id, "storyboard")
        version = self.store.save_document(
            project_id,
            "storyboard",
            _json_data(storyboard.to_dict()),
            actor=actor,
            summary="Generate storyboard from scenario",
            parent_version=latest.version if latest else None,
        )
        return storyboard, version

    def match_storyboard(
        self,
        project_id: str,
        storyboard: VideoIRDocument,
        segments: Sequence[IndexSegment],
        *,
        actor: str = "system",
    ) -> tuple[tuple[MatchResult, ...], DocumentVersion]:
        requests = tuple(selection for scene in storyboard.scenes for selection in scene.selections)
        matches = self.matcher.find(requests, segments)
        latest = self.store.latest_document(project_id, "media_match")
        version = self.store.save_document(
            project_id,
            "media_match",
            {
                "schema_version": "1.0.0",
                "project_id": project_id,
                "video_ir_version": storyboard.project.id,
                "results": [
                    {
                        "request_id": match.request_id,
                        "selected_candidate_id": _candidate_id(match.selected) if match.selected else None,
                        "candidates": [
                            {
                                "id": _candidate_id(candidate),
                                "asset_id": candidate.asset_id,
                                "source_in_seconds": candidate.source_in_seconds,
                                "source_out_seconds": candidate.source_out_seconds,
                                "score": candidate.score,
                                "score_breakdown": dict(candidate.score_breakdown),
                                "explanation": candidate.explanation,
                            }
                            for candidate in ((match.selected,) if match.selected else ()) + match.alternatives
                        ],
                    }
                    for match in matches
                ],
            },
            actor=actor,
            summary="Match indexed media to storyboard selections",
            parent_version=latest.version if latest else None,
        )
        return matches, version

    def compile_plan(
        self,
        project_id: str,
        storyboard: VideoIRDocument,
        matches: Sequence[MatchResult],
        style: StyleProfile,
        *,
        actor: str = "compiler",
    ) -> tuple[CompilationResult, DocumentVersion]:
        result = self.compiler.compile(storyboard, matches, style)
        latest = self.store.latest_document(project_id, "edit_plan")
        version = self.store.save_document(
            project_id,
            "edit_plan",
            _json_data(result.document.to_dict()),
            actor=actor,
            summary=f"Compile portable edit plan with {len(result.warnings)} warning(s)",
            parent_version=latest.version if latest else None,
        )
        return result, version


def _json_data(value: Mapping[str, Any] | Any) -> Mapping[str, Any]:
    if is_dataclass(value):
        value = asdict(value)
    return _strip_enums(value)


def _strip_enums(value: Any) -> Any:
    if isinstance(value, Mapping):
        return {key: _strip_enums(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_strip_enums(item) for item in value]
    return value


def _candidate_id(candidate: Any) -> str:
    return f"{candidate.asset_id}:{candidate.source_in_seconds:.3f}:{candidate.source_out_seconds:.3f}"

