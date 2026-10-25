"""Deterministic Video IR + matches + style to portable Edit Plan compiler."""

from __future__ import annotations

from dataclasses import dataclass
from decimal import Decimal, ROUND_HALF_UP
from fractions import Fraction
from typing import Mapping, Sequence

from .contracts import MatchResult
from .edit_plan import EditDocument, EditOperation, OperationKind
from .errors import CompilationError, ValidationError
from .ir import ProjectSpec, Requirement, Scene, VideoIRDocument
from .plan_validation import validate_edit_document
from .style import StyleProfile


@dataclass(frozen=True, slots=True)
class CompilationWarning:
    code: str
    scene_id: str
    message: str


@dataclass(frozen=True, slots=True)
class CompilationResult:
    document: EditDocument
    warnings: tuple[CompilationWarning, ...]


def parse_fps(value: str) -> Fraction:
    parts = value.split("/", maxsplit=1)
    numerator = int(parts[0])
    denominator = int(parts[1]) if len(parts) == 2 else 1
    if numerator <= 0 or denominator <= 0:
        raise CompilationError(f"Invalid frame rate: {value}")
    return Fraction(numerator, denominator)


def seconds_to_frames(seconds: float, fps: Fraction) -> int:
    frames = (Decimal(str(seconds)) * Decimal(fps.numerator) / Decimal(fps.denominator)).to_integral_value(
        rounding=ROUND_HALF_UP
    )
    return max(1, int(frames))


class DeterministicEditCompiler:
    """Pure compiler used identically for dry runs and NLE application."""

    def compile(
        self,
        storyboard: VideoIRDocument,
        matches: Sequence[MatchResult] | Mapping[str, MatchResult],
        style: StyleProfile,
    ) -> CompilationResult:
        match_by_request = self._index_matches(matches)
        fps = parse_fps(storyboard.project.delivery.fps)
        operations: list[EditOperation] = []
        warnings: list[CompilationWarning] = []
        cursor = 0

        for scene in storyboard.scenes:
            scene_result = self._compile_scene(scene, match_by_request, style, fps, cursor)
            operations.extend(scene_result[0])
            warnings.extend(scene_result[1])
            cursor += seconds_to_frames(scene.editing.target_seconds, fps)

        operations.extend(self._compile_audio(storyboard, style))
        document = EditDocument(
            schema_version="1.0.0",
            project_id=storyboard.project.id,
            timeline_name=f"{storyboard.project.title} - Auto",
            fps=storyboard.project.delivery.fps,
            operations=tuple(operations),
        )
        validation = validate_edit_document(document)
        if validation:
            raise CompilationError("\n".join(str(error) for error in validation))
        return CompilationResult(document=document, warnings=tuple(warnings))

    @staticmethod
    def _index_matches(
        matches: Sequence[MatchResult] | Mapping[str, MatchResult],
    ) -> Mapping[str, MatchResult]:
        return matches if isinstance(matches, Mapping) else {item.request_id: item for item in matches}

    def _compile_scene(
        self,
        scene: Scene,
        matches: Mapping[str, MatchResult],
        style: StyleProfile,
        fps: Fraction,
        record_frame: int,
    ) -> tuple[list[EditOperation], list[CompilationWarning]]:
        if not scene.selections:
            if scene.requirement == Requirement.REQUIRED:
                raise CompilationError(f"Required scene {scene.id!r} has no media selection request")
            return [], [CompilationWarning("scene_without_selection", scene.id, "Scene omitted from automatic edit")]

        operations: list[EditOperation] = []
        warnings: list[CompilationWarning] = []
        selection = scene.selections[0]
        match = matches.get(selection.request_id)
        if match is None or match.selected is None:
            message = f"No selected media for {selection.request_id!r}"
            if scene.requirement == Requirement.REQUIRED:
                raise CompilationError(message)
            return [], [CompilationWarning("missing_match", scene.id, message)]

        candidate = match.selected
        desired_frames = seconds_to_frames(scene.editing.target_seconds, fps)
        source_in = seconds_to_frames(candidate.source_in_seconds, fps) - 1
        source_out = seconds_to_frames(candidate.source_out_seconds, fps) - 1
        available_frames = max(0, source_out - source_in + 1)
        duration_frames = min(desired_frames, available_frames)
        if duration_frames == 0:
            raise CompilationError(f"Candidate {candidate.asset_id!r} has no usable frames")
        if duration_frames < desired_frames:
            warnings.append(
                CompilationWarning(
                    "duration_short",
                    scene.id,
                    f"Candidate supplies {duration_frames}/{desired_frames} frames; later scenes are not compacted implicitly",
                )
            )

        insert_id = f"op-{scene.id}-{selection.request_id}-insert"
        operations.append(
            EditOperation(
                id=insert_id,
                kind=OperationKind.INSERT_CLIP,
                params={
                    "asset_id": candidate.asset_id,
                    "track": "V1",
                    "record_frame": record_frame,
                    "source_in_frames": source_in,
                    "source_out_frames": source_out,
                    "duration_frames": duration_frames,
                },
                source_scene_id=scene.id,
                rationale=candidate.explanation,
            )
        )
        operations.append(
            EditOperation(
                id=f"op-{scene.id}-trim",
                kind=OperationKind.TRIM_CLIP,
                params={
                    "operation_id": insert_id,
                    "source_in_frames": source_in,
                    "source_out_frames": source_in + duration_frames - 1,
                },
                source_scene_id=scene.id,
                rationale="Respect scene target duration while retaining the selected gesture",
            )
        )

        for index, graphic in enumerate(scene.graphics):
            if graphic.requirement == Requirement.OPTIONAL and graphic.template_id is None:
                continue
            graphic_start = graphic.start_seconds or 0.0
            graphic_duration = graphic.duration_seconds or min(scene.editing.target_seconds, 5.0)
            kind = OperationKind.ADD_TITLE if graphic.kind in {"title", "chapter", "lower_third"} else OperationKind.ADD_OVERLAY
            template_id = graphic.template_id or self._template_for(graphic.kind, style)
            operations.append(
                EditOperation(
                    id=f"op-{scene.id}-graphic-{index}",
                    kind=kind,
                    params={
                        "template_id": template_id,
                        "text": graphic.text,
                        "track": "V2",
                        "record_frame": record_frame + seconds_to_frames(graphic_start, fps),
                        "duration_frames": seconds_to_frames(graphic_duration, fps),
                    },
                    source_scene_id=scene.id,
                    rationale=f"Semantic graphic: {graphic.kind}",
                )
            )

        if scene.narration:
            operations.append(
                EditOperation(
                    id=f"op-{scene.id}-subtitle",
                    kind=OperationKind.ADD_SUBTITLE,
                    params={
                        "template_id": style.graphics.subtitle_template_id,
                        "text": scene.narration.text,
                        "record_frame": record_frame,
                        "duration_frames": duration_frames,
                    },
                    source_scene_id=scene.id,
                    rationale="Narration is represented as an editable subtitle cue",
                )
            )

        operations.append(
            EditOperation(
                id=f"op-{scene.id}-color",
                kind=OperationKind.APPLY_COLOR_PRESET,
                params={"preset_id": f"{style.id}:color", "target_operation_id": insert_id},
                source_scene_id=scene.id,
                rationale=f"Style rule {style.id}.color",
            )
        )

        transition = scene.editing.transitions[-1] if scene.editing.transitions else None
        if transition and transition.kind not in {"cut"}:
            operations.append(
                EditOperation(
                    id=f"op-{scene.id}-transition-out",
                    kind=OperationKind.ADD_TRANSITION,
                    params={
                        "operation_id": insert_id,
                        "kind": transition.kind,
                        "position": "end",
                        "duration_frames": transition.duration_frames,
                    },
                    source_scene_id=scene.id,
                    rationale="Scene transition intent",
                )
            )
        return operations, warnings

    @staticmethod
    def _compile_audio(storyboard: VideoIRDocument, style: StyleProfile) -> list[EditOperation]:
        music = next((scene.audio.music for scene in storyboard.scenes if scene.audio.music), None)
        if music is None or music.source_id is None:
            return []
        source_id = music.source_id
        operations = [
            EditOperation(
                id="op-music-master",
                kind=OperationKind.ADD_MUSIC,
                params={"asset_id": source_id, "track": "A3", "record_frame": 0},
                rationale="Project music bed",
            ),
            EditOperation(
                id="op-music-level",
                kind=OperationKind.SET_VOLUME,
                params={
                    "operation_id": "op-music-master",
                    "level_db": music.level_db if music.level_db != -20.0 else style.audio.music_level_db,
                },
                rationale="Style music level",
            ),
        ]
        duck_level = music.duck_level_db if music.duck_level_db is not None else style.audio.duck_level_db
        operations.append(
            EditOperation(
                id="op-music-duck",
                kind=OperationKind.SET_VOLUME,
                params={"operation_id": "op-music-master", "level_db": duck_level},
                rationale="Voice priority ducking level",
            )
        )
        return operations

    @staticmethod
    def _template_for(kind: str, style: StyleProfile) -> str:
        if kind == "ingredient_list":
            return style.graphics.ingredients_template_id
        return style.graphics.title_template_id

