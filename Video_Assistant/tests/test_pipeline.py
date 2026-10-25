from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from video_assistant.matching import IndexSegment
from video_assistant.pipeline import ProductionPipeline
from video_assistant.storage import SqliteStore
from video_assistant.style import StyleProfile


class PipelineTests(unittest.TestCase):
    def test_runs_scenario_storyboard_match_compile_and_versioning(self) -> None:
        scenario = """
INGREDIENTS
- 2 citrons
- 100 g de sucre

ETAPE 1 - CUISSON
Faites cuire la préparation pendant dix minutes.

DEGUSTATION
Servez la tarte bien fraîche.
"""
        style = StyleProfile(
            schema_version="1.0.0",
            id="crossroads_classic",
            name="Crossroads Classic",
            description="",
            editing=__import__("video_assistant.style", fromlist=["EditingStyle"]).EditingStyle(2, 6, "cut", True),
            audio=__import__("video_assistant.style", fromlist=["AudioStyle"]).AudioStyle("soft", -20, -27, True),
            graphics=__import__("video_assistant.style", fromlist=["GraphicsStyle"]).GraphicsStyle(
                "crossroads_title_v1", "crossroads_ingredients_v2", "crossroads_subtitle_v1"
            ),
            extensions={},
        )
        with tempfile.TemporaryDirectory() as directory:
            with SqliteStore(Path(directory) / "project.db") as store:
                pipeline = ProductionPipeline(store)
                project = pipeline.create_project("Tarte citron")
                storyboard, storyboard_version = pipeline.generate_storyboard(
                    project.id,
                    scenario,
                    {"id": "tarte-001", "title": "Tarte citron", "style_id": style.id},
                )
                segments = tuple(
                    IndexSegment(
                        asset_id=f"asset-{index}",
                        start_seconds=0,
                        end_seconds=scene.editing.target_seconds + 2,
                        text=scene.purpose,
                        keywords=scene.visual.search_terms if scene.visual else (),
                        shot_type="close_up",
                        quality=0.9,
                    )
                    for index, scene in enumerate(storyboard.scenes)
                )
                matches, match_version = pipeline.match_storyboard(project.id, storyboard, segments)
                result, plan_version = pipeline.compile_plan(project.id, storyboard, matches, style)

                self.assertEqual((storyboard_version.version, match_version.version, plan_version.version), (1, 1, 1))
                self.assertEqual(store.latest_document(project.id, "edit_plan").version, 1)
                self.assertGreaterEqual(len(matches), 2)
                self.assertGreaterEqual(len(result.document.operations), 3)


if __name__ == "__main__":
    unittest.main()

