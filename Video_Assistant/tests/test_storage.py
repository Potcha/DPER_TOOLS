from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from video_assistant.storage import SqliteStore


class StorageTests(unittest.TestCase):
    def test_versions_are_immutable_and_ordered(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            with SqliteStore(Path(directory) / "project.db") as store:
                project = store.create_project("Crossroads")
                first = store.save_document(project.id, "storyboard", {"scenes": []}, actor="user", summary="Initial")
                second = store.save_document(
                    project.id,
                    "storyboard",
                    {"scenes": [{"id": "intro"}]},
                    actor="user",
                    summary="Add intro",
                    parent_version=1,
                )
                self.assertEqual((first.version, second.version), (1, 2))
                self.assertEqual([item.version for item in store.list_documents(project.id, "storyboard")], [1, 2])
                self.assertEqual(store.latest_document(project.id, "storyboard").version, 2)

    def test_optimistic_parent_version_is_required(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            with SqliteStore(Path(directory) / "project.db") as store:
                project = store.create_project("Crossroads")
                store.save_document(project.id, "edit_plan", {"operations": []}, actor="compiler", summary="Compile")
                with self.assertRaises(ValueError):
                    store.save_document(
                        project.id,
                        "edit_plan",
                        {"operations": [1]},
                        actor="user",
                        summary="Change",
                        parent_version=3,
                    )

    def test_job_progress_and_cancel_are_persisted(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            with SqliteStore(Path(directory) / "project.db") as store:
                project = store.create_project("Crossroads")
                job = store.create_job(project.id, "analyze_media", {"asset_id": "asset-1"})
                updated = store.update_job(job.id, status="running", progress=0.5, message="Scene detection")
                self.assertTrue(store.request_job_cancel(job.id))
                persisted = store.get_job(job.id)
                self.assertEqual(updated.status, "running")
                self.assertIsNotNone(persisted)
                assert persisted is not None
                self.assertTrue(persisted.cancel_requested)
                store.update_job(job.id, status="cancelled", progress=0.5, message="Cancelled by user")
                self.assertFalse(store.request_job_cancel(job.id))


if __name__ == "__main__":
    unittest.main()

