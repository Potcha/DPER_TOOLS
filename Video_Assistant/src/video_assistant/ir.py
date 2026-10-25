"""Typed Video IR models and deterministic structural validation.

The JSON Schema in ``schemas/video-ir.schema.json`` is the portable contract.
These dataclasses provide a dependency-free in-process representation and are
kept deliberately stricter than the schema for fields that the compiler needs.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass, field
from enum import StrEnum
from typing import Any, Mapping, Sequence, TypeAlias

from .errors import DocumentValidationError, ValidationError

IR_SCHEMA_VERSION = "1.0.0"


class NarrativeIntent(StrEnum):
    INTRO = "intro"
    HOOK = "hook"
    PRESENTATION = "presentation"
    INGREDIENTS = "ingredients"
    INSTRUCTION = "instruction"
    TRANSITION = "transition"
    RESULT = "result"
    TASTING = "tasting"
    OUTRO = "outro"


class Requirement(StrEnum):
    REQUIRED = "required"
    RECOMMENDED = "recommended"
    OPTIONAL = "optional"


class ShotType(StrEnum):
    EXTREME_CLOSE_UP = "extreme_close_up"
    CLOSE_UP = "close_up"
    MEDIUM = "medium"
    WIDE = "wide"
    OVERHEAD = "overhead"
    SCREEN_RECORDING = "screen_recording"


class Provenance(StrEnum):
    SCENARIO = "scenario"
    STYLE = "style"
    USER = "user"
    AI = "ai"
    DEFAULT = "default"


@dataclass(frozen=True, slots=True)
class DurationRange:
    min_seconds: float
    max_seconds: float

    def validate(self, path: str, errors: list[ValidationError]) -> None:
        if self.min_seconds <= 0:
            errors.append(ValidationError("duration_min", path, "minimum duration must be positive"))
        if self.max_seconds < self.min_seconds:
            errors.append(
                ValidationError(
                    "duration_range",
                    path,
                    "maximum duration must be greater than or equal to minimum duration",
                )
            )


@dataclass(frozen=True, slots=True)
class DeliverySpec:
    platform: str
    width: int
    height: int
    fps: str
    target_duration_seconds: float | None = None


@dataclass(frozen=True, slots=True)
class ProjectSpec:
    id: str
    title: str
    delivery: DeliverySpec
    style_id: str


@dataclass(frozen=True, slots=True)
class NarrationSpec:
    text: str
    source_id: str | None = None
    keep_complete_sentences: bool = True
    optional: bool = False


@dataclass(frozen=True, slots=True)
class VisualNeed:
    description: str
    search_terms: tuple[str, ...] = ()
    preferred_shots: tuple[ShotType, ...] = ()
    shot_duration: DurationRange | None = None
    required_elements: tuple[str, ...] = ()
    optional_elements: tuple[str, ...] = ()


@dataclass(frozen=True, slots=True)
class SelectionRequest:
    request_id: str
    purpose: str
    need: VisualNeed
    min_candidates: int = 1
    fallback: str | None = None


@dataclass(frozen=True, slots=True)
class GraphicSpec:
    kind: str
    text: str
    requirement: Requirement = Requirement.OPTIONAL
    start_seconds: float | None = None
    duration_seconds: float | None = None
    template_id: str | None = None


@dataclass(frozen=True, slots=True)
class MusicSpec:
    source_id: str | None
    profile_id: str | None
    level_db: float = -20.0
    duck_level_db: float | None = None


@dataclass(frozen=True, slots=True)
class AudioSpec:
    voice_priority: bool = True
    keep_natural_sound: bool = True
    music: MusicSpec | None = None


@dataclass(frozen=True, slots=True)
class TransitionSpec:
    kind: str = "cut"
    duration_frames: int = 0


@dataclass(frozen=True, slots=True)
class EditingSpec:
    target_seconds: float
    tolerance_seconds: float = 2.0
    transitions: tuple[TransitionSpec, ...] = ()
    pace: str = "steady"


@dataclass(frozen=True, slots=True)
class ConstraintSpec:
    mandatory_elements: tuple[str, ...] = ()
    forbidden_elements: tuple[str, ...] = ()
    fallbacks: tuple[str, ...] = ()


@dataclass(frozen=True, slots=True)
class DecisionRecord:
    id: str
    decision: str
    provenance: Provenance
    reason: str
    confidence: float | None = None


def _enum(
    enum_type: type[StrEnum], value: object, path: str, errors: list[ValidationError], default: str
) -> Any:
    if isinstance(value, str):
        try:
            return enum_type(value)
        except ValueError:
            allowed = ", ".join(item.value for item in enum_type)
            errors.append(ValidationError("enum_value", path, f"unknown value {value!r}", f"one of: {allowed}"))
    else:
        errors.append(ValidationError("type_string", path, f"expected {enum_type.__name__} value"))
    return enum_type(default)


@dataclass(frozen=True, slots=True)
class Scene:
    id: str
    title: str
    purpose: str
    intent: NarrativeIntent
    requirement: Requirement
    editing: EditingSpec
    narration: NarrationSpec | None = None
    visual: VisualNeed | None = None
    selections: tuple[SelectionRequest, ...] = ()
    graphics: tuple[GraphicSpec, ...] = ()
    audio: AudioSpec = field(default_factory=AudioSpec)
    constraints: ConstraintSpec = field(default_factory=ConstraintSpec)
    decisions: tuple[DecisionRecord, ...] = ()


@dataclass(frozen=True, slots=True)
class VideoIRDocument:
    schema_version: str
    project: ProjectSpec
    scenes: tuple[Scene, ...]

    @classmethod
    def from_dict(cls, raw: Mapping[str, Any]) -> "VideoIRDocument":
        errors: list[ValidationError] = []
        if raw.get("schema_version") != IR_SCHEMA_VERSION:
            errors.append(
                ValidationError(
                    "schema_version",
                    "$.schema_version",
                    f"expected {IR_SCHEMA_VERSION}, got {raw.get('schema_version')!r}",
                    "migrate the document explicitly before compiling",
                )
            )
        project_raw = _mapping(raw.get("project"), "$.project", errors)
        project = _parse_project(project_raw, errors) if project_raw is not None else None
        scenes_raw = _sequence(raw.get("scenes"), "$.scenes", errors)
        scenes = tuple(
            _parse_scene(_mapping(item, f"$.scenes[{index}]", errors), f"$.scenes[{index}]", errors)
            for index, item in enumerate(scenes_raw or ())
        )
        if not scenes:
            errors.append(ValidationError("scenes_empty", "$.scenes", "at least one scene is required"))
        ids = [scene.id for scene in scenes]
        if len(ids) != len(set(ids)):
            errors.append(ValidationError("scene_id_duplicate", "$.scenes", "scene ids must be unique"))
        if errors or project is None:
            raise DocumentValidationError(tuple(errors))
        return cls(schema_version=IR_SCHEMA_VERSION, project=project, scenes=scenes)

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


JsonObject: TypeAlias = Mapping[str, Any]


def _mapping(value: object, path: str, errors: list[ValidationError]) -> JsonObject | None:
    if isinstance(value, Mapping):
        return value
    errors.append(ValidationError("type_object", path, "expected an object"))
    return None


def _sequence(value: object, path: str, errors: list[ValidationError]) -> Sequence[Any] | None:
    if isinstance(value, Sequence) and not isinstance(value, (str, bytes, bytearray)):
        return value
    errors.append(ValidationError("type_array", path, "expected an array"))
    return None


def _required_string(raw: JsonObject, key: str, path: str, errors: list[ValidationError]) -> str:
    value = raw.get(key)
    if not isinstance(value, str) or not value.strip():
        errors.append(ValidationError("required_string", f"{path}.{key}", "expected a non-empty string"))
        return ""
    return value


def _number(value: object, path: str, errors: list[ValidationError]) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        errors.append(ValidationError("type_number", path, "expected a number"))
        return 0.0
    return float(value)


def _parse_project(raw: JsonObject, errors: list[ValidationError]) -> ProjectSpec:
    delivery_raw = _mapping(raw.get("delivery"), "$.project.delivery", errors)
    delivery = DeliverySpec(
        platform=_required_string(delivery_raw or {}, "platform", "$.project.delivery", errors),
        width=int(_number((delivery_raw or {}).get("width"), "$.project.delivery.width", errors)),
        height=int(_number((delivery_raw or {}).get("height"), "$.project.delivery.height", errors)),
        fps=_required_string(delivery_raw or {}, "fps", "$.project.delivery", errors),
        target_duration_seconds=_optional_number(
            (delivery_raw or {}).get("target_duration_seconds"),
            "$.project.delivery.target_duration_seconds",
            errors,
        ),
    )
    return ProjectSpec(
        id=_required_string(raw, "id", "$.project", errors),
        title=_required_string(raw, "title", "$.project", errors),
        delivery=delivery,
        style_id=_required_string(raw, "style_id", "$.project", errors),
    )


def _parse_scene(raw: JsonObject, path: str, errors: list[ValidationError]) -> Scene:
    editing_raw = _mapping(raw.get("editing"), f"{path}.editing", errors) or {}
    editing = EditingSpec(
        target_seconds=_number(editing_raw.get("target_seconds"), f"{path}.editing.target_seconds", errors),
        tolerance_seconds=_number(
            editing_raw.get("tolerance_seconds", 2.0), f"{path}.editing.tolerance_seconds", errors
        ),
        pace=_required_string(editing_raw, "pace", f"{path}.editing", errors),
        transitions=_parse_transitions(editing_raw.get("transitions"), f"{path}.editing.transitions", errors),
    )
    visual_raw = raw.get("visual")
    visual = (
        _parse_visual(_mapping(visual_raw, f"{path}.visual", errors) or {}, f"{path}.visual", errors)
        if visual_raw is not None
        else None
    )
    narration_raw = raw.get("narration")
    narration = (
        _parse_narration(_mapping(narration_raw, f"{path}.narration", errors) or {}, f"{path}.narration", errors)
        if narration_raw is not None
        else None
    )
    selections_raw = _sequence(raw.get("selections", []), f"{path}.selections", errors) or ()
    graphics_raw = _sequence(raw.get("graphics", []), f"{path}.graphics", errors) or ()
    constraints_value = raw.get("constraints", {})
    constraints_raw = _mapping(constraints_value, f"{path}.constraints", errors) if constraints_value is not None else {}
    decisions_raw = _sequence(raw.get("decisions", []), f"{path}.decisions", errors) or ()
    return Scene(
        id=_required_string(raw, "id", path, errors),
        title=_required_string(raw, "title", path, errors),
        purpose=_required_string(raw, "purpose", path, errors),
        intent=_enum(NarrativeIntent, raw.get("intent"), f"{path}.intent", errors, "instruction"),
        requirement=_enum(Requirement, raw.get("requirement"), f"{path}.requirement", errors, "optional"),
        editing=editing,
        narration=narration,
        visual=visual,
        selections=tuple(
            _parse_selection(_mapping(item, f"{path}.selections[{index}]", errors) or {}, path, errors)
            for index, item in enumerate(selections_raw)
        ),
        graphics=tuple(_parse_graphic(item, f"{path}.graphics", errors) for item in graphics_raw),
        audio=(
            _parse_audio(raw["audio"], f"{path}.audio", errors)
            if raw.get("audio") is not None
            else AudioSpec()
        ),
        constraints=_parse_constraints(constraints_raw or {}, f"{path}.constraints", errors),
        decisions=tuple(
            _parse_decision(_mapping(item, f"{path}.decisions[{index}]", errors) or {}, path, errors)
            for index, item in enumerate(decisions_raw)
        ),
    )


def _parse_narration(raw: JsonObject, path: str, errors: list[ValidationError]) -> NarrationSpec:
    return NarrationSpec(
        text=_required_string(raw, "text", path, errors),
        source_id=_optional_string(raw.get("source_id"), f"{path}.source_id", errors),
        keep_complete_sentences=bool(raw.get("keep_complete_sentences", True)),
        optional=bool(raw.get("optional", False)),
    )


def _parse_visual(raw: JsonObject, path: str, errors: list[ValidationError]) -> VisualNeed:
    shot_raw = _sequence(raw.get("preferred_shots", []), f"{path}.preferred_shots", errors) or ()
    shot_duration_raw = raw.get("shot_duration")
    return VisualNeed(
        description=_required_string(raw, "description", path, errors),
        search_terms=_strings(raw.get("search_terms", []), f"{path}.search_terms", errors),
        preferred_shots=tuple(
            _enum(ShotType, value, f"{path}.preferred_shots[{index}]", errors, "medium")
            for index, value in enumerate(shot_raw)
        ),
        shot_duration=(
            _parse_duration(_mapping(shot_duration_raw, f"{path}.shot_duration", errors) or {}, path, errors)
            if shot_duration_raw is not None
            else None
        ),
        required_elements=_strings(raw.get("required_elements", []), f"{path}.required_elements", errors),
        optional_elements=_strings(raw.get("optional_elements", []), f"{path}.optional_elements", errors),
    )


def _parse_selection(raw: JsonObject, path: str, errors: list[ValidationError]) -> SelectionRequest:
    need_raw = _mapping(raw.get("need"), f"{path}.need", errors) or {}
    return SelectionRequest(
        request_id=_required_string(raw, "request_id", path, errors),
        purpose=_required_string(raw, "purpose", path, errors),
        need=_parse_visual(need_raw, f"{path}.need", errors),
        min_candidates=max(1, int(_number(raw.get("min_candidates", 1), f"{path}.min_candidates", errors))),
        fallback=_optional_string(raw.get("fallback"), f"{path}.fallback", errors),
    )


def _parse_graphic(raw: object, path: str, errors: list[ValidationError]) -> GraphicSpec:
    obj = _mapping(raw, path, errors) or {}
    return GraphicSpec(
        kind=_required_string(obj, "kind", path, errors),
        text=_required_string(obj, "text", path, errors),
        requirement=_enum(Requirement, obj.get("requirement", "optional"), f"{path}.requirement", errors, "optional"),
        start_seconds=_optional_number(obj.get("start_seconds"), f"{path}.start_seconds", errors),
        duration_seconds=_optional_number(obj.get("duration_seconds"), f"{path}.duration_seconds", errors),
        template_id=_optional_string(obj.get("template_id"), f"{path}.template_id", errors),
    )


def _parse_audio(raw: object, path: str, errors: list[ValidationError]) -> AudioSpec:
    obj = _mapping(raw, path, errors) or {}
    music_raw = obj.get("music")
    music = (
        _parse_music(_mapping(music_raw, f"{path}.music", errors) or {}, f"{path}.music", errors)
        if music_raw is not None
        else None
    )
    return AudioSpec(
        voice_priority=bool(obj.get("voice_priority", True)),
        keep_natural_sound=bool(obj.get("keep_natural_sound", True)),
        music=music,
    )


def _parse_music(raw: JsonObject, path: str, errors: list[ValidationError]) -> MusicSpec:
    return MusicSpec(
        source_id=_optional_string(raw.get("source_id"), f"{path}.source_id", errors),
        profile_id=_optional_string(raw.get("profile_id"), f"{path}.profile_id", errors),
        level_db=_number(raw.get("level_db", -20.0), f"{path}.level_db", errors),
        duck_level_db=_optional_number(raw.get("duck_level_db"), f"{path}.duck_level_db", errors),
    )


def _parse_constraints(raw: JsonObject, path: str, errors: list[ValidationError]) -> ConstraintSpec:
    return ConstraintSpec(
        mandatory_elements=_strings(raw.get("mandatory_elements", []), f"{path}.mandatory_elements", errors),
        forbidden_elements=_strings(raw.get("forbidden_elements", []), f"{path}.forbidden_elements", errors),
        fallbacks=_strings(raw.get("fallbacks", []), f"{path}.fallbacks", errors),
    )


def _parse_decision(raw: JsonObject, path: str, errors: list[ValidationError]) -> DecisionRecord:
    confidence = _optional_number(raw.get("confidence"), f"{path}.confidence", errors)
    if confidence is not None and not 0 <= confidence <= 1:
        errors.append(ValidationError("confidence_range", f"{path}.confidence", "confidence must be in [0, 1]"))
    return DecisionRecord(
        id=_required_string(raw, "id", path, errors),
        decision=_required_string(raw, "decision", path, errors),
        provenance=_enum(Provenance, raw.get("provenance"), f"{path}.provenance", errors, "default"),
        reason=_required_string(raw, "reason", path, errors),
        confidence=confidence,
    )


def _parse_transitions(raw: object, path: str, errors: list[ValidationError]) -> tuple[TransitionSpec, ...]:
    values = _sequence(raw, path, errors) or ()
    return tuple(
        TransitionSpec(
            kind=_required_string(item_raw := _mapping(item, f"{path}[{index}]", errors) or {}, "kind", path, errors),
            duration_frames=max(
                0,
                int(_number(item_raw.get("duration_frames", 0), f"{path}[{index}].duration_frames", errors)),
            ),
        )
        for index, item in enumerate(values)
    )


def _parse_duration(raw: JsonObject, path: str, errors: list[ValidationError]) -> DurationRange:
    value = DurationRange(
        min_seconds=_number(raw.get("min_seconds"), f"{path}.min_seconds", errors),
        max_seconds=_number(raw.get("max_seconds"), f"{path}.max_seconds", errors),
    )
    value.validate(path, errors)
    return value


def _strings(raw: object, path: str, errors: list[ValidationError]) -> tuple[str, ...]:
    values = _sequence(raw, path, errors) or ()
    result: list[str] = []
    for index, value in enumerate(values):
        if not isinstance(value, str) or not value.strip():
            errors.append(ValidationError("type_string", f"{path}[{index}]", "expected a non-empty string"))
        else:
            result.append(value)
    return tuple(result)


def _optional_string(value: object, path: str, errors: list[ValidationError]) -> str | None:
    if value is None:
        return None
    if not isinstance(value, str) or not value.strip():
        errors.append(ValidationError("type_string", path, "expected a non-empty string or null"))
        return None
    return value


def _optional_number(value: object, path: str, errors: list[ValidationError]) -> float | None:
    if value is None:
        return None
    return _number(value, path, errors)

