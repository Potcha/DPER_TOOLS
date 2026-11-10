"""Generate synthetic DPM-7 inputs and optionally exercise an owned MCP host."""
import argparse
import copy
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from PIL import Image
from generate_vector_drawing_fixtures import Drawing
from smoke_inspection_mcp import Mcp

TRIANGLE = [(50, 95), (125, 120), (75, 205)]
SQUARE = [(230, 110), (320, 110), (320, 200), (230, 200)]
TOOL = "propose_circle_landmark_matches"


def make_fixtures(folder):
    pdf = folder / "reperes-circulaires.pdf"
    photo = folder / "photo-reperes.png"
    drawing = Drawing()
    drawing.text(25, 30, "4 mm")
    drawing.text(210, 48, "5 mm")
    for x, y in TRIANGLE + SQUARE:
        drawing.circle(x, y, 12)
    pdf.write_bytes(drawing.pdf_bytes())

    image = Image.new("RGB", (400, 330))
    centers = [(x + 22, y + 18) for x, y in TRIANGLE + SQUARE]
    for y in range(image.height):
        for x in range(image.width):
            if y >= 293:
                value = 40 if (x // 5 + y // 5) % 2 == 0 else 190
            else:
                value = 200 if any((x - cx) ** 2 + (y - cy) ** 2 <= 15 ** 2
                                   for cx, cy in centers) else 50
            image.putpixel((x, y), (value,) * 3)
    image.save(photo)
    return pdf, photo


def prepare_views(client, pdf, photo):
    client.call("open_image", {"path": str(photo)})
    context = client.call("get_measurement_context")
    assert context["image_quality_assessment"]["is_exploitable"], context["image_quality_assessment"]
    state = client.call("analyze_drawing", {"path": str(pdf), "expected_revision": context["document_revision"]})
    features = {item["nominal"]: item["id"] for item in state["inspection"]["plan"]["characteristics"]}
    client.call("analyze_drawing_geometry", {"page": 1, "expected_revision": state["document_revision"]})
    summaries = client.call("get_drawing_geometries", {"page": 1, "limit": 100})["items"]
    circles = [client.call("get_drawing_geometries", {"page": 1, "geometry_id": item["id"]})["items"][0]
               for item in summaries if item["kind"] == "circle_candidate"]

    def ids_for(points):
        ids = []
        for x, y in points:
            matches = [item["id"] for item in circles
                       if abs(item["circle"]["center"]["x"] - x) < .02
                       and abs(item["circle"]["center"]["y"] - y) < .02]
            assert len(matches) == 1, ((x, y), matches)
            ids.append(matches[0])
        return ids

    requests = []
    for label, points, nominal in (("DPM-7 triangle", TRIANGLE, 4), ("DPM-7 carré", SQUARE, 5)):
        ids = ids_for(points)
        revision = client.call("get_inspection_state")["document_revision"]
        view = client.call("define_drawing_view", {"expected_revision": revision, "page": 1,
                           "label": label, "feature_ids": [features[nominal]], "geometry_ids": ids})
        searches = [{"id": f"photo-{i}", "center_px": {"x": x + 24, "y": y + 17}, "radius_px": 14}
                    for i, (x, y) in reversed(list(enumerate(points)))]
        requests.append({"capture_id": context["capture_id"], "view_id": view["view"]["id"],
                         "plan_circle_ids": ids, "image_circle_searches": searches, "threshold_px": .6})
    revision = client.call("get_inspection_state")["document_revision"]
    for request in requests:
        request["expected_revision"] = revision
    return requests


def verify(client, pdf, requests):
    triangle, square = requests
    baseline = client.call("get_inspection_state")
    context = client.call("get_measurement_context")
    cases = []

    def unchanged(source_changed=False):
        after = client.call("get_inspection_state")
        if source_changed:
            assert after["drawing_view_status"] and all(not v["current"] for v in after["drawing_view_status"])
            after["drawing_view_status"] = baseline["drawing_view_status"]
        assert after == baseline
        assert client.call("get_measurement_context") == context

    for request, state, accepted, permutations in ((triangle, "unique_hypothesis", 1, 6),
                                                   (square, "ambiguous", 4, 24)):
        result = client.call(TOOL, request)
        proposal = result["proposal"]
        matching = proposal["matching"]
        assert matching["state"] == state and matching["accepted_count"] == accepted, matching
        assert matching["evaluated_permutations"] == permutations
        assert len(matching["rejected_permutations"]) == permutations - accepted
        assert ("unique_candidate_id" in matching) == (accepted == 1)
        assert not matching["registered"] and not matching["reviewed"]
        assert result["view_source_proof"] and result["document_fingerprint"] and result["image_fingerprint"]
        assert result["capture_id"] == request["capture_id"] and result["document_revision"] == request["expected_revision"]
        for evidence in proposal["image_circles"]:
            search = evidence["search"]["center_px"]
            fit = evidence["fit"]
            center = fit["geometry"]["Circle"]["center"]
            assert abs(center["x"] - (search["x"] - 2)) < .3
            assert abs(center["y"] - (search["y"] + 1)) < .3
            assert fit["evidence"]["coverage_fraction"] >= .7 and fit["evidence"]["accepted_points"]
        unchanged()
        cases.append({"case": state, "response": result})

    mismatched = copy.deepcopy(triangle)
    mismatched["image_circle_searches"][0] = square["image_circle_searches"][0]
    result = client.call(TOOL, mismatched)
    matching = result["proposal"]["matching"]
    assert matching["state"] == "no_match" and matching["accepted_count"] == 0
    assert len(matching["rejected_permutations"]) == 6 and "unique_candidate_id" not in matching
    unchanged()
    cases.append({"case": "no_match", "response": result})

    for name in ("stale_revision", "wrong_capture", "foreign_pdf_circle", "overlapping_searches", "missing_contour"):
        request = copy.deepcopy(triangle)
        if name == "stale_revision":
            request["expected_revision"] -= 1
        elif name == "wrong_capture":
            request["capture_id"] += "-wrong"
        elif name == "foreign_pdf_circle":
            request["plan_circle_ids"][0] = square["plan_circle_ids"][0]
        elif name == "overlapping_searches":
            request["image_circle_searches"][0]["center_px"] = request["image_circle_searches"][1]["center_px"]
        else:
            request["image_circle_searches"][0]["center_px"] = {"x": 170, "y": 260}
        cases.append({"case": name, "response": client.call(TOOL, request, expect_error=True)})
        unchanged()

    if pdf is not None:
        original = pdf.read_bytes()
        try:
            pdf.write_bytes(original + b"\n% changed synthetic source\n")
            cases.append({"case": "changed_pdf", "response": client.call(TOOL, triangle, expect_error=True)})
            unchanged(source_changed=True)
        finally:
            pdf.write_bytes(original)
        unchanged()
    return cases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path)
    parser.add_argument("--test-port", type=int)
    args = parser.parse_args()
    if args.test_port is not None and not 18765 <= args.test_port <= 65535:
        parser.error("Use an owned test host >=18765.")
    folder = args.folder.resolve()
    folder.mkdir(parents=True, exist_ok=False)
    pdf, photo = make_fixtures(folder)
    if args.test_port is None:
        print(pdf, photo, sep="\n")
        return
    client = Mcp(folder, args.test_port)
    requests = prepare_views(client, pdf, photo)
    cases = verify(client, pdf, requests)
    summary = {"passed": len(cases), "failed": 0, "diagnostics_read_only": True,
               "requests": requests, "cases": cases}
    (folder / "summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")
    print(json.dumps({k: summary[k] for k in ("passed", "failed", "diagnostics_read_only")}))


if __name__ == "__main__":
    main()
