from __future__ import annotations

import unittest

from video_assistant.director import RecipeStoryboardDirector


class RecipeDirectorTests(unittest.TestCase):
    def test_builds_valid_storyboard_from_real_recipe_sections(self) -> None:
        scenario = """
INTRO
Aujourd'hui, nous préparons une tarte au citron très fraîche.

INGREDIENTS
- 200 g de farine
- 100 g de beurre
- 3 citrons

ETAPE 1 - PATE
Faites sabler la farine avec le beurre puis ajoutez l'eau froide.

ETAPE 2 - CUISSON
Étalez la pâte et faites-la cuire à 180 degrés pendant 20 minutes.

DEGUSTATION
La texture doit être fondante et l'équilibre parfait.
"""
        document = RecipeStoryboardDirector().generate(
            scenario,
            {"id": "tarte-001", "title": "Tarte au citron", "style_id": "crossroads_classic"},
        )
        self.assertGreaterEqual(len(document.scenes), 5)
        self.assertEqual(document.scenes[1].intent, "ingredients")
        self.assertEqual(document.scenes[1].graphics[0].kind, "ingredient_list")
        self.assertEqual(document.scenes[-1].intent, "tasting")

    def test_unstructured_scenario_still_produces_a_scene(self) -> None:
        document = RecipeStoryboardDirector().generate(
            "Michel prépare un plat simple et savoureux.",
            {"id": "fallback", "title": "Plat"},
        )
        self.assertEqual(len(document.scenes), 1)
        self.assertTrue(document.scenes[0].selections)


if __name__ == "__main__":
    unittest.main()

