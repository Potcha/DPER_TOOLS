"""Reversible DaVinci Resolve scripting capability probe.

Run this through Resolve's external Python or MCP bridge with ``resolve`` already
connected. It creates and removes an isolated project and never edits a user
project. The returned JSON is the evidence source for the capability report.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any


def run_probe(resolve: Any, media_path: str) -> dict[str, Any]:
    result: dict[str, Any] = {"resolve_version": None, "checks": [], "cleanup": {}}

    def check(name: str, call: str, value: Any) -> Any:
        result["checks"].append({"name": name, "call": call, "supported": value is not None and value is not False, "observed": value})
        return value

    manager = resolve.GetProjectManager()
    previous = manager.GetCurrentProject()
    previous_name = previous.GetName() if previous else None
    result["resolve_version"] = resolve.GetVersionString()
    project_name = "_VA_POC_21_1"

    try:
        if previous:
            manager.SaveProject()
        if project_name in manager.GetProjectListInCurrentFolder():
            manager.LoadProject(project_name)
            project = manager.GetCurrentProject()
        else:
            project = manager.CreateProject(project_name)
        check("project_create", "ProjectManager.CreateProject", project.GetName() if project else None)
        if project is None:
            return result

        pool = project.GetMediaPool()
        imported = check(
            "media_import",
            "MediaPool.ImportMedia",
            pool.ImportMedia([{"FilePath": media_path}]),
        )
        if not imported:
            return result
        media = imported[0]

        timeline = check("timeline_create", "MediaPool.CreateEmptyTimeline", pool.CreateEmptyTimeline("POC Timeline"))
        if timeline is None:
            return result
        check("track_create", "Timeline.AddTrack('audio')", timeline.AddTrack("audio"))

        clips = check(
            "clip_insert_trim_position",
            "MediaPool.AppendToTimeline(startFrame,endFrame,recordFrame,trackIndex)",
            pool.AppendToTimeline(
                [
                    {"mediaPoolItem": media, "startFrame": 5, "endFrame": 54, "recordFrame": 0, "trackIndex": 1},
                    {"mediaPoolItem": media, "startFrame": 25, "endFrame": 74, "recordFrame": 50, "trackIndex": 1},
                ]
            ),
        )
        if len(clips) != 2:
            return result

        item_shape = [
            {
                "start": item.GetStart(),
                "end": item.GetEnd(),
                "duration": item.GetDuration(),
                "source_start": item.GetSourceStartFrame(),
                "source_end": item.GetSourceEndFrame(),
            }
            for item in clips
        ]
        check("clip_geometry", "TimelineItem.GetStart/End/SourceStartFrame/SourceEndFrame", item_shape)

        title = check("title_insert", "Timeline.InsertTitleIntoTimeline('Text')", timeline.InsertTitleIntoTimeline("Text"))
        title_text = None
        if title:
            comps = title.GetFusionCompCount()
            comp = title.GetFusionCompByIndex(1) if comps else title.AddFusionComp()
            text_tool = comp.GetToolList(False) if comp and hasattr(comp, "GetToolList") else {}
            text_plus = next((tool for tool in text_tool.values() if tool.ID == "TextPlus"), None) if text_tool else None
            if text_plus is None and comp:
                text_plus = comp.AddTool("TextPlus")
            if text_plus:
                text_plus.SetInput("StyledText", "POC TITRE", 0)
                title_text = text_plus.GetInput("StyledText", 0)
            title.SetName("POC TITRE")
        check("title_text", "FusionComp.AddTool('TextPlus').SetInput('StyledText')", title_text)

        transition = clips[0].AddTransition(
            {"type": "Cross Dissolve", "category": "simple", "position": "end", "alignment": "center", "duration": 8}
        )
        check("transition_add", "TimelineItem.AddTransition(Cross Dissolve)", transition.GetType() if transition else None)

        volume = check(
            "audio_volume",
            "TimelineItem.SetProperties(AudioVolumeEnabled, AudioVolume)",
            clips[1].SetProperties({"AudioVolumeEnabled": True, "AudioVolume": -12.0}),
        )
        check("audio_volume_readback", "TimelineItem.GetProperties", clips[1].GetProperties())
        check("fade", "TimelineItem.SetFades", clips[1].SetFades({"FadeIn": 5, "FadeOut": 5}))
        check("timeline_state", "Timeline.GetTrackCount/GetItemListInTrack", {"video_tracks": timeline.GetTrackCount("video"), "audio_tracks": timeline.GetTrackCount("audio"), "video_items": len(timeline.GetItemListInTrack("video", 1))})
    finally:
        if previous_name:
            restored = manager.LoadProject(previous_name)
            result["cleanup"]["previous_project_restored"] = bool(restored)
        if project_name in manager.GetProjectListInCurrentFolder():
            loaded = manager.GetCurrentProject()
            if loaded and loaded.GetName() != project_name:
                result["cleanup"]["poc_project_deleted"] = manager.DeleteProject(project_name)
    return result


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: davinci_resolve_poc.py MEDIA_PATH", file=sys.stderr)
        return 2
    try:
        import DaVinciResolveScript as dvr
    except ImportError:
        print("DaVinciResolveScript is not available. Run from Resolve's scripting Python.", file=sys.stderr)
        return 3
    resolve = dvr.scriptapp("Resolve")
    if resolve is None:
        print("DaVinci Resolve is not running or scripting is disabled.", file=sys.stderr)
        return 4
    print(json.dumps(run_probe(resolve, str(Path(sys.argv[1]).resolve())), ensure_ascii=False, indent=2, default=str))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

