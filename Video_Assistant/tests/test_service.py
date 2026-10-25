from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from video_assistant.service import VideoAssistantService

ROOT = Path(__file__).resolve().parents[1]


class ServiceWorkflowTests(unittest.TestCase):
    def test_api_service_runs_full_local_workflow(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            service = VideoAssistantService(Path(directory) / "service.db")
            try:
                project = service.create_project(
                    {
                        "name": "Endives",
                        "style_id": "crossroads_classic",
                        "delivery": {"platform": "youtube", "width": 1920, "height": 1080, "fps": "25"},
                    }
                )
                generated = service.generate_storyboard(
                    project["id"],
                    {
                        "scenario": "INGREDIENTS\n- 4 endives\n- 4 tranches de jambon\n\nETAPE 1 - BECHAMEL\nFaites fondre le beurre et ajoutez la farine.",
                    },
                )
                analyzed = service.analyze_paths(
                    [str(ROOT / "artifacts" / "poc" / "resolve-capability.mp4")]
                )
                matches = service.match_media(
                    project["id"],
                    {"segments": [item["segment"] for item in analyzed]},
                )
                style = json.loads(
                    (ROOT / "examples" / "styles" / "crossroads-classic.json").read_text(encoding="utf-8")
                )
                service.choose_style(project["id"], style)
                plan = service.compile_plan(project["id"])

                self.assertGreaterEqual(len(generated["storyboard"]["scenes"]), 2)
                self.assertEqual(analyzed[0]["analysis"]["codec_video"], "h264")
                self.assertGreaterEqual(len(matches["matches"]), 2)
                self.assertGreaterEqual(len(plan["edit_plan"]["operations"]), 3)
                self.assertEqual(plan["version"], 1)
            finally:
                service.close()


if __name__ == "__main__":
    unittest.main()

