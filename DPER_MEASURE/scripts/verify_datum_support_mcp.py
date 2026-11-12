"""Generate and verify explicit datum/support proposals on an owned MCP host."""
import argparse
import json
from pathlib import Path
import sys
sys.dont_write_bytecode = True
from generate_vector_drawing_fixtures import Drawing
from smoke_inspection_mcp import Mcp


def fixture(path, duplicate=False, outside=False):
    drawing = Drawing()
    drawing.command("/Span << /ActualText <FEFF2316> >> BDC")
    drawing.text(35, 55, "+", 14)
    drawing.command("EMC")
    drawing.circle(39, 51, 2)
    drawing.text(45, 55, " | Ø0,1 mm | A | B", 14)
    drawing.text(35, 240 if outside else 105, "DATUM A")
    if duplicate:
        drawing.text(35, 150, "REPERE A")
    drawing.polyline(((20, 200), (300, 200)))
    path.write_bytes(drawing.pdf_bytes())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path)
    parser.add_argument("--test-port", required=True, type=int)
    args = parser.parse_args()
    if not 18765 <= args.test_port <= 65535:
        parser.error("A dedicated owned test port is required.")
    folder = args.folder.resolve()
    folder.mkdir(parents=True, exist_ok=False)
    client = Mcp(folder, args.test_port)
    results = []
    for name, duplicate, outside in [("unique", False, False), ("duplicate", True, False), ("outside", False, True)]:
        path = folder / (name + ".pdf")
        fixture(path, duplicate, outside)
        rev = client.call("get_inspection_state")["document_revision"]
        analyzed = client.call("analyze_drawing", {"path": str(path), "expected_revision": rev})
        catalog = client.call("analyze_drawing_geometry", {"page": 1, "expected_revision": analyzed["document_revision"]})
        plan = client.call("get_inspection_state")["inspection"]["plan"]
        feature = plan["characteristics"][0]["id"]
        items = client.call("get_drawing_geometries", {"page": 1, "limit": 100})["items"]
        support = next(item["id"] for item in items if item["kind"] == "line")
        view = client.call("define_drawing_view", {"expected_revision": catalog["document_revision"], "page": 1,
            "label": "Explicit datum scope", "feature_ids": [feature], "geometry_ids": [support]})
        params = {"expected_revision": view["document_revision"], "feature_id": feature,
            "view_id": view["view"]["id"], "label": "A", "support_id": support,
            "justification": "Operator selected this vector support for documentary review."}
        before = client.call("get_inspection_state")
        context = client.call("get_measurement_context")
        response = client.call("inspect_drawing_datum_support", params, expect_error=duplicate or outside)
        if not duplicate and not outside:
            diagnostic = response["diagnostic"]
            assert diagnostic["declaration"]["source_text"] == "DATUM A", diagnostic
            assert diagnostic["support"]["id"] == support
            assert diagnostic["suspension_reason"] == "drawing_relation_unresolved"
            assert all(diagnostic[key] is False for key in ["reviewed", "reference_resolved", "measured_on_photo"])
            for change in [{"label": "B"}, {"support_id": "absent"}, {"justification": " "},
                           {"expected_revision": 0}, {"reviewed": True}]:
                client.call("inspect_drawing_datum_support", dict(params, **change), expect_error=True)
            results.extend(["missing datum refused", "missing support refused", "empty justification refused",
                            "stale revision refused", "client authority field refused"])
        assert client.call("get_inspection_state") == before
        assert client.call("get_measurement_context") == context
        results.append(name + (" refused" if duplicate or outside else " accepted as proposal"))
    (folder / "summary.json").write_text(json.dumps({"results": results, "checks": len(results),
        "session_unchanged_after_diagnostics": True}, indent=2), encoding="utf-8")
    print(json.dumps(results))


if __name__ == "__main__":
    main()
