"""Generate the DPM-3 angle fixture, optionally verify it on an owned MCP host."""
import argparse
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from generate_vector_drawing_fixtures import Drawing
from smoke_inspection_mcp import Mcp


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path)
    parser.add_argument("--test-port", type=int)
    args = parser.parse_args()
    if args.test_port is not None and not 18765 <= args.test_port <= 65535:
        parser.error("Use an owned test host >=18765, never the user instance.")
    folder = args.folder.resolve()
    folder.mkdir(parents=True, exist_ok=False)
    pdf = folder / "supports-angulaires.pdf"
    drawing = Drawing()
    drawing.text(30, 25, "Units mm")
    drawing.text(30, 45, "123 ±0.1°")
    drawing.text(30, 65, "18 mm")
    drawing.text(30, 290, "SYNTHETIC SOFTWARE TEST - NO PHYSICAL SCALE", size=9)
    lines = [((60, 100), (180, 100)), ((80, 60), (160, 140)),
             ((60, 160), (180, 160)), ((240, 140), (260, 160)),
             ((60, 99), (180, 101)), ((300, 80), (300, 150)),
             ((160, 140), (80, 60))]
    for points in lines:
        drawing.polyline(points)
    pdf.write_bytes(drawing.pdf_bytes())
    if args.test_port is None:
        print(pdf)
        return
    client = Mcp(folder, args.test_port)
    current = client.call("get_inspection_state")
    state = client.call("analyze_drawing", {"path": str(pdf), "expected_revision": current["document_revision"]})
    features = state["inspection"]["plan"]["characteristics"]
    angle = next(f for f in features if f["kind"] == "angle")
    linear = next(f for f in features if f["kind"] == "linear")
    assert angle["nominal"] == 123, angle
    catalog = client.call("analyze_drawing_geometry", {"page": 1, "expected_revision": state["document_revision"]})
    summaries = client.call("get_drawing_geometries", {"page": 1, "limit": 100})["items"]
    items = [client.call("get_drawing_geometries", {"page": 1, "geometry_id": item["id"]})["items"][0]
             for item in summaries]

    def find(points):
        matches = [item for item in items if item["kind"] == "line" and len(item["points"]) == 2
                   and all(abs(item["points"][i][axis] - points[i][j]) < .01
                           for i in range(2) for j, axis in enumerate(("x", "y")))]
        assert len(matches) == 1, (points, matches)
        return matches[0]["id"]

    ids = [find(points) for points in lines]
    scoped_ids = [ids[i] for i in (0, 1, 2, 3, 4, 6)]
    defined = client.call("define_drawing_view", {"expected_revision": catalog["document_revision"],
        "page": 1, "label": "DPM-3 explicit angle supports", "feature_ids": [angle["id"], linear["id"]],
        "geometry_ids": scoped_ids})
    request = {"expected_revision": defined["document_revision"], "feature_id": angle["id"],
        "view_id": defined["view"]["id"], "support_ids": ids[:2],
        "justification": "Explicit documentary supports, no automatic choice of angular sector."}
    baseline = client.call("get_inspection_state")
    context = client.call("get_measurement_context")
    results = []

    def unchanged(source_changed=False):
        after = client.call("get_inspection_state")
        if source_changed:
            assert all(not item["current"] for item in after["drawing_view_status"])
            after["drawing_view_status"] = baseline["drawing_view_status"]
        assert after == baseline
        assert client.call("get_measurement_context") == context

    for name, pair in (("45_and_135_not_nominal_123", ids[:2]),
                       ("reverse_support_order", ids[1::-1]),
                       ("reverse_endpoints", [ids[0], ids[6]])):
        result = client.call("inspect_drawing_angular_supports", dict(request, support_ids=pair))
        diagnostic = result["diagnostic"]
        assert abs(diagnostic["intersection_pt"]["x"] - 120) < .01
        assert abs(diagnostic["intersection_pt"]["y"] - 100) < .01
        assert all(abs(a - b) < .01 for a, b in zip(diagnostic["candidate_angles_deg"], [45, 135]))
        assert diagnostic["selected_angle_deg"] is None
        assert diagnostic["state"] == "documentary_proposal"
        assert not diagnostic["reviewed"] and not diagnostic["measured_on_photo"]
        assert diagnostic["feature_source"]["word_ids"] and diagnostic["view_source_proof"]
        assert len(diagnostic["supports"]) == 2 and diagnostic["suspension_reasons"]
        unchanged()
        results.append({"case": name, "passed": True, "diagnostic": diagnostic})
    for name, updates in (("parallel", {"support_ids": [ids[0], ids[2]]}),
                          ("extrapolation", {"support_ids": [ids[0], ids[3]]}),
                          ("near_parallel", {"support_ids": [ids[0], ids[4]]}),
                          ("outside_view", {"support_ids": [ids[0], ids[5]]}),
                          ("duplicate_support", {"support_ids": [ids[0], ids[0]]}),
                          ("wrong_dimension_kind", {"feature_id": linear["id"]}),
                          ("stale_revision", {"expected_revision": request["expected_revision"] - 1}),
                          ("empty_justification", {"justification": " "})):
        response = client.call("inspect_drawing_angular_supports", dict(request, **updates), expect_error=True)
        unchanged()
        results.append({"case": name, "passed": True, "response": response})
    response = client.rpc("tools/call", {"name": "inspect_drawing_angular_supports",
        "arguments": dict(request, reviewed=True)}, return_envelope=True)
    assert "error" in response or response.get("result", {}).get("isError") is True, response
    unchanged()
    results.append({"case": "forged_review", "passed": True, "response": response})
    with pdf.open("ab") as stream:
        stream.write(b"\n% DPM-3 changed source must suspend reuse\n")
    response = client.call("inspect_drawing_angular_supports", request, expect_error=True)
    unchanged(source_changed=True)
    results.append({"case": "changed_source", "passed": True, "response": response})
    summary = {"passed": len(results), "failed": 0, "session_unchanged": True,
               "request": request, "cases": results}
    (folder / "summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")
    print(json.dumps({key: summary[key] for key in ("passed", "failed", "session_unchanged")}))


if __name__ == "__main__":
    main()
