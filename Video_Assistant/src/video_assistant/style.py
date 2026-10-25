"""Versioned video style profiles separated from narrative intent."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Mapping


@dataclass(frozen=True, slots=True)
class EditingStyle:
    shot_min_seconds: float
    shot_max_seconds: float
    default_transition: str
    avoid_jump_cuts: bool


@dataclass(frozen=True, slots=True)
class AudioStyle:
    music_profile: str | None
    music_level_db: float
    duck_level_db: float
    keep_natural_sound: bool


@dataclass(frozen=True, slots=True)
class GraphicsStyle:
    title_template_id: str
    ingredients_template_id: str
    subtitle_template_id: str


@dataclass(frozen=True, slots=True)
class StyleProfile:
    schema_version: str
    id: str
    name: str
    description: str
    editing: EditingStyle
    audio: AudioStyle
    graphics: GraphicsStyle
    extensions: Mapping[str, Any]

    @classmethod
    def from_dict(cls, raw: Mapping[str, Any]) -> "StyleProfile":
        editing = raw["editing"]
        audio = raw["audio"]
        graphics = raw["graphics"]
        known = {"schema_version", "id", "name", "description", "editing", "audio", "graphics"}
        return cls(
            schema_version=str(raw["schema_version"]),
            id=str(raw["id"]),
            name=str(raw["name"]),
            description=str(raw.get("description", "")),
            editing=EditingStyle(
                shot_min_seconds=float(editing["shot_min_seconds"]),
                shot_max_seconds=float(editing["shot_max_seconds"]),
                default_transition=str(editing["default_transition"]),
                avoid_jump_cuts=bool(editing["avoid_jump_cuts"]),
            ),
            audio=AudioStyle(
                music_profile=audio.get("music_profile"),
                music_level_db=float(audio["music_level_db"]),
                duck_level_db=float(audio["duck_level_db"]),
                keep_natural_sound=bool(audio.get("keep_natural_sound", True)),
            ),
            graphics=GraphicsStyle(
                title_template_id=str(graphics["title_template_id"]),
                ingredients_template_id=str(graphics["ingredients_template_id"]),
                subtitle_template_id=str(graphics["subtitle_template_id"]),
            ),
            extensions={key: value for key, value in raw.items() if key not in known},
        )

