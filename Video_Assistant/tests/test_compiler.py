from __future__ import annotations

import json
import unittest
from pathlib import Path

from video_assistant.compiler import DeterministicEditCompiler, parse_fps, seconds_to_frames
from video_assistant.contracts import MediaCandidate, MatchResult
from video_assistant.edit_plan import OperationKind
from video_assistant.errors import CompilationError
from video_assistant.ir import VideoIRDocument
from video_assistant.plan_validation import validate_edit_document
from video_assistant.style import StyleProfile

ROOT = Path(__file__).resolve().parents[1]


class CompilerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.storyboard = VideoIRDocument.from_dict(
            json.loads((ROOT / "examples" / "video-ir" / "endives-au-jambon.json").read_text(encoding="utf-8"))
        )
        self.style = StyleProfile.from_dict(
            json.loads((ROOT / "examples" / "styles" / "crossroads-classic.json").read_text(encoding="utf-8"))
        )

    def matches(self) -> list[MatchResult]:
        results = []
        for scene in self.storyboard.scenes:
            request = scene.selections[0]
            candidate = MediaCandidate(
                asset_id=f"asset-{request.request_id}",
                source_in_seconds=0,
                source_out_seconds=scene.editing.target_seconds + 3,
                score=0.94,
                score_breakdown={"semantic": 0.95, "quality": 0.93},
                explanation=f"Best candidate for {request.purpose}",
            )
            results.append(MatchResult(request_id=request.request_id, selected=candidate, alternatives=()))
        return results

    def test_compiles_real_storyboard_into_portable_plan(self) -> None:
        result = DeterministicEditCompiler().compile(self.storyboard, self.matches(), self.style)
        self.assertEqual(result.warnings, ())
        self.assertEqual(validate_edit_document(result.document), ())
        kinds = [operation.kind for operation in result.document.operations]
        self.assertEqual(kinds.count(OperationKind.INSERT_CLIP), 3)
        self.assertIn(OperationKind.ADD_TITLE, kinds)
        self.assertIn(OperationKind.ADD_OVERLAY, kinds)
        self.assertIn(OperationKind.ADD_SUBTITLE, kinds)
        self.assertIn(OperationKind.ADD_MUSIC, kinds)
        self.assertIn(OperationKind.SET_VOLUME, kinds)

    def test_frame_calculations_are_exact(self) -> None:
        self.assertEqual(seconds_to_frames(1, parse_fps("25")), 25)
        self.assertEqual(seconds_to_frames(1, parse_fps("30000/1001")), 30)
        self.assertEqual(seconds_to_frames(2.5, parse_fps("25")), 63)

    def test_required_scene_without_match_fails_compilation(self) -> None:
        matches = self.matches()[1:]
        with self.assertRaises(CompilationError):
            DeterministicEditCompiler().compile(self.storyboard, matches, self.style)

    def test_short_candidate_is_compiled_with_warning(self) -> None:
        matches = self.matches()
        short = MediaCandidate("asset-short", 0, 4, 0.8, {"duration": 0.2}, "Only four seconds available")
        matches[2] = MatchResult("bechamel-montage", short, ())
        result = DeterministicEditCompiler().compile(self.storyboard, matches, self.style)
        self.assertIn("duration_short", {warning.code for warning in result.warnings})


if __name__ == "__main__":
    unittest.main()

