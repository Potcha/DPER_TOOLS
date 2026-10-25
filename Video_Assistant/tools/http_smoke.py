from __future__ import annotations

import json
import urllib.request
from pathlib import Path
from typing import Any


BASE = "http://127.0.0.1:8766"


def request(path: str, payload: dict[str, Any] | None = None) -> dict[str, Any]:
    data = json.dumps(payload, ensure_ascii=False).encode("utf-8") if payload is not None else None
    req = urllib.request.Request(
        BASE + path,
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST" if payload is not None else "GET",
    )
    with urllib.request.urlopen(req) as response:
        return json.loads(response.read().decode("utf-8"))


def main() -> None:
    project = request(
        "/api/projects",
        {
            "name": "HTTP Smoke Endives",
            "style_id": "crossroads_classic",
            "delivery": {"platform": "youtube", "width": 1920, "height": 1080, "fps": "25"},
        },
    )
    storyboard = request(
        f"/api/projects/{project['id']}/storyboard",
        {
            "scenario": "INGREDIENTS\n- 4 endives\n\nETAPE 1 - BECHAMEL\nFaites fondre le beurre et ajoutez la farine.",
        },
    )
    analyzed = request(
        "/api/media/analyze",
        {"paths": [str(Path("artifacts/poc/resolve-capability.mp4").resolve())]},
    )
    matches = request(
        f"/api/projects/{project['id']}/match",
        {"segments": [item["segment"] for item in analyzed["items"]]},
    )
    request(f"/api/projects/{project['id']}/style", {"style": json.loads(Path("examples/styles/crossroads-classic.json").read_text(encoding="utf-8"))})
    plan = request(f"/api/projects/{project['id']}/compile", {})
    print(
        json.dumps(
            {
                "project_id": project["id"],
                "storyboard_scenes": len(storyboard["storyboard"]["scenes"]),
                "analyzed_assets": len(analyzed["items"]),
                "match_results": len(matches["matches"]),
                "edit_operations": len(plan["edit_plan"]["operations"]),
                "edit_plan_version": plan["version"],
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()

