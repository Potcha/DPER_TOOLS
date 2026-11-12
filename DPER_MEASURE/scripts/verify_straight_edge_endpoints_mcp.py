"""Generate straight-edge fixtures and optionally verify an owned MCP test host."""
import argparse
import math
from pathlib import Path
import sys

sys.dont_write_bytecode = True

from PIL import Image, ImageDraw
from verify_line_intersection_mcp import Mcp, search


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path)
    parser.add_argument("--test-port", type=int, help="Port of a dedicated, owned MCP test host")
    args = parser.parse_args()
    if args.test_port is not None and not 18765 <= args.test_port <= 65535:
        parser.error("Use an owned test port between 18765 and 65535.")
    folder = args.folder.resolve()
    folder.mkdir(parents=True, exist_ok=False)
    source = folder / "bord-borne.png"
    continuation = folder / "bord-prolonge.png"
    masked = folder / "bord-masque.png"
    image = Image.new("RGB", (256, 256), (30, 30, 30))
    ImageDraw.Draw(image).rectangle((51, 91, 190, 160), fill=(220, 220, 220))
    image.save(source)
    masked_image = image.copy()
    ImageDraw.Draw(masked_image).rectangle((56, 87, 61, 96), fill=(30, 30, 30))
    masked_image.save(masked)
    drawing = ImageDraw.Draw(image)
    drawing.rectangle((0, 0, 50, 90), fill=(220, 220, 220))
    drawing.rectangle((191, 0, 255, 90), fill=(220, 220, 220))
    image.save(continuation)
    print(source)
    print(continuation)
    print(masked)
    if args.test_port is None:
        return

    client = Mcp(folder, args.test_port)
    request = {"search": {
        "edge": search((53.5, 92), (187.5, 92)),
        "start_boundary": search((49, 93.5), (49, 130)),
        "end_boundary": search((192, 93.5), (192, 130)),
    }}

    def open_fixture(path):
        context = client.call("open_image", {"path": str(path)})
        request.update(capture_id=context["capture_id"], expected_revision=context["document_revision"])
        return context

    def assert_unchanged(before):
        after = client.call("get_measurement_context")
        for field in ("capture_id", "document_revision", "measurements", "calibration"):
            assert after[field] == before[field], field

    before = open_fixture(source)
    result = client.call("inspect_straight_edge_endpoints", request)["diagnostic"]
    for side, x in (("start", 50.5), ("end", 190.5)):
        point = result[side]["intersection_px"]
        assert math.hypot(point["x"] - x, point["y"] - 90.5) < 0.25, point
    assert abs(result["length_px"] - 140) < 0.35
    assert not result["edge_support"]["evidence"]["endpoint_extent_verified"]
    assert len(result["exterior_probe_centers_px"]) == 2
    for points, xs in zip(result["interior_observed_points_px"], ((53.5, 55.5, 57.5), (187.5, 185.5, 183.5)), strict=True):
        assert len(points) == 3
        for point, x in zip(points, xs, strict=True):
            assert math.hypot(point["x"] - x, point["y"] - 90.5) < 0.35, point
    assert result["interior_maximum_offset_px"] == 1.5
    assert_unchanged(before)

    before = open_fixture(continuation)
    rejection = client.call("inspect_straight_edge_endpoints", request, expect_error=True)
    assert any("se prolonge ou reste ambigu" in block.get("text", "") for block in rejection["content"])
    assert_unchanged(before)
    before = open_fixture(masked)
    rejection = client.call("inspect_straight_edge_endpoints", request, expect_error=True)
    assert any("Contour intérieur" in block.get("text", "") for block in rejection["content"])
    assert_unchanged(before)
    print(f"Bord de {result['length_px']:.4f} px. Prolongement et masque intérieur refusés. Sessions inchangées.")


if __name__ == "__main__":
    main()
