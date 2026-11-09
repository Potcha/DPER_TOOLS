"""Generate a synthetic corner and optionally exercise an owned MCP test host."""
import argparse
import math
from pathlib import Path
import sys

sys.dont_write_bytecode = True

from PIL import Image, ImageDraw
from smoke_inspection_mcp import Mcp


def search(start, end):
    return {"start": dict(zip(("x", "y"), start)), "end": dict(zip(("x", "y"), end))}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path)
    parser.add_argument("--test-port", type=int, help="Port of a dedicated, owned MCP test host")
    args = parser.parse_args()
    if args.test_port is not None and not 18765 <= args.test_port <= 65535:
        parser.error("Use an owned test port between 18765 and 65535.")
    folder = args.folder.resolve()
    folder.mkdir(parents=True, exist_ok=False)
    source = folder / "coin-synthetique.png"
    image = Image.new("RGB", (256, 256), (30, 30, 30))
    ImageDraw.Draw(image).rectangle((121, 131, 255, 255), fill=(220, 220, 220))
    image.save(source)
    print(source)
    if args.test_port is None:
        return

    client = Mcp(folder, args.test_port)
    assert "inspect_line_intersection" in {tool["name"] for tool in client.rpc("tools/list", {})["tools"]}
    before = client.call("open_image", {"path": str(source)})
    request = {
        "capture_id": before["capture_id"], "expected_revision": before["document_revision"],
        "searches": [search((185, 132), (123.5, 132)), search((119, 133.5), (119, 195))],
    }
    response = client.call("inspect_line_intersection", request)
    diagnostic = response["diagnostic"]
    point = diagnostic["intersection_px"]
    assert math.hypot(point["x"] - 120.5, point["y"] - 130.5) < 0.25, point
    assert abs(diagnostic["acute_angle_deg"] - 90.0) < 0.25
    assert response["capture_id"] == request["capture_id"]
    assert response["document_revision"] == request["expected_revision"]
    assert len(diagnostic["supports"]) == 2
    assert all(support["evidence"]["accepted_points"] and not support["evidence"]["endpoint_extent_verified"]
               for support in diagnostic["supports"])
    far = [search((185, 132), (140, 132)), request["searches"][1]]
    for refused, expected in (
        (dict(request, searches=far), "insuffisamment étayée"),
        (dict(request, capture_id="other-capture"), "La capture a changé"),
        (dict(request, expected_revision=request["expected_revision"] - 1), "révision actuelle"),
    ):
        rejection = client.call("inspect_line_intersection", refused, expect_error=True)
        message = " ".join(block.get("text", "") for block in rejection["content"])
        assert expected in message, rejection
    after = client.call("get_measurement_context")
    for field in ("capture_id", "document_revision", "measurements", "calibration"):
        assert after[field] == before[field], field
    print(f"Intersection ({point['x']:.4f}, {point['y']:.4f}) px. Trois refus vérifiés. Session inchangée.")


if __name__ == "__main__":
    main()
