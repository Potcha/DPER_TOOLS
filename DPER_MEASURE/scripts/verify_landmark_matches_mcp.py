"""DPM-7 exhaustive landmark hypotheses, using an owned test host only."""
import argparse
import copy
import json
import math
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from PIL import Image
from smoke_inspection_mcp import Mcp


def request_for(points, order=None):
    images = [(100 - 2*y, 50 + 2*x) for x, y in points]
    order = list(range(len(points))) if order is None else order
    return {"plan_landmarks": [{"id": f"p{i}", "position": {"x": x, "y": y}} for i, (x, y) in enumerate(points)],
            "image_landmarks": [{"id": f"i{i}", "position": {"x": images[i][0], "y": images[i][1]}} for i in order],
            "plan_source": "Explicit synthetic drawing points, no physical scale",
            "image_source": "Explicit synthetic image points, no pixel detection claimed",
            "threshold_px": .01}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path)
    parser.add_argument("--test-port", type=int, required=True)
    args = parser.parse_args()
    if not 18765 <= args.test_port <= 65535:
        parser.error("Use an owned test host >=18765, never the user instance.")
    folder = args.folder.resolve()
    folder.mkdir(parents=True, exist_ok=False)
    sentinel = folder / "sentinel.png"
    Image.new("RGB", (8, 8), (80, 80, 80)).save(sentinel)
    client = Mcp(folder, args.test_port)
    client.call("open_image", {"path": str(sentinel)})
    baseline = client.call("get_measurement_context")
    inspection = client.call("get_inspection_state")
    results = []

    def unchanged():
        assert client.call("get_measurement_context") == baseline
        assert client.call("get_inspection_state") == inspection

    triangle = request_for([(0, 0), (40, 0), (10, 30)], [2, 0, 1])
    square = request_for([(0, 0), (20, 0), (20, 20), (0, 20)])
    reflected = copy.deepcopy(triangle)
    for point in reflected["image_landmarks"]:
        point["position"]["x"] = -point["position"]["x"]
    outlier = request_for([(0, 0), (40, 0), (10, 30), (70, 25)])
    outlier["image_landmarks"][-1]["position"]["x"] += 200
    noisy = copy.deepcopy(triangle)
    noisy["threshold_px"] = .5
    noisy["image_landmarks"][0]["position"]["x"] += .05
    for name, request, state, accepted in (
        ("unique_triangle", triangle, "unique_hypothesis", 1),
        ("symmetric_square", square, "ambiguous", 4),
        ("reflection", reflected, "no_match", 0),
        ("outlier_never_discarded", outlier, "no_match", 0),
        ("noisy_unique", noisy, "unique_hypothesis", 1),
    ):
        result = client.call("propose_landmark_matches", request)
        assert result["state"] == state and result["accepted_count"] == accepted, (name, result)
        assert result["evaluated_permutations"] == math.factorial(len(request["plan_landmarks"]))
        assert len(result["hypotheses"]) == accepted
        assert len(result["rejected_permutations"]) + accepted == result["evaluated_permutations"]
        assert not result["registered"] and not result["reviewed"] and not result["input_provenance_verified"]
        assert (result.get("unique_candidate_id") is not None) == (accepted == 1)
        assert result["plan_landmarks"] == request["plan_landmarks"]
        assert result["image_landmarks"] == request["image_landmarks"]
        for hypothesis in result["hypotheses"]:
            assert len(hypothesis["registration"]["inlier_indices"]) == len(request["plan_landmarks"])
            assert max(hypothesis["registration"]["residuals_px"]) <= request["threshold_px"]
        unchanged()
        results.append({"case": name, "passed": True, "request": request, "response": result})
    invalid = []
    for name in ("unequal", "too_few", "duplicate_id", "duplicate_point", "missing_source", "zero_threshold", "forged_review"):
        request = copy.deepcopy(triangle)
        if name == "unequal": request["image_landmarks"].pop()
        elif name == "too_few":
            request["plan_landmarks"].pop(); request["image_landmarks"].pop()
        elif name == "duplicate_id": request["plan_landmarks"][1]["id"] = " p0 "
        elif name == "duplicate_point": request["image_landmarks"][1]["position"] = request["image_landmarks"][0]["position"]
        elif name == "missing_source": request["image_source"] = " "
        elif name == "zero_threshold": request["threshold_px"] = 0
        else: request["reviewed"] = True
        invalid.append((name, request))
    for name, request in invalid:
        response = client.rpc("tools/call", {"name": "propose_landmark_matches", "arguments": request}, return_envelope=True)
        assert "error" in response or response.get("result", {}).get("isError") is True, (name, response)
        unchanged()
        results.append({"case": name, "passed": True, "request": request, "response": response})
    summary = {"passed": len(results), "failed": 0, "session_unchanged": True, "cases": results}
    (folder / "summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")
    print(json.dumps({key: summary[key] for key in ("passed", "failed", "session_unchanged")}))


if __name__ == "__main__":
    main()
