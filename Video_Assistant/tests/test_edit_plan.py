from __future__ import annotations

import copy
import json
import unittest
from dataclasses import replace
from pathlib import Path

from video_assistant.edit_plan import EditDocument, EditOperation, OperationKind
from video_assistant.plan_validation import validate_edit_document

ROOT = Path(__file__).resolve().parents[1]


class EditPlanTests(unittest.TestCase):
    def load_example(self) -> EditDocument:
        raw = json.loads((ROOT / "examples" / "edit-plans" / "endives-partial.json").read_text(encoding="utf-8"))
        return EditDocument(
            schema_version=raw["schema_version"],
            project_id=raw["project_id"],
            timeline_name=raw["timeline_name"],
            fps=raw["fps"],
            operations=tuple(EditOperation(**operation) for operation in raw["operations"]),
        )

    def test_example_is_executable_shape(self) -> None:
        self.assertEqual(validate_edit_document(self.load_example()), ())

    def test_trim_must_reference_existing_media_operation(self) -> None:
        document = self.load_example()
        operations = list(document.operations)
        operations[1] = copy.deepcopy(operations[1])
        operations[1].params["operation_id"] = "unknown"
        broken = replace(document, operations=tuple(operations))
        self.assertIn("operation_reference", {error.code for error in validate_edit_document(broken)})

    def test_duplicate_operation_ids_are_rejected(self) -> None:
        document = self.load_example()
        operations = (*document.operations, document.operations[0])
        broken = replace(document, operations=operations)
        self.assertIn("operation_id_duplicate", {error.code for error in validate_edit_document(broken)})


if __name__ == "__main__":
    unittest.main()

