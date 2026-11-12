"""Generate reproducible arc fixtures and optionally verify an owned MCP host."""
import argparse
import math
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from PIL import Image
from verify_line_intersection_mcp import Mcp, search


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path)
    parser.add_argument("--test-port", type=int)
    args = parser.parse_args()
    if args.test_port is not None and not 18765 <= args.test_port <= 65535:
        parser.error("Use a dedicated owned test port >= 18765.")
    folder = args.folder.resolve()
    folder.mkdir(parents=True, exist_ok=False)
    sources = []
    for filename, expected_error in (
        ("arc-borne.png", None),
        ("arc-prolonge.png", "se prolonge ou reste ambigu"),
        ("arc-masque.png", "Contour intérieur"),
    ):
        pixels = []
        for y in range(256):
            for x in range(256):
                count = 0
                for dy in range(4):
                    for dx in range(4):
                        u = x + (dx + .5) / 4 - .5 - 128.5
                        v = y + (dy + .5) / 4 - .5 - 128.5
                        quadrant = u >= 0 and v >= 0
                        disk = u * u + v * v <= 3600
                        if filename == "arc-prolonge.png":
                            count += disk != quadrant
                        else:
                            masked = filename == "arc-masque.png" and 5 <= v <= 9.5 and u > 54 and 55 <= math.hypot(u, v) <= 65
                            count += disk and quadrant and not masked
                pixels.append((30 + round(190 * count / 16),) * 3)
        image = Image.new("RGB", (256, 256))
        image.putdata(pixels)
        path = folder / filename
        image.save(path)
        sources.append((path, expected_error))
        print(path)
    if args.test_port is None:
        return
    client = Mcp(folder, args.test_port)
    assert "inspect_arc_endpoints" in {t["name"] for t in client.rpc("tools/list", {})["tools"]}
    request = {"search": {
        "center": {"x": 129.1, "y": 127.9}, "radius": 61,
        "start_rad": .05, "sweep_rad": math.pi / 2 - .1,
        "start_boundary": search((185.5, 127.2), (158.5, 127.2)),
        "end_boundary": search((127.2, 158.5), (127.2, 185.5)),
    }}
    for path, expected_error in sources:
        before = client.call("open_image", {"path": str(path)})
        request.update(capture_id=before["capture_id"], expected_revision=before["document_revision"])
        if expected_error:
            refused = client.call("inspect_arc_endpoints", request, expect_error=True)
            assert any(expected_error in b.get("text", "") for b in refused["content"]), refused
        else:
            result = client.call("inspect_arc_endpoints", request)["diagnostic"]
            for side, expected in (("start", (188.5, 128.5)), ("end", (128.5, 188.5))):
                point = result[side]["intersection_px"]
                assert math.hypot(point["x"] - expected[0], point["y"] - expected[1]) < .35, point
            assert abs(result["sweep_rad"] - math.pi / 2) < .01
            assert abs(result["arc_length_px"] - 30 * math.pi) < .5
            assert not result["arc_support"]["evidence"]["endpoint_extent_verified"]
            for points, endpoint in zip(result["interior_observed_points_px"], ((188.5, 128.5), (128.5, 188.5)), strict=True):
                assert len(points) == 3
                for point, distance in zip(points, (3, 5, 7), strict=True):
                    assert abs(math.hypot(point["x"] - endpoint[0], point["y"] - endpoint[1]) - distance) < .5, point
            assert result["interior_maximum_offset_px"] == 1.5
            print(f"Arc: {result['arc_length_px']:.6f} px; sweep {result['sweep_rad']:.6f} rad")
        after = client.call("get_measurement_context")
        for field in ("capture_id", "document_revision", "measurements", "calibration"):
            assert before[field] == after[field], field
    print("Arc borné accepté, continuation inversée et masque intérieur refusés, sessions inchangées.")


if __name__ == "__main__":
    main()
