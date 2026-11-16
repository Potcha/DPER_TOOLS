"""Exercise a quadratic span on an isolated Windows MCP test host."""

import argparse
import math
import sys
from pathlib import Path

from PIL import Image

sys.dont_write_bytecode = True
from smoke_inspection_mcp import Mcp


def edge_y(x):
    return 110 - (x - 120) ** 2 / 180


def step(distance):
    return 0.5 + 0.5 * math.tanh(distance / 0.7)


def image_fixture(folder, kind):
    values = []
    for y in range(210):
        for x in range(240):
            distance = y - edge_y(x)
            value = 30 + 170 * step(distance)
            if kind == "masked" and 114 <= x <= 126 and 87 <= y <= 125:
                value = 200
            if kind == "reflection":
                value = 25 + 90 * step(distance) + 100 * step(distance + 3)
            values.append((round(value),) * 3)
    image = Image.new("RGB", (240, 210))
    image.putdata(values)
    path = folder / f"{kind}.png"
    image.save(path)
    return path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--port", type=int, required=True)
    args = parser.parse_args()
    folder = args.output.resolve()
    folder.mkdir(parents=True, exist_ok=True)
    client = Mcp(folder, args.port)
    tool = "inspect_visible_contour_spans"
    start, end = {"x": 60, "y": 91}, {"x": 180, "y": 91}
    forward = {"Quadratic": {"start": start, "end": end, "midpoint_offset_px": 21}}
    reverse = {"Quadratic": {"start": end, "end": start, "midpoint_offset_px": -21}}
    wrong_bow = {"Quadratic": dict(forward["Quadratic"], midpoint_offset_px=-21)}
    outside = {"Quadratic": dict(forward["Quadratic"], start={"x": 3, "y": 91})}
    maximum_error = 0.0

    def state():
        return client.call("get_measurement_context"), client.call("get_inspection_state")

    for kind in ("visible", "masked", "reflection"):
        client.call("open_image", {"path": str(image_fixture(folder, kind))})
        before = state()
        guides = [forward, reverse]
        if kind == "visible":
            guides.extend((wrong_bow, outside))
        request = dict(capture_id=before[0]["capture_id"],
                       expected_revision=before[0]["document_revision"],
                       guides=guides, search_radius_px=5)
        result = client.call(tool, request)
        assert result["capture_id"] == request["capture_id"]
        assert result["document_revision"] == request["expected_revision"]
        assert len(result["image_fingerprint"]) == 16
        outcomes = result["spans"]
        expected = ["observed", "observed", "refused", "refused"] if kind == "visible" else ["refused"] * 2
        assert [item["status"] for item in outcomes] == expected, outcomes
        for item in outcomes:
            if item["status"] == "refused":
                assert item["reason"] and "support" not in item
                continue
            support = item["support"]
            points = support["accepted_points"]
            assert len(points) == support["sample_count"]
            assert not support["endpoint_extent_verified"]
            assert "geometry" not in support
            maximum_error = max(maximum_error, max(abs(p["y"] - edge_y(p["x"])) for p in points))
        if kind == "visible":
            a, b = (item["support"]["accepted_points"] for item in outcomes[:2])
            assert all(math.hypot(p["x"] - q["x"], p["y"] - q["y"]) < 1e-6 for p, q in zip(a, reversed(b)))
            client.call(tool, dict(request, expected_revision=request["expected_revision"] - 1), expect_error=True)
        assert state() == before
    assert maximum_error < 0.25, maximum_error
    print(f"4 appels; erreur verticale maximale depuis la référence synthétique : {maximum_error:.4f} px")
    print(client.log_path)


if __name__ == "__main__":
    main()
