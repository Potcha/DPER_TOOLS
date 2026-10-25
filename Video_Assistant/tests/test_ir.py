from __future__ import annotations

import copy
import json
import unittest
from pathlib import Path

from video_assistant.errors import DocumentValidationError
from video_assistant.ir import VideoIRDocument

ROOT = Path(__file__).resolve().parents[1]


class VideoIRTests(unittest.TestCase):
    def test_all_real_examples_validate(self) -> None:
        for path in sorted((ROOT / "examples" / "video-ir").glob("*.json")):
            with self.subTest(path=path.name):
                document = VideoIRDocument.from_dict(json.loads(path.read_text(encoding="utf-8")))
                self.assertGreaterEqual(len(document.scenes), 2)

    def test_round_trip_preserves_serializable_model(self) -> None:
        raw = json.loads((ROOT / "examples" / "video-ir" / "endives-au-jambon.json").read_text(encoding="utf-8"))
        document = VideoIRDocument.from_dict(raw)
        self.assertEqual(document.project.id, "crossroads-endives-001")
        self.assertEqual(len(document.scenes), 3)

    def test_reports_actionable_structural_errors(self) -> None:
        raw = {"schema_version": "0.9.0", "project": {}, "scenes": []}
        with self.assertRaises(DocumentValidationError) as context:
            VideoIRDocument.from_dict(raw)
        codes = {error.code for error in context.exception.errors}
        self.assertIn("schema_version", codes)
        self.assertIn("required_string", codes)
        self.assertIn("scenes_empty", codes)

    def test_duplicate_scene_ids_are_rejected(self) -> None:
        path = ROOT / "examples" / "video-ir" / "tarte-citron-shorts.json"
        raw = copy.deepcopy(json.loads(path.read_text(encoding="utf-8")))
        raw["scenes"][1]["id"] = raw["scenes"][0]["id"]
        with self.assertRaises(DocumentValidationError) as context:
            VideoIRDocument.from_dict(raw)
        self.assertIn("scene_id_duplicate", {error.code for error in context.exception.errors})


if __name__ == "__main__":
    unittest.main()

