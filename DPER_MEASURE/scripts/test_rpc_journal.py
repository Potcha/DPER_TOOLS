import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock


SCRIPT = Path(__file__).with_name("smoke_inspection_mcp.py")
SPEC = importlib.util.spec_from_file_location("smoke_inspection_mcp", SCRIPT)
smoke = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(smoke)


class FakeResponse:
    headers = {}

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        return False

    def read(self):
        return json.dumps({"jsonrpc": "2.0", "id": 1, "result": {"ok": True}}).encode()


class RpcJournalRetryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=SCRIPT.parent)
        self.addCleanup(self.temp.cleanup)
        folder = Path(self.temp.name)
        self.client = smoke.Mcp.__new__(smoke.Mcp)
        self.client.folder = folder
        self.client.session = None
        self.client.counter = 0
        self.client.log = []
        self.client.port = 18765
        self.client.log_path = folder / "rpc_evidence_001.json"

    def test_transient_replace_error_retries_publication_without_repeating_rpc(self):
        original_replace = smoke.os.replace
        attempts = 0

        def replace_with_two_transient_failures(source, destination):
            nonlocal attempts
            attempts += 1
            if attempts <= 2:
                raise PermissionError("temporary destination lock")
            original_replace(source, destination)

        with mock.patch.object(smoke.urllib.request, "urlopen", return_value=FakeResponse()) as urlopen, \
                mock.patch.object(smoke.os, "replace", side_effect=replace_with_two_transient_failures), \
                mock.patch("time.sleep"):
            result = self.client.rpc("tools/list", {})

        self.assertEqual(result, {"ok": True})
        self.assertEqual(urlopen.call_count, 1)
        self.assertEqual(attempts, 3)
        self.assertTrue(self.client.log_path.is_file())

    def test_persistent_replace_error_fails_after_bounded_retries_without_repeating_rpc(self):
        with mock.patch.object(smoke.urllib.request, "urlopen", return_value=FakeResponse()) as urlopen, \
                mock.patch.object(smoke.os, "replace", side_effect=PermissionError("persistent lock")) as replace, \
                mock.patch("time.sleep"):
            with self.assertRaises(PermissionError):
                self.client.rpc("tools/list", {})

        self.assertEqual(urlopen.call_count, 1)
        self.assertEqual(replace.call_count, 6)
        self.assertFalse(self.client.log_path.exists())


if __name__ == "__main__":
    unittest.main()
