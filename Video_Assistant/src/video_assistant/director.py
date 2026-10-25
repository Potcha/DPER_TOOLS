"""AI Director boundary and deterministic recipe fallback generator."""

from __future__ import annotations

import re
import unicodedata
from dataclasses import replace
from typing import Any, Mapping, Sequence

from .contracts import LLMProvider, LLMRequest, Message, StoryboardGenerator
from .errors import DocumentValidationError
from .ir import (
    AudioSpec,
    ConstraintSpec,
    DecisionRecord,
    EditingSpec,
    GraphicSpec,
    NarrativeIntent,
    NarrationSpec,
    ProjectSpec,
    Provenance,
    Requirement,
    Scene,
    SelectionRequest,
    ShotType,
    TransitionSpec,
    VideoIRDocument,
    VisualNeed,
)


class LLMStoryboardDirector:
    """Uses a provider only for semantic decomposition; validation stays deterministic."""

    def __init__(self, provider: LLMProvider) -> None:
        self.provider = provider

    def generate(self, scenario: str, project: Mapping[str, Any]) -> VideoIRDocument:
        request = LLMRequest(
            task="scenario_to_video_ir",
            messages=(
                Message("system", "Produce an executable storyboard as JSON. Never emit executable NLE commands."),
                Message("user", f"PROJECT\n{project!r}\n\nSCENARIO\n{scenario}"),
            ),
            response_schema=_video_ir_response_schema(),
            temperature=0.2,
        )
        raw = self.provider.complete_json(request)
        try:
            return VideoIRDocument.from_dict(raw)
        except DocumentValidationError:
            raise


class RecipeStoryboardDirector:
    """Offline fallback for cooking scripts with explicit recipe sections."""

    _heading = re.compile(
        r"^(INTRO|ACCROCHE|PLAT[ -]?TERMINE|PRESENTATION|INGREDIENTS|ETAPE[ -]?([0-9]+)|CUISSON|DRESSAGE|DEGUSTATION|OUTRO)\s*[:\-]?\s*(.*)$",
        re.IGNORECASE,
    )

    def generate(self, scenario: str, project: Mapping[str, Any]) -> VideoIRDocument:
        project_spec = self._project_spec(project)
        blocks = self._blocks(scenario)
        if not blocks:
            blocks = [("presentation", "", scenario.strip() or "Scénario vide")]
        scenes = tuple(self._scene(index, heading, note, body, project_spec) for index, (heading, note, body) in enumerate(blocks))
        return VideoIRDocument.from_dict(
            {
                "schema_version": "1.0.0",
                "project": {
                    "id": project_spec.id,
                    "title": project_spec.title,
                    "style_id": project_spec.style_id,
                    "delivery": {
                        "platform": project_spec.delivery.platform,
                        "width": project_spec.delivery.width,
                        "height": project_spec.delivery.height,
                        "fps": project_spec.delivery.fps,
                        "target_duration_seconds": project_spec.delivery.target_duration_seconds,
                    },
                },
                "scenes": [
                    {
                        "id": scene.id,
                        "title": scene.title,
                        "purpose": scene.purpose,
                        "intent": scene.intent.value,
                        "requirement": scene.requirement.value,
                        "editing": {
                            "target_seconds": scene.editing.target_seconds,
                            "tolerance_seconds": scene.editing.tolerance_seconds,
                            "pace": scene.editing.pace,
                            "transitions": [
                                {"kind": transition.kind, "duration_frames": transition.duration_frames}
                                for transition in scene.editing.transitions
                            ],
                        },
                        "narration": {
                            "text": scene.narration.text,
                            "keep_complete_sentences": scene.narration.keep_complete_sentences,
                        }
                        if scene.narration
                        else None,
                        "visual": {
                            "description": scene.visual.description,
                            "search_terms": list(scene.visual.search_terms),
                            "preferred_shots": [shot.value for shot in scene.visual.preferred_shots],
                        }
                        if scene.visual
                        else None,
                        "selections": [
                            {
                                "request_id": selection.request_id,
                                "purpose": selection.purpose,
                                "need": {
                                    "description": selection.need.description,
                                    "search_terms": list(selection.need.search_terms),
                                    "preferred_shots": [shot.value for shot in selection.need.preferred_shots],
                                },
                            }
                            for selection in scene.selections
                        ],
                        "graphics": [
                            {
                                "kind": graphic.kind,
                                "text": graphic.text,
                                "requirement": graphic.requirement.value,
                                "start_seconds": graphic.start_seconds,
                                "duration_seconds": graphic.duration_seconds,
                            }
                            for graphic in scene.graphics
                        ],
                    }
                    for scene in scenes
                ],
            }
        )

    @staticmethod
    def _project_spec(project: Mapping[str, Any]) -> ProjectSpec:
        delivery = project.get("delivery", {})
        return ProjectSpec(
            id=str(project.get("id", "recipe-project")),
            title=str(project.get("title", "Recette")),
            style_id=str(project.get("style_id", "crossroads_classic")),
            delivery=RecipeStoryboardDirector._delivery_from_mapping(delivery),
        )

    @staticmethod
    def _delivery_from_mapping(delivery: Mapping[str, Any]) -> Any:
        from .ir import DeliverySpec

        return DeliverySpec(
            platform=str(delivery.get("platform", "youtube")),
            width=int(delivery.get("width", 1920)),
            height=int(delivery.get("height", 1080)),
            fps=str(delivery.get("fps", "25")),
            target_duration_seconds=delivery.get("target_duration_seconds"),
        )

    @staticmethod
    def _normalize(value: str) -> str:
        normalized = unicodedata.normalize("NFKD", value.casefold())
        return "".join(character for character in normalized if not unicodedata.combining(character))

    def _blocks(self, scenario: str) -> list[tuple[str, str, str]]:
        blocks: list[tuple[str, str, str]] = []
        current: list[str] = []
        heading = "presentation"
        note = ""
        for line in scenario.splitlines():
            match = self._heading.match(self._normalize(line).strip())
            if match:
                if current:
                    blocks.append((heading, note, "\n".join(current).strip()))
                    current = []
                heading = self._normalize(match.group(1))
                note = match.group(3).strip()
            elif line.strip():
                current.append(line.strip())
        if current:
            blocks.append((heading, note, "\n".join(current).strip()))
        return [block for block in blocks if block[2]]

    def _scene(
        self,
        index: int,
        heading: str,
        note: str,
        body: str,
        project: ProjectSpec,
    ) -> Scene:
        intent = self._intent(heading)
        scene_id = f"scene-{index + 1:02d}-{_slug(heading or intent.value)}"
        word_count = max(1, len(body.split()))
        target = min(45.0, max(5.0, word_count / 2.5))
        search_terms = tuple(list(dict.fromkeys(_slug(word) for word in body.split() if len(word) > 3))[:8])
        shot = ShotType.OVERHEAD if intent == NarrativeIntent.INGREDIENTS else ShotType.CLOSE_UP
        need = VisualNeed(
            description=note or f"Illustrer: {body[:100]}",
            search_terms=search_terms,
            preferred_shots=(shot,),
        )
        graphics: list[GraphicSpec] = []
        if intent == NarrativeIntent.INGREDIENTS:
            items = [line.strip("-• ") for line in body.splitlines() if line.strip()]
            if items:
                graphics.append(
                    GraphicSpec(
                        kind="ingredient_list",
                        text="\n".join(items),
                        requirement=Requirement.RECOMMENDED,
                        start_seconds=1,
                        duration_seconds=max(5.0, min(target - 1, 10.0)),
                        template_id="crossroads_ingredients_v2",
                    )
                )
        return Scene(
            id=scene_id,
            title=heading.replace("-", " ").title() or "Scene",
            purpose=note or f"Rendre visible: {body[:100]}",
            intent=intent,
            requirement=Requirement.REQUIRED if intent in {NarrativeIntent.INTRO, NarrativeIntent.INGREDIENTS} else Requirement.RECOMMENDED,
            editing=EditingSpec(
                target_seconds=round(target, 1),
                tolerance_seconds=2.0,
                transitions=(TransitionSpec(kind="cross_dissolve" if intent == NarrativeIntent.RESULT else "cut"),),
                pace="energetic" if intent in {NarrativeIntent.INTRO, NarrativeIntent.HOOK} else "steady",
            ),
            narration=NarrationSpec(text=body, keep_complete_sentences=True),
            visual=need,
            selections=(
                SelectionRequest(
                    request_id=f"{scene_id}-visual",
                    purpose=note or f"Plan illustrant {heading}",
                    need=need,
                    min_candidates=1,
                    fallback="Utiliser un plan moyen explicatif",
                ),
            ),
            graphics=tuple(graphics),
            audio=AudioSpec(
                voice_priority=True,
                keep_natural_sound=True,
            ),
            constraints=ConstraintSpec(
                mandatory_elements=("Ne jamais couper une phrase",) if project else (),
                fallbacks=("Conserver un plan large",),
            ),
            decisions=(
                DecisionRecord(
                    id=f"{scene_id}-fallback",
                    decision="Scene determinee par une section explicite du scenario",
                    provenance=Provenance.DEFAULT,
                    reason="Generateur deterministe de secours, aucun fournisseur IA disponible",
                    confidence=0.5,
                ),
            ),
        )

    @staticmethod
    def _intent(heading: str) -> NarrativeIntent:
        if heading.startswith("intro") or heading.startswith("accroche"):
            return NarrativeIntent.INTRO
        if "ingredient" in heading:
            return NarrativeIntent.INGREDIENTS
        if heading.startswith("plat") or heading.startswith("dressage"):
            return NarrativeIntent.RESULT
        if heading.startswith("degustation"):
            return NarrativeIntent.TASTING
        if heading.startswith("outro"):
            return NarrativeIntent.OUTRO
        return NarrativeIntent.INSTRUCTION


def _slug(value: str) -> str:
    normalized = unicodedata.normalize("NFKD", value.casefold())
    return re.sub(r"[^a-z0-9]+", "-", "".join(char for char in normalized if not unicodedata.combining(char))).strip("-")


def _video_ir_response_schema() -> Mapping[str, Any]:
    return {
        "type": "object",
        "required": ["schema_version", "project", "scenes"],
        "properties": {
            "schema_version": {"const": "1.0.0"},
            "project": {"type": "object"},
            "scenes": {"type": "array"},
        },
    }

