from __future__ import annotations

import unittest

from video_assistant.ir import DurationRange, SelectionRequest, ShotType, VisualNeed
from video_assistant.matching import ExplainableMediaMatcher, IndexSegment


class MatchingTests(unittest.TestCase):
    def setUp(self) -> None:
        self.request = SelectionRequest(
            request_id="bechamel-montage",
            purpose="Montrer Michel couper une endive",
            need=VisualNeed(
                description="Michel coupe les endives",
                search_terms=("endive", "couteau", "preparation"),
                preferred_shots=(ShotType.CLOSE_UP,),
                shot_duration=DurationRange(3, 4),
                required_elements=("endive",),
            ),
            min_candidates=2,
        )

    def test_ranks_explained_candidates_by_deterministic_score(self) -> None:
        catalog = (
            IndexSegment("weak", 0, 8, keywords=("cuisine",), shot_type="wide", quality=0.4),
            IndexSegment("good", 12, 17, "Michel coupe l'endive au couteau", ("endive", "preparation"), "close_up", 0.9),
            IndexSegment(
                "best",
                40,
                45,
                "Michel coupe les endives au couteau en preparation",
                ("endive", "preparation"),
                "close_up",
                0.96,
            ),
        )
        result = ExplainableMediaMatcher().find((self.request,), catalog)[0]
        self.assertIsNotNone(result.selected)
        assert result.selected is not None
        self.assertEqual(result.selected.asset_id, "best")
        self.assertEqual(result.alternatives[0].asset_id, "good")
        self.assertIn("required_elements", result.selected.score_breakdown)
        self.assertIn("Matched", result.selected.explanation)

    def test_required_element_mismatch_cannot_be_selected(self) -> None:
        catalog = (IndexSegment("wrong", 0, 8, "personne dans la cuisine", quality=0.99),)
        result = ExplainableMediaMatcher().find((self.request,), catalog)[0]
        self.assertIsNone(result.selected)

    def test_segment_must_have_positive_duration(self) -> None:
        catalog = (IndexSegment("empty", 5, 5, "endive", ("endive",)),)
        result = ExplainableMediaMatcher().find((self.request,), catalog)[0]
        self.assertIsNone(result.selected)
        self.assertEqual(result.alternatives, ())


if __name__ == "__main__":
    unittest.main()

