"""Verify documentary associations and PDF view cues on an owned MCP test host."""
import argparse
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from generate_vector_drawing_fixtures import Drawing
from smoke_inspection_mcp import Mcp


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path)
    parser.add_argument("--test-port", type=int, required=True)
    args = parser.parse_args()
    if not 18765 <= args.test_port <= 65535:
        parser.error("Use the port of an owned test host.")
    folder = args.folder.resolve()
    folder.mkdir(parents=True, exist_ok=False)
    client = Mcp(folder, args.test_port)

    def analyze(path):
        revision = client.call("get_inspection_state")["document_revision"]
        state = client.call("analyze_drawing", {"path": str(path), "expected_revision": revision})
        return client.call("analyze_drawing_geometry", {"page": 1, "expected_revision": state["document_revision"]})

    symmetric = Path(__file__).resolve().parents[1] / "tests/fixtures/drawing_geometry/03_two_symmetric_views.pdf"
    catalog = analyze(symmetric)
    before = client.call("get_inspection_state")
    indices = client.call("inspect_drawing_scope_hypotheses", {
        "page": 1, "expected_revision": catalog["document_revision"],
    })["hypotheses"]
    assert [entry["label"] for entry in indices["anchors"]] == ["VUE A", "VUE B"], indices
    assert all(entry["words"] and len(entry["alternatives"]) == 1 for entry in indices["anchors"]), indices
    assert indices["requires_confirmation"] and indices["selected_view_id"] is None
    assert client.call("get_inspection_state") == before

    pdf = folder / "supports-documentaires.pdf"
    drawing = Drawing()
    drawing.text(30, 30, "18 mm")
    drawing.text(30, 55, "123°")
    drawing.text(220, 30, "20 mm")
    drawing.polyline(((40, 80), (160, 80)))
    drawing.polyline(((60, 120), (140, 120)))
    drawing.polyline(((80, 40), (160, 120)))
    pdf.write_bytes(drawing.pdf_bytes())
    catalog = analyze(pdf)
    plan = client.call("get_inspection_state")["inspection"]["plan"]
    features = {item["nominal"]: item["id"] for item in plan["characteristics"]}
    geometries = client.call("get_drawing_geometries", {"page": 1, "limit": 100})["items"]
    ids = [item["id"] for item in geometries if item["kind"] == "line"]
    assert len(ids) == 3, geometries

    def define(revision, label, feature_ids, geometry_ids):
        return client.call("define_drawing_view", {
            "expected_revision": revision, "page": 1, "label": label,
            "feature_ids": feature_ids, "geometry_ids": geometry_ids,
        })

    view = define(catalog["document_revision"], "Vue explicite", [features[18], features[123]], ids)
    linear = {
        "expected_revision": view["document_revision"], "feature_id": features[18],
        "view_id": view["view"]["id"], "support_ids": ids[:2], "justification": "Supports du plan synthétique.",
    }
    context = client.call("get_measurement_context")
    selected = client.call("select_drawing_supports", linear)
    assert abs(selected["association"]["evidence"]["normal_separation_pt"] - 40) < .01, selected
    assert client.call("get_measurement_context")["measurements"] == context["measurements"]
    client.call("select_drawing_supports", linear, expect_error=True)

    angular = dict(linear, expected_revision=selected["document_revision"],
                   feature_id=features[123], support_ids=[ids[0], ids[2]])
    diagnostic = client.call("inspect_drawing_angular_supports", angular)["diagnostic"]
    assert diagnostic["selected_angle_deg"] is None
    sector = next(sector for sector in diagnostic["available_sectors"] if abs(sector["angle_deg"] - 45) < .01)
    client.call("select_drawing_supports", angular, expect_error=True)
    selected_angle = client.call("select_drawing_supports",
                                 dict(angular, angular_endpoint_indices=sector["endpoint_indices"]))
    assert selected_angle["association"]["angular_sector"] == sector
    state = client.call("get_inspection_state")
    assert len(state["drawing_support_association_status"]) == 2
    assert all(item["current"] for item in state["drawing_support_association_status"]), state
    for association in state["inspection"]["support_associations"]:
        assert not association["evidence"]["reviewed"] and not association["evidence"]["measured_on_photo"]

    competing = define(selected_angle["document_revision"], "Vue concurrente", [features[20]], ids[:2])
    conflicted = dict(linear, expected_revision=competing["document_revision"])
    client.call("inspect_drawing_linear_supports", conflicted, expect_error=True)
    client.call("select_drawing_supports", conflicted, expect_error=True)
    after = client.call("get_inspection_state")
    assert all(not item["current"] for item in after["drawing_support_association_status"]), after
    assert after["document_revision"] == competing["document_revision"]
    print("PASS: two traced view cues, linear and angular selections, stale revision, missing sector, competing view.")


if __name__ == "__main__":
    main()
