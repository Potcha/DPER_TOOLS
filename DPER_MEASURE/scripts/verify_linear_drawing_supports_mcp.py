"""Reproduce the DPM-3 PDF support diagnostic on an owned test host."""
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
        parser.error("Use an owned test host >= 18765, never the user instance.")
    folder = args.folder.resolve()
    folder.mkdir(parents=True, exist_ok=False)
    pdf = folder / "supports-lineaires.pdf"
    drawing = Drawing()
    drawing.text(30, 30, "18 ±0.1 mm")
    drawing.text(30, 290, "SYNTHETIC SOFTWARE TEST - NO PHYSICAL SCALE", size=9)
    lines = [((40, 80), (160, 80)), ((60, 120), (140, 120)),
             ((200, 80), (240, 100)), ((200, 150), (240, 150))]
    for points in lines:
        drawing.polyline(points)
    pdf.write_bytes(drawing.pdf_bytes())
    if args.test_port is None:
        print(pdf)
        return
    client = Mcp(folder, args.test_port)
    current = client.call("get_inspection_state")
    state = client.call("analyze_drawing", {"path": str(pdf), "expected_revision": current["document_revision"]})
    feature = state["inspection"]["plan"]["characteristics"][0]
    assert feature["nominal"] == 18 and feature["kind"] == "linear", feature
    catalog = client.call("analyze_drawing_geometry", {"page": 1, "expected_revision": state["document_revision"]})
    summaries = client.call("get_drawing_geometries", {"page": 1, "limit": 100})["items"]
    items = [client.call("get_drawing_geometries", {"page": 1, "geometry_id": item["id"]})["items"][0]
             for item in summaries]

    def find(points):
        matches = [item for item in items if item["kind"] == "line" and
                   len(item["points"]) == 2 and all(abs(item["points"][i][axis] - points[i][j]) < .01
                   for i in range(2) for j, axis in enumerate(("x", "y")))]
        assert len(matches) == 1, (points, items)
        return matches[0]["id"]

    ids = [find(points) for points in lines]
    defined = client.call("define_drawing_view", {"expected_revision": catalog["document_revision"],
        "page": 1, "label": "DPM-3 explicit linear supports", "feature_ids": [feature["id"]], "geometry_ids": ids})
    request = {"expected_revision": defined["document_revision"], "feature_id": feature["id"],
        "view_id": defined["view"]["id"], "support_ids": ids[:2],
        "justification": "Synthetic pair explicitly identified; no interpretation of dimension-line roles."}
    baseline = client.call("get_inspection_state")
    context = client.call("get_measurement_context")
    results = []

    def unchanged(source_changed=False):
        after = client.call("get_inspection_state")
        if source_changed:
            assert all(not item["current"] for item in after["drawing_view_status"])
            # Status is recomputed from external bytes; the stored session is unchanged.
            after["drawing_view_status"] = baseline["drawing_view_status"]
        assert after == baseline
        assert client.call("get_measurement_context") == context

    for name, pair in (("parallel", ids[:2]), ("reverse_order", ids[1::-1])):
        result = client.call("inspect_drawing_linear_supports", dict(request, support_ids=pair))
        diagnostic = result["diagnostic"]
        assert abs(diagnostic["normal_separation_pt"] - 40) < .01
        assert abs(diagnostic["overlap_length_pt"] - 80) < .01
        assert diagnostic["state"] == "documentary_proposal"
        assert not diagnostic["reviewed"] and not diagnostic["measured_on_photo"]
        assert diagnostic["feature_source"]["word_ids"] and diagnostic["view_source_proof"]
        assert len(diagnostic["supports"]) == 2 and diagnostic["suspension_reasons"]
        unchanged()
        results.append({"case": name, "passed": True, "diagnostic": diagnostic})
    for name, updates in (("duplicate", {"support_ids": [ids[0], ids[0]]}),
                          ("nonparallel", {"support_ids": [ids[0], ids[2]]}),
                          ("no_overlap", {"support_ids": [ids[0], ids[3]]}),
                          ("stale_revision", {"expected_revision": request["expected_revision"] - 1}),
                          ("empty_justification", {"justification": " "}),
                          ("unknown_support", {"support_ids": [ids[0], "unknown"]})):
        response = client.call("inspect_drawing_linear_supports", dict(request, **updates), expect_error=True)
        unchanged()
        results.append({"case": name, "passed": True, "response": response})
    # The source bytes change without reimporting. A cached view must not authorize reuse.
    with pdf.open("ab") as source:
        source.write(b"\n% changed source for stale-view rejection\n")
    response = client.call("inspect_drawing_linear_supports", request, expect_error=True)
    unchanged(source_changed=True)
    results.append({"case": "changed_pdf_source", "passed": True, "response": response})
    summary = {"passed": len(results), "failed": 0, "session_unchanged": True,
               "request": request, "cases": results}
    (folder / "summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")
    print(json.dumps({key: summary[key] for key in ("passed", "failed", "session_unchanged")}))


if __name__ == "__main__":
    main()
