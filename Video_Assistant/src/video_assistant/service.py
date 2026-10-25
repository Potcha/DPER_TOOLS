"""Application service used by the local HTTP API and future clients."""

from __future__ import annotations

from pathlib import Path
from typing import Any, Mapping, Sequence

from .matching import IndexSegment
from .media_intelligence import FfprobeAnalyzer
from .pipeline import ProductionPipeline
from .storage import SqliteStore
from .style import StyleProfile


class VideoAssistantService:
    def __init__(self, database_path: str | Path) -> None:
        self.store = SqliteStore(database_path)
        self.store.__enter__()
        self.pipeline = ProductionPipeline(self.store)
        self.analyzer = FfprobeAnalyzer()

    def close(self) -> None:
        self.store.__exit__(None, None, None)

    def create_project(self, payload: Mapping[str, Any]) -> Mapping[str, Any]:
        name = str(payload.get("name", "")).strip()
        if not name:
            raise ValueError("Project name is required")
        record = self.store.create_project(name)
        settings = {
            "id": record.id,
            "title": name,
            "style_id": payload.get("style_id", "crossroads_classic"),
            "delivery": payload.get(
                "delivery",
                {"platform": "youtube", "width": 1920, "height": 1080, "fps": "25"},
            ),
        }
        self.store.save_document(record.id, "project", settings, actor="user", summary="Create project")
        return {"id": record.id, "name": record.name, "created_utc": record.created_utc, "settings": settings}

    def generate_storyboard(self, project_id: str, payload: Mapping[str, Any]) -> Mapping[str, Any]:
        project = self.store.latest_document(project_id, "project")
        if project is None:
            raise KeyError(f"Unknown project: {project_id}")
        scenario = str(payload.get("scenario", "")).strip()
        if not scenario:
            raise ValueError("Scenario is required")
        storyboard, version = self.pipeline.generate_storyboard(
            project_id,
            scenario,
            project.data,
            actor=payload.get("actor", "user"),
        )
        return {"version": version.version, "storyboard": storyboard.to_dict()}

    def analyze_paths(self, paths: Sequence[str]) -> list[Mapping[str, Any]]:
        results: list[Mapping[str, Any]] = []
        for index, value in enumerate(paths):
            path = str(value).strip()
            if not path:
                continue
            asset_id = Path(path).stem or f"asset-{index}"
            analysis = self.analyzer.analyze(_asset(asset_id, path))
            results.append({"analysis": analysis, "segment": _segment_from_analysis(analysis)})
        return results

    def match_media(self, project_id: str, payload: Mapping[str, Any]) -> Mapping[str, Any]:
        latest = self.store.latest_document(project_id, "storyboard")
        if latest is None:
            raise ValueError("Generate a storyboard before matching media")
        from .ir import VideoIRDocument

        storyboard = VideoIRDocument.from_dict(latest.data)
        raw_segments = payload.get("segments", [])
        segments = tuple(_segment(item) for item in raw_segments)
        matches, version = self.pipeline.match_storyboard(project_id, storyboard, segments)
        return {
            "version": version.version,
            "matches": [
                {
                    "request_id": match.request_id,
                    "selected": _candidate(match.selected) if match.selected else None,
                    "alternatives": [_candidate(candidate) for candidate in match.alternatives],
                }
                for match in matches
            ],
        }

    def compile_plan(self, project_id: str) -> Mapping[str, Any]:
        from .contracts import MediaCandidate, MatchResult
        from .ir import VideoIRDocument

        storyboard_version = self.store.latest_document(project_id, "storyboard")
        match_version = self.store.latest_document(project_id, "media_match")
        style_version = self.store.latest_document(project_id, "style")
        if storyboard_version is None or match_version is None:
            raise ValueError("Storybook and media matches are required before compilation")
        if style_version is None:
            raise ValueError("Choose a video style before compilation")
        storyboard = VideoIRDocument.from_dict(storyboard_version.data)
        style = StyleProfile.from_dict(style_version.data)
        matches = []
        for item in match_version.data.get("results", []):
            candidates = {
                candidate["id"]: MediaCandidate(
                    asset_id=candidate["asset_id"],
                    source_in_seconds=float(candidate["source_in_seconds"]),
                    source_out_seconds=float(candidate["source_out_seconds"]),
                    score=float(candidate["score"]),
                    score_breakdown=dict(candidate.get("score_breakdown", {})),
                    explanation=str(candidate["explanation"]),
                )
                for candidate in item.get("candidates", [])
            }
            selected_id = item.get("selected_candidate_id")
            matches.append(
                MatchResult(
                    request_id=str(item["request_id"]),
                    selected=candidates.get(selected_id) if selected_id else None,
                    alternatives=tuple(candidate for key, candidate in candidates.items() if key != selected_id),
                )
            )
        result, version = self.pipeline.compile_plan(project_id, storyboard, matches, style)
        return {
            "version": version.version,
            "edit_plan": result.document.to_dict(),
            "warnings": [
                {"code": warning.code, "scene_id": warning.scene_id, "message": warning.message}
                for warning in result.warnings
            ],
        }

    def choose_style(self, project_id: str, style: Mapping[str, Any]) -> Mapping[str, Any]:
        profile = StyleProfile.from_dict(style)
        latest = self.store.latest_document(project_id, "style")
        version = self.store.save_document(
            project_id,
            "style",
            style,
            actor="user",
            summary=f"Choose style {profile.name}",
            parent_version=latest.version if latest else None,
        )
        return {"version": version.version, "style": style}


def _asset(asset_id: str, path: str) -> Any:
    from .contracts import MediaAsset

    return MediaAsset(asset_id, path, "video", None, {})


def _segment_from_analysis(analysis: Mapping[str, Any]) -> Mapping[str, Any]:
    return {
        "asset_id": analysis["asset_id"],
        "start_seconds": 0,
        "end_seconds": analysis.get("duration_seconds") or 1,
        "text": Path(str(analysis["path"])).stem,
        "keywords": [],
        "shot_type": None,
        "quality": 0.5,
    }


def _segment(raw: Mapping[str, Any]) -> IndexSegment:
    return IndexSegment(
        asset_id=str(raw["asset_id"]),
        start_seconds=float(raw["start_seconds"]),
        end_seconds=float(raw["end_seconds"]),
        text=str(raw.get("text", "")),
        keywords=tuple(str(value) for value in raw.get("keywords", [])),
        shot_type=raw.get("shot_type"),
        quality=float(raw.get("quality", 0.5)),
    )


def _candidate(candidate: Any) -> Mapping[str, Any]:
    return {
        "asset_id": candidate.asset_id,
        "source_in_seconds": candidate.source_in_seconds,
        "source_out_seconds": candidate.source_out_seconds,
        "score": candidate.score,
        "score_breakdown": dict(candidate.score_breakdown),
        "explanation": candidate.explanation,
    }

