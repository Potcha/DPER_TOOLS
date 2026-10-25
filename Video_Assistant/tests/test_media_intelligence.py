from __future__ import annotations

import json
import shutil
import subprocess
import unittest
from pathlib import Path

from video_assistant.contracts import MediaAsset
from video_assistant.errors import MediaIntelligenceError
from video_assistant.media_intelligence import (
    FfprobeAnalyzer,
    parse_ffprobe,
    parse_scene_change_times,
    parse_silencedetect,
)

ROOT = Path(__file__).resolve().parents[1]


class MediaIntelligenceParsingTests(unittest.TestCase):
    def test_parses_deterministic_ffprobe_metadata(self) -> None:
        raw = {
            "format": {"duration": "4.000000", "size": "12345", "start_time": "0"},
            "streams": [
                {"codec_type": "video", "codec_name": "h264", "width": 1280, "height": 720, "avg_frame_rate": "25/1"},
                {"codec_type": "audio", "codec_name": "aac", "channels": 2, "sample_rate": "48000"},
            ],
        }
        analysis = parse_ffprobe(asset_id="asset", path="clip.mp4", raw=raw)
        self.assertEqual(analysis.kind, "video")
        self.assertEqual(analysis.fps, "25/1")
        self.assertEqual(analysis.width, 1280)
        self.assertEqual(analysis.sample_rate, 48000)

    def test_parses_silence_and_scene_outputs(self) -> None:
        silence = """
[silencedetect @ 0] silence_start: 0.5
[silencedetect @ 0] silence_end: 1.3 | silence_duration: 0.8
[silencedetect @ 0] silence_start: 3.0
[silencedetect @ 0] silence_end: 3.6 | silence_duration: 0.6
"""
        intervals = parse_silencedetect(silence)
        self.assertEqual([(item.start_seconds, item.end_seconds) for item in intervals], [(0.5, 1.3), (3.0, 3.6)])
        self.assertEqual(parse_scene_change_times("pts_time:1.2\npts_time:2.75\n"), (1.2, 2.75))

    def test_missing_media_is_actionable(self) -> None:
        analyzer = FfprobeAnalyzer(runner=lambda _: (_ for _ in ()).throw(AssertionError("must not run")))
        asset = MediaAsset("missing", str(ROOT / "does-not-exist.mp4"), "video", None, {})
        with self.assertRaises(MediaIntelligenceError) as context:
            analyzer.analyze(asset)
        self.assertIn("missing", str(context.exception).casefold())


@unittest.skipUnless(shutil.which("ffprobe") and shutil.which("ffmpeg"), "FFmpeg tools are unavailable")
class MediaIntelligenceIntegrationTests(unittest.TestCase):
    def test_real_ffmpeg_analysis_on_generated_probe_media(self) -> None:
        path = ROOT / "artifacts" / "poc" / "resolve-capability.mp4"
        asset = MediaAsset("probe", str(path), "video", 4.0, {})
        analyzer = FfprobeAnalyzer()
        analysis = analyzer.analyze(asset)
        self.assertEqual(analysis["codec_video"], "h264")
        self.assertEqual(analysis["fps"], "25/1")
        self.assertEqual(analysis["width"], 1280)
        self.assertIsInstance(analyzer.detect_silences(asset), tuple)
        self.assertIsInstance(analyzer.detect_scene_changes(asset), tuple)


if __name__ == "__main__":
    unittest.main()

