"""Portable Edit Plan operations shared by all NLE adapters."""

from __future__ import annotations

from dataclasses import asdict, dataclass
from enum import StrEnum
from typing import Any, Mapping, Protocol


class OperationKind(StrEnum):
    INSERT_CLIP = "insert_clip"
    TRIM_CLIP = "trim_clip"
    ADD_TITLE = "add_title"
    ADD_OVERLAY = "add_overlay"
    ADD_TRANSITION = "add_transition"
    ADD_MUSIC = "add_music"
    SET_VOLUME = "set_volume"
    ADD_SUBTITLE = "add_subtitle"
    APPLY_COLOR_PRESET = "apply_color_preset"


@dataclass(frozen=True, slots=True)
class EditOperation:
    id: str
    kind: OperationKind
    params: Mapping[str, Any]
    source_scene_id: str | None = None
    rationale: str | None = None

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


@dataclass(frozen=True, slots=True)
class EditDocument:
    schema_version: str
    project_id: str
    timeline_name: str
    fps: str
    operations: tuple[EditOperation, ...]

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


@dataclass(frozen=True, slots=True)
class AdapterCapability:
    operation: OperationKind
    supported: bool
    implementation: str
    limitations: tuple[str, ...] = ()


class EditAdapter(Protocol):
    """Only boundary allowed to contain software-specific editing code."""

    @property
    def adapter_id(self) -> str: ...

    def capabilities(self) -> tuple[AdapterCapability, ...]: ...

    def dry_run(self, plan: EditDocument) -> tuple[AdapterCapability, ...]: ...

    def apply(self, plan: EditDocument) -> Mapping[str, Any]: ...

