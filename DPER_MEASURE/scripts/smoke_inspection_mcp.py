"""Exercise a dedicated, freshly launched DPER_MEASURE instance on synthetic data.

Only use --run after checking that the MCP listener belongs to the test process.
This script never starts/stops the application or modifies a real inspection.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import subprocess
import sys
import urllib.error
import urllib.request

# Importing the fixture helper must not create unrelated files in scripts/.
sys.dont_write_bytecode = True

SMOKE_INSPECTION_OPTIONS = {"sample_count": 96, "search_radius_px": 5.0, "minimum_gradient": 35.0,
    "polarity": "Any", "minimum_coverage": 0.8, "outlier_threshold_px": 1.5,
    "maximum_rms_px": 0.5, "minimum_arc_sweep_rad": math.pi / 3.0}


def fixtures(folder):
    from PIL import Image, ImageDraw
    from generate_vector_drawing_fixtures import principal_fixture
    folder.mkdir(parents=True, exist_ok=False)
    scale = 4
    image = Image.new("RGB", (400 * scale, 320 * scale), (190, 190, 190))
    draw = ImageDraw.Draw(image)
    draw.ellipse((140 * scale, 100 * scale, 260 * scale, 220 * scale), fill=(40, 40, 40))
    for bx, by in [(10, 10), (350, 10), (10, 260), (350, 260)]:
        for iy in range(8):
            for ix in range(8):
                shade = 55 if (ix + iy) % 2 else 215
                draw.rectangle(((bx + ix * 5) * scale, (by + iy * 5) * scale,
                                (bx + (ix + 1) * 5) * scale - 1,
                                (by + (iy + 1) * 5) * scale - 1), fill=(shade,) * 3)
    image = image.resize((400, 320), Image.Resampling.LANCZOS)
    image.save(folder / "synthetic.png")
    drawing, vector_truth = principal_fixture()
    vector_truth["file"] = "drawing.pdf"
    pdf = drawing.pdf_bytes()
    (folder / "drawing.pdf").write_bytes(pdf)
    calibration = {"id": "synthetic-independent-reference", "device": "Synthetic fixture",
                   "device_id": "synthetic", "resolution": [400, 320], "optical_setup": "Synthetic 2D plane",
                   "mm_per_pixel": 0.035, "reference_mm": 7.0, "reference_px": 200.0,
                   "validation_status": "single_reference_unverified", "reference_type": "segment",
                   "reference_points_native_px": [{"x": 50, "y": 290}, {"x": 250, "y": 290}],
                   "origin": "synthetic_fixture", "author": "deterministic smoke test"}
    project = {"schema_version": 13, "capture_id": "synthetic-smoke-capture", "image_file": "synthetic.png",
               "device": "Synthetic fixture", "device_id": "synthetic", "optical_setup": "Synthetic 2D plane",
               "calibration": calibration, "calibrations": [calibration], "measurements": []}
    (folder / "fixture.dperproj").write_text(json.dumps(project, indent=2), encoding="utf-8")
    (folder / "ground_truth.json").write_text(json.dumps({"center_px": [200, 160], "radius_px": 60,
        "independent_mm_per_px": 0.035, "diameter_mm": 4.2, "drawing_nominal_mm": 4.0,
        "synthetic_only": True, "fixture_version": 2, "drawing_sha256": hashlib.sha256(pdf).hexdigest(),
        "vector_fixture": vector_truth}, indent=2, ensure_ascii=False), encoding="utf-8")


class Mcp:
    def __init__(self, folder, port):
        self.folder, self.session, self.counter, self.log = folder, None, 0, []
        self.port = port
        attempt = 1
        while (folder / f"rpc_evidence_{attempt:03d}.json").exists():
            attempt += 1
        self.log_path = folder / f"rpc_evidence_{attempt:03d}.json"
        self.rpc("initialize", {"protocolVersion": "2025-03-26", "capabilities": {},
                               "clientInfo": {"name": "dper-synthetic-smoke", "version": "1"}})
        self.rpc("notifications/initialized", {}, notification=True)

    def rpc(self, method, params, notification=False):
        self.counter += 1
        request = {"jsonrpc": "2.0", "method": method, "params": params}
        if not notification:
            request["id"] = self.counter
        headers = {"Content-Type": "application/json", "Accept": "application/json, text/event-stream"}
        if self.session:
            headers["Mcp-Session-Id"] = self.session
        message = urllib.request.Request(f"http://127.0.0.1:{self.port}/mcp", json.dumps(request).encode(), headers)
        try:
            with urllib.request.urlopen(message, timeout=120) as response:
                if response.headers.get("Mcp-Session-Id"):
                    self.session = response.headers["Mcp-Session-Id"]
                body = response.read().decode("utf-8")
        except urllib.error.HTTPError as error:
            body = error.read().decode("utf-8", errors="replace")
            raise AssertionError(f"HTTP {error.code} during {method}: {body[:1000]}") from error
        if not body.strip():
            return None
        if body.lstrip().startswith("{"):
            result = json.loads(body)
        else:
            messages = []
            for event in body.replace("\r\n", "\n").split("\n\n"):
                data = "\n".join(line[5:].lstrip() for line in event.splitlines() if line.startswith("data:")).strip()
                if data:
                    messages.append(json.loads(data))
            assert messages, f"Aucune réponse JSON dans le flux SSE : {body[:1000]}"
            result = next((item for item in messages if item.get("id") == request.get("id")), messages[-1])
        self.log.append({"request": request, "response": result})
        self.log_path.write_text(json.dumps(self.log, indent=2, ensure_ascii=False), encoding="utf-8")
        assert "error" not in result, result.get("error")
        return result.get("result")

    def call(self, name, arguments=None, expect_error=False):
        response = self.rpc("tools/call", {"name": name, "arguments": arguments or {}})
        is_error = response.get("isError", False)
        assert is_error == expect_error, (name, response)
        if expect_error:
            return response
        blocks = [block["text"] for block in response.get("content", []) if block.get("type") == "text"]
        assert blocks, (name, response)
        if name == "save_project":
            return "\n".join(blocks)
        return json.loads("\n".join(blocks))


def run_bound_workflow(client, folder, feature_id, options, truth):
    state = client.call("get_inspection_state")
    capture = state["capture_id"]
    catalog = client.call("analyze_drawing_geometry", {"page": 1, "expected_revision": state["document_revision"]})
    assert catalog["page"] == 1 and catalog["primitive_count"] >= 2, catalog
    assert catalog["document_fingerprint"] == state["inspection"]["plan"]["source_fingerprint"]

    # Request one entry at a time: cover pagination rather than relying on a large limit.
    primitives, offset = [], 0
    while True:
        page = client.call("get_drawing_geometries", {"page": 1, "offset": offset, "limit": 1})
        assert page["offset"] == offset and len(page["items"]) <= 1, page
        primitives.extend(page["items"])
        if page["next_offset"] is None:
            break
        assert page["next_offset"] > offset and page["next_offset"] <= page["total"], page
        offset = page["next_offset"]
    assert len(primitives) == page["total"] == catalog["primitive_count"]
    assert len({item["id"] for item in primitives}) == len(primitives)
    circles = [item for item in primitives if item["kind"] == "circle_candidate"]
    assert len(circles) == 1, primitives
    circle = circles[0]
    expected_circle = truth["vector_fixture"]["shapes"][0]
    assert abs(circle["circle"]["center"]["x"] - expected_circle["center_pt"][0]) < 0.1, circle
    assert abs(circle["circle"]["center"]["y"] - expected_circle["center_pt"][1]) < 0.1, circle
    assert abs(circle["circle"]["radius_pt"] - expected_circle["radius_pt"]) < 0.1, circle
    detail = client.call("get_drawing_geometries", {"page": 1, "geometry_id": circle["id"]})
    assert detail["total"] == 1 and detail["items"][0]["source"]["path_data"], detail
    assert len(detail["items"][0]["points"]) >= 16, detail
    filtered = client.call("get_drawing_geometries", {"page": 1, "kind": "circle_candidate", "limit": 1})
    assert filtered["total"] == 1 and filtered["items"][0]["id"] == circle["id"], filtered

    defined = client.call("define_drawing_view", {"expected_revision": catalog["document_revision"], "page": 1,
        "label": "Synthetic explicit source view", "feature_ids": [feature_id], "geometry_ids": [circle["id"]]})
    view = defined["view"]
    assert feature_id in view["feature_ids"] and circle["id"] in view["geometry_ids"]
    bindings = client.call("get_drawing_binding_candidates", {"feature_id": feature_id, "view_id": view["id"]})
    assert len(bindings["candidates"]) == 1, bindings
    candidate = bindings["candidates"][0]
    assert candidate["geometry_id"] == circle["id"] and candidate["feature_id"] == feature_id
    selected = client.call("select_drawing_binding", {"expected_revision": bindings["document_revision"],
        "feature_id": feature_id, "view_id": view["id"], "candidate_id": candidate["id"],
        "justification": "Synthetic page 1: the only circular vector contour and its separate diameter callout; selection is not human review."})
    assert selected["binding"]["reviewed"] is False

    # There is intentionally no geometry/coordinate or reviewed field in this API.
    args = {"capture_id": capture, "expected_revision": selected["document_revision"],
        "feature_id": feature_id, "group": "Synthetic bound circle", "options": options, "commit": False}
    before = client.call("get_measurement_context")
    client.call("inspect_bound_feature", args, expect_error=True)  # Existing raw registration has no view scope.
    assert client.call("get_measurement_context")["document_revision"] == before["document_revision"]
    scoped_landmarks = [
        {"plan": {"x": 50, "y": 30}, "image": {"x": 150, "y": 110}},
        {"plan": {"x": 90, "y": 30}, "image": {"x": 230, "y": 110}},
        {"plan": {"x": 50, "y": 80}, "image": {"x": 150, "y": 210}}]
    registration = {"capture_id": capture, "expected_revision": before["document_revision"],
        "maximum_error_px": 0.1, "correspondences": scoped_landmarks, "drawing_view_id": "unknown-synthetic-view"}
    client.call("register_inspection", registration, expect_error=True)
    assert client.call("get_measurement_context")["document_revision"] == before["document_revision"]
    registration["drawing_view_id"] = view["id"]
    state = client.call("register_inspection", registration)
    assert state["inspection"]["registration_scope"]["view_id"] == view["id"]
    assert abs(state["inspection"]["registration"]["transform"]["scale"] - 2.0) < 1e-9
    args["expected_revision"] = state["document_revision"]
    count_before = len(client.call("get_measurement_context")["measurements"])
    preview = client.call("inspect_bound_feature", args)
    assert preview["record"]["measurement_id"] is None
    assert preview["record"]["drawing_binding_id"] == candidate["id"]
    assert preview["record"]["outcome"]["geometry_state"] == "measured", preview
    assert preview["record"]["outcome"]["conformity_status"] == "indeterminate", preview
    assert "drawing_binding_unreviewed" in preview["record"]["outcome"]["blocking_reasons"]
    assert len(client.call("get_measurement_context")["measurements"]) == count_before
    args.update(expected_revision=preview["document_revision"], commit=True)
    committed = client.call("inspect_bound_feature", args)
    record = committed["record"]
    assert record["drawing_binding_id"] == candidate["id"]
    assert record["outcome"]["geometry_state"] == "measured", record
    assert record["outcome"]["conformity_status"] == "indeterminate", record
    assert "drawing_binding_unreviewed" in record["outcome"]["blocking_reasons"]
    assert abs(record["value_mm"] - truth["diameter_mm"]) < 0.02, record
    assert abs(record["value_mm"] - truth["drawing_nominal_mm"]) > 0.15
    observed = record["result"]["geometry"]["Circle"]
    assert abs(observed["radius"] - truth["radius_px"]) < 0.3, observed
    assert len(record["result"]["evidence"]["accepted_points"]) >= 80
    context = client.call("get_measurement_context")
    assert len(context["measurements"]) == count_before + 1
    native = next(item for item in context["measurements"] if item["id"] == record["measurement_id"])
    assert native["kind"] == "circle" and native["origin"] == "vision"
    center, edge = native["points_native_px"]
    native_diameter = 2 * math.hypot(edge["x"]-center["x"], edge["y"]-center["y"]) * truth["independent_mm_per_px"]
    assert abs(native_diameter-record["value_mm"]) < 1e-9
    client.call("save_project", {"path": str(folder / "bound_inspection.dperproj"), "overwrite": False})
    preparation = client.call("prepare_report", {"folder": str(folder)})
    selected_images = [item["key"] for item in preparation["items"]
        if item["measurement_count"] > 0 and item["name"].startswith("bound_inspection")]
    assert selected_images, preparation
    exported = client.call("export_report", {"capture_id": capture, "expected_revision": preparation["document_revision"],
        "folder": str(folder), "selected_image_keys": selected_images[:1], "format": "pdf",
        "destination": str(folder / "bound_inspection.pdf"), "include_audit_page": False,
        "persist_preferences": False, "overwrite": False})
    assert Path(exported["report_path"]).read_bytes().startswith(b"%PDF") and exported["manifest_written"]
    manifest = json.loads(Path(exported["manifest_path"]).read_text(encoding="utf-8-sig"))
    sessions = [picture["session"] for picture in manifest["pictures"]]
    assert any(any(item.get("drawing_binding_id") == candidate["id"]
        for item in session["automated_inspection"]["records"]) for session in sessions)
    assert any(session["automated_inspection"]["bindings"][0]["reviewed"] is False for session in sessions)

    context = client.call("get_measurement_context")
    source_child = client.call("add_measurements", {"capture_id": capture,
        "expected_revision": context["document_revision"], "measurements": [{
            "kind": "distance", "points": native["points_native_px"],
            "label": "Derived from PDF-bound observation", "source_measurement_ids": [record["measurement_id"]]}]})[0]
    assert source_child["dependency_stale"] is False
    # This deliberately changes only our generated source, keeping PDF byte length/xrefs valid.
    source = folder / "drawing.pdf"
    original = source.read_bytes()
    assert hashlib.sha256(original).hexdigest() == truth["drawing_sha256"], "Refus: source synthétique inattendue."
    modified = original.replace(b"SYNTHETIQUE", b"SYNTHETIQUF", 1)
    assert len(modified) == len(original) and modified != original
    with (folder / "drawing_before_source_change.pdf").open("xb") as backup:
        backup.write(original)
    source.write_bytes(modified)
    context = client.call("get_measurement_context")
    args.update(expected_revision=context["document_revision"], commit=False)
    client.call("inspect_bound_feature", args, expect_error=True)
    changed = client.call("get_inspection_state")
    changed_records = [item for item in changed["inspection"]["records"] if item.get("drawing_binding_id") == candidate["id"]]
    assert changed_records and all("drawing_source_changed" in item["outcome"]["blocking_reasons"] for item in changed_records)
    assert all(item["outcome"]["geometry_state"] != "validated" for item in changed_records)
    source_context = client.call("get_measurement_context")
    assert len(source_context["measurements"]) == count_before + 2
    suspended_child = next(item for item in source_context["measurements"] if item["id"] == source_child["id"])
    assert suspended_child["dependency_stale"] and suspended_child["components"] == []
    source_evidence = {"same_byte_length": True, "before_sha256": hashlib.sha256(original).hexdigest(),
        "after_sha256": hashlib.sha256(modified).hexdigest(), "source_modified": str(source),
        "bound_inspection_refused": True, "get_state_reason": "drawing_source_changed",
        "records": changed_records}
    (folder / "source_change_evidence.json").write_text(json.dumps(source_evidence, indent=2, ensure_ascii=False), encoding="utf-8")
    return {"vector_primitives_paginated": len(primitives), "vector_detail_preserves_source": True,
        "binding_candidate_id": candidate["id"], "binding_reviewed": False,
        "unscoped_and_unknown_view_rejected": True, "bound_inspection_without_geometry_inputs": True,
        "bound_measured_diameter_mm": record["value_mm"], "bound_unreviewed_geometry_state": "measured",
        "bound_unreviewed_conformity_status": "indeterminate", "bound_pdf_and_manifest_written": True,
        "same_length_drawing_change_rejected": True, "drawing_change_get_state_reason": "drawing_source_changed",
        "external_drawing_change_invalidates_derived_measurements": True}


def dependency_checks(folder, client):
    """Exercise dependency freshness, history, deletion, and safe project loading on fixtures only."""
    source = folder / "drawing.pdf"
    truth = json.loads((folder / "ground_truth.json").read_text(encoding="utf-8"))
    assert (folder / "drawing_before_source_change.pdf").is_file()
    assert hashlib.sha256(source.read_bytes()).hexdigest() != truth["drawing_sha256"]

    context = client.call("get_measurement_context")
    assert context["capture_id"] == "synthetic-smoke-capture" and context["image_available"]
    assert context["calibration"]["id"] == "synthetic-independent-reference"
    assert (folder / Path(context["image_source"]).name).is_file()
    state = client.call("analyze_drawing", {"path": str(source), "expected_revision": context["document_revision"]})
    plan = state["inspection"]["plan"]
    features = [item for item in plan["characteristics"]
        if item["kind"] == "diameter" and item["unit"] == "millimeter"]
    assert features, plan
    feature = features[0]
    capture = state["capture_id"]
    state = client.call("register_inspection", {"capture_id": capture,
        "expected_revision": state["document_revision"], "maximum_error_px": 0.1,
        "correspondences": [
            {"plan": {"x": 0, "y": 0}, "image": {"x": 50, "y": 50}},
            {"plan": {"x": 100, "y": 0}, "image": {"x": 250, "y": 50}},
            {"plan": {"x": 0, "y": 100}, "image": {"x": 50, "y": 250}}]})
    assert abs(state["inspection"]["registration"]["transform"]["scale"] - 2.0) < 1e-9

    context = client.call("get_measurement_context")
    root_result = client.call("add_measurements", {"capture_id": capture,
        "expected_revision": context["document_revision"], "measurements": [{
            "kind": "rectangle", "is_construction": True,
            "points": [{"x": 40, "y": 40}, {"x": 90, "y": 90}],
            "label": "Dependency root", "method": "Synthetic smoke root"}]})
    root = root_result[0]
    assert root["is_construction"] and root["source_measurement_ids"] == []
    assert root["dependency_stale"] is False
    root_id = root["id"]
    root_points = root["points_native_px"]

    context = client.call("get_measurement_context")
    sibling_result = client.call("add_measurements", {"capture_id": capture,
        "expected_revision": context["document_revision"], "measurements": [{
            "kind": "distance", "points": [{"x": 120, "y": 100}, {"x": 150, "y": 100}],
            "label": "Independent root", "method": "Synthetic independent root"}]})
    sibling = sibling_result[0]
    assert sibling["source_measurement_ids"] == [] and sibling["dependency_stale"] is False
    sibling_id = sibling["id"]

    args = {"capture_id": capture, "expected_revision": client.call("get_measurement_context")["document_revision"],
        "feature_id": feature["id"], "group": "Synthetic dependency circle",
        "geometry_in_drawing": {"Circle": {"center": {"x": 75, "y": 55}, "radius": 29.0}},
        "options": dict(SMOKE_INSPECTION_OPTIONS), "commit": True,
        "source_measurement_ids": [root_id]}
    committed = client.call("inspect_feature", args)
    circle_record = committed["record"]
    circle_id = circle_record["measurement_id"]
    assert circle_id is not None
    context = client.call("get_measurement_context")
    circle = next(item for item in context["measurements"] if item["id"] == circle_id)
    assert circle["source_measurement_ids"] == [root_id] and circle["dependency_stale"] is False

    child_points = circle["points_native_px"]
    context = client.call("get_measurement_context")
    grandchild_result = client.call("add_measurements", {"capture_id": capture,
        "expected_revision": context["document_revision"], "measurements": [{
            "kind": "distance", "points": child_points, "label": "Synthetic grandchild",
            "method": "Derived from observed circle", "source_measurement_ids": [circle_id]}]})
    grandchild = grandchild_result[0]
    grandchild_id = grandchild["id"]
    assert grandchild["source_measurement_ids"] == [circle_id] and grandchild["dependency_stale"] is False

    context = client.call("get_measurement_context")
    measurements_before = {item["id"]: item for item in context["measurements"]}
    assert measurements_before[root_id]["dependency_stale"] is False
    assert measurements_before[circle_id]["dependency_stale"] is False
    assert measurements_before[grandchild_id]["dependency_stale"] is False
    assert measurements_before[sibling_id]["dependency_stale"] is False
    moved_root_points = [root_points[0], {"x": root_points[1]["x"] + 10.0, "y": root_points[1]["y"]}]
    updated = client.call("update_measurements", {"capture_id": capture,
        "expected_revision": context["document_revision"],
        "updates": [{"id": root_id, "points": moved_root_points}]})
    assert updated[0]["id"] == root_id and updated[0]["dependency_stale"] is False
    stale_context = client.call("get_measurement_context")
    stale_measurements = {item["id"]: item for item in stale_context["measurements"]}
    for measurement_id in [circle_id, grandchild_id]:
        assert stale_measurements[measurement_id]["dependency_stale"] is True
        assert stale_measurements[measurement_id]["components"] == []
    assert stale_measurements[sibling_id]["dependency_stale"] is False
    assert stale_measurements[sibling_id]["components"] == measurements_before[sibling_id]["components"]
    inspection = client.call("get_inspection_state")
    stale_record = next(item for item in inspection["inspection"]["records"]
        if item.get("measurement_id") == circle_id)
    assert "measurement_revision_changed" in stale_record["outcome"]["blocking_reasons"], stale_record

    def history(action):
        current = client.call("get_measurement_context")
        assert current["capture_id"] == capture
        return client.call("measurement_history", {"action": action,
            "expected_revision": current["document_revision"]})

    undone = history("undo")
    undo_measurements = {item["id"]: item for item in undone["measurements"]}
    assert undo_measurements[root_id]["points_native_px"] == root_points
    for measurement_id in [circle_id, grandchild_id]:
        assert undo_measurements[measurement_id]["dependency_stale"] is True
        assert undo_measurements[measurement_id]["components"] == []
    assert undo_measurements[sibling_id]["dependency_stale"] is False
    undo_state = client.call("get_inspection_state")
    undo_record = next(item for item in undo_state["inspection"]["records"]
        if item.get("measurement_id") == circle_id)
    assert undo_record["outcome"]["geometry_state"] != "validated"
    redone = history("redo")
    redo_measurements = {item["id"]: item for item in redone["measurements"]}
    assert redo_measurements[root_id]["points_native_px"] == moved_root_points
    for measurement_id in [circle_id, grandchild_id]:
        assert redo_measurements[measurement_id]["dependency_stale"] is True
        assert redo_measurements[measurement_id]["components"] == []
    assert redo_measurements[sibling_id]["dependency_stale"] is False
    redo_state = client.call("get_inspection_state")
    redo_record = next(item for item in redo_state["inspection"]["records"]
        if item.get("measurement_id") == circle_id)
    assert redo_record["outcome"]["geometry_state"] != "validated"

    before_parent_delete = client.call("get_measurement_context")
    client.call("delete_measurements", {"capture_id": capture,
        "expected_revision": before_parent_delete["document_revision"], "ids": [root_id]}, expect_error=True)
    after_parent_delete = client.call("get_measurement_context")
    assert after_parent_delete["document_revision"] == before_parent_delete["document_revision"]
    assert after_parent_delete["measurements"] == before_parent_delete["measurements"]

    context = after_parent_delete
    client.call("delete_measurements", {"capture_id": capture,
        "expected_revision": context["document_revision"], "ids": [circle_id, grandchild_id]})
    first_undo = history("undo")
    first_undo_by_id = {item["id"]: item for item in first_undo["measurements"]}
    assert circle_id in first_undo_by_id and grandchild_id not in first_undo_by_id
    second_undo = history("undo")
    second_undo_by_id = {item["id"]: item for item in second_undo["measurements"]}
    assert circle_id in second_undo_by_id and grandchild_id in second_undo_by_id
    for measurement_id in [circle_id, grandchild_id]:
        assert second_undo_by_id[measurement_id]["dependency_stale"] is True
        assert second_undo_by_id[measurement_id]["components"] == []
    assert second_undo_by_id[root_id]["points_native_px"] == moved_root_points

    valid_project = folder / "dependency_graph.dperproj"
    client.call("save_project", {"path": str(valid_project), "overwrite": False})
    saved_project = json.loads(valid_project.read_text(encoding="utf-8-sig"))
    future_project = json.loads(json.dumps(saved_project))
    future_project["schema_version"] = 999
    future_path = folder / "dependency_future_schema.dperproj"
    future_path.write_text(json.dumps(future_project, indent=2), encoding="utf-8")
    cycle_project = json.loads(json.dumps(saved_project))
    cycle_measurements = {item["id"]: item for item in cycle_project["measurements"]}
    cycle_measurements[root_id]["source_measurement_ids"] = [circle_id]
    cycle_path = folder / "dependency_cycle.dperproj"
    cycle_path.write_text(json.dumps(cycle_project, indent=2), encoding="utf-8")

    before_rejections = client.call("get_measurement_context")
    for bad_project in [future_path, cycle_path]:
        client.call("open_project", {"path": str(bad_project)}, expect_error=True)
        after_rejection = client.call("get_measurement_context")
        assert after_rejection["document_revision"] == before_rejections["document_revision"]
        assert after_rejection["capture_id"] == before_rejections["capture_id"]
        assert after_rejection["measurements"] == before_rejections["measurements"]

    forged_freshness = json.loads(json.dumps(saved_project))
    for measurement in forged_freshness["measurements"]:
        measurement["dependency_stale"] = False
    forged_path = folder / "dependency_forged_freshness.dperproj"
    forged_path.write_text(json.dumps(forged_freshness, indent=2), encoding="utf-8")
    reopened = client.call("open_project", {"path": str(forged_path)})
    reopened_measurements = {item["id"]: item for item in reopened["measurements"]}
    for measurement_id in [circle_id, grandchild_id]:
        assert reopened_measurements[measurement_id]["dependency_stale"] is True
        assert reopened_measurements[measurement_id]["components"] == []
    assert reopened_measurements[sibling_id]["dependency_stale"] is False
    historical_vision = next(item for item in reopened["measurements"]
        if item["origin"] == "vision" and not item["source_measurement_ids"])
    client.call("add_measurements", {"capture_id": capture, "expected_revision": reopened["document_revision"],
        "measurements": [{"kind": "distance", "points": child_points,
            "source_measurement_ids": [historical_vision["id"]]}]}, expect_error=True)
    assert client.call("get_measurement_context")["document_revision"] == reopened["document_revision"]
    client.call("save_project", {"path": str(folder / "dependency_report.dperproj"), "overwrite": False})
    preparation = client.call("prepare_report", {"folder": str(folder)})
    selected = [item["key"] for item in preparation["items"]
        if item["measurement_count"] > 0 and item["name"].startswith("dependency_report.")]
    assert selected, preparation
    exported = client.call("export_report", {"capture_id": capture,
        "expected_revision": preparation["document_revision"], "folder": str(folder),
        "selected_image_keys": selected[:1], "format": "pdf", "include_audit_page": False,
        "destination": str(folder / "dependency_report.pdf"), "persist_preferences": False, "overwrite": False})
    assert exported["manifest_written"] and Path(exported["report_path"]).read_bytes().startswith(b"%PDF")
    parsed = subprocess.run(["pdftotext", "-layout", exported["report_path"], "-"], capture_output=True)
    assert parsed.returncode == 0 and not parsed.stderr, parsed.stderr
    assert "recalculer" in parsed.stdout.decode("utf-8"), "Notice de recalcul absente du PDF."
    return {"measurement_dependencies_propagate_stale": True,
        "manual_root_and_independent_sibling_preserved": True,
        "inspection_revision_change_reason": "measurement_revision_changed",
        "undo_redo_restore_geometry_without_refreshing_proof": True,
        "parent_only_deletion_rejected_without_revision_change": True,
        "batch_delete_undo_restores_parents_before_children": True,
        "future_schema_and_dependency_cycle_rejected_without_context_change": True,
        "imported_dependency_freshness_recomputed": True,
        "imported_vision_parent_requires_new_observation": True,
        "dependency_pdf_and_manifest_written": True,
        "dependency_pdf_parser_clean_and_recompute_notice_present": True}


def registration_landmark_checks(folder, client):
    """Persist the actual ordered landmarks, including the independently rejected point."""
    context = client.call("get_measurement_context")
    capture, calibration_before = context["capture_id"], context["calibration"]
    pairs = [
        {"plan": {"x": 0, "y": 0}, "image": {"x": 50, "y": 50}},
        {"plan": {"x": 100, "y": 0}, "image": {"x": 250, "y": 50}},
        {"plan": {"x": 0, "y": 100}, "image": {"x": 50, "y": 250}},
        {"plan": {"x": 100, "y": 100}, "image": {"x": 250, "y": 250}},
        {"plan": {"x": 50, "y": 50}, "image": {"x": 350, "y": 50}},
    ]
    state = client.call("register_inspection", {"capture_id": capture,
        "expected_revision": context["document_revision"], "correspondences": pairs, "maximum_error_px": 0.1})
    report = state["inspection"]["registration"]
    assert report["correspondences"] == pairs
    assert report["inlier_indices"] == [0, 1, 2, 3]
    assert report["threshold_px"] == 0.1 and len(report["residuals_px"]) == 5
    assert all(report["residuals_px"][index] <= 0.1 for index in report["inlier_indices"])
    assert report["residuals_px"][4] > 200
    for index, pair in enumerate(pairs):
        expected = math.hypot(2 * pair["plan"]["x"] + 50 - pair["image"]["x"],
                              2 * pair["plan"]["y"] + 50 - pair["image"]["y"])
        assert abs(report["residuals_px"][index] - expected) < 1e-9
    project = folder / "registration_landmarks.dperproj"
    client.call("save_project", {"path": str(project), "overwrite": False})
    saved = json.loads(project.read_text(encoding="utf-8-sig"))
    assert saved["schema_version"] == 15
    assert saved["automated_inspection"]["registration"]["correspondences"] == pairs
    client.call("open_project", {"path": str(project)})
    restored = client.call("get_inspection_state")["inspection"]["registration"]
    assert restored["correspondences"] == pairs and restored["inlier_indices"] == report["inlier_indices"]
    assert restored["threshold_px"] == report["threshold_px"]
    assert len(restored["residuals_px"]) == len(report["residuals_px"]) == len(pairs)
    assert all(abs(a-b) < 1e-9 for a, b in zip(restored["residuals_px"], report["residuals_px"]))
    assert client.call("get_measurement_context")["calibration"] == calibration_before
    preparation = client.call("prepare_report", {"folder": str(folder)})
    selected = [item["key"] for item in preparation["items"]
        if item["measurement_count"] > 0 and item["name"].startswith("registration_landmarks.")]
    assert selected, preparation
    exported = client.call("export_report", {"capture_id": capture,
        "expected_revision": preparation["document_revision"], "folder": str(folder),
        "selected_image_keys": selected[:1], "format": "pdf", "include_audit_page": False,
        "destination": str(folder / "registration_landmarks.pdf"), "persist_preferences": False, "overwrite": False})
    assert exported["manifest_written"]
    manifest = json.loads(Path(exported["manifest_path"]).read_text(encoding="utf-8-sig"))
    assert any(picture["session"]["automated_inspection"]["registration"]["correspondences"] == pairs
               for picture in manifest["pictures"])
    for picture in manifest["pictures"]:
        registration = picture["session"]["automated_inspection"]["registration"]
        assert registration["inlier_indices"] == report["inlier_indices"]
        assert registration["threshold_px"] == report["threshold_px"]
        assert len(registration["residuals_px"]) == len(pairs)
        assert all(abs(a-b) < 1e-9 for a, b in zip(registration["residuals_px"], report["residuals_px"]))
    return {"registration_original_landmarks_preserved": True,
        "registration_inlier_indices_resolve_to_original_coordinates": True,
        "registration_all_point_residuals_preserved": True,
        "registration_project_reload_preserves_landmarks": True,
        "registration_manifest_preserves_landmarks": True,
        "registration_scale_does_not_change_independent_calibration": True,
        "registration_rejected_landmark_count": 1, "registration_retained_landmark_count": 4}


def run(folder, port):
    assert 1 <= port <= 65535 and port != 8765, "Refus: utiliser un port de test distinct du port utilisateur 8765."
    truth = json.loads((folder / "ground_truth.json").read_text(encoding="utf-8"))
    assert truth.get("synthetic_only") is True and truth.get("fixture_version") == 2, "Refus: dossier non identifié comme fixture synthétique courante."
    assert hashlib.sha256((folder / "drawing.pdf").read_bytes()).hexdigest() == truth["drawing_sha256"], "Refus: source synthétique modifiée avant le test."
    client = Mcp(folder, port)
    catalog = client.rpc("tools/list", {})
    names = {tool["name"] for tool in catalog["tools"]}
    required = {"analyze_drawing", "propose_drawing_interpretation", "get_inspection_state", "register_inspection", "inspect_feature",
        "analyze_drawing_geometry", "get_drawing_geometries", "define_drawing_view",
        "get_drawing_binding_candidates", "select_drawing_binding", "inspect_bound_feature",
        "get_measurement_context", "add_measurements", "update_measurements", "delete_measurements",
        "measurement_history", "open_project", "save_project"}
    assert required <= names, names
    initial = client.call("get_measurement_context")
    if initial["image_available"]:
        assert initial["capture_id"] == "synthetic-smoke-capture", "Refus: cette instance possède une autre capture."
        # The application intentionally exposes only the image basename in MCP context.
        assert (folder / Path(initial["image_source"]).name).is_file(), "Refus: source de test inconnue."
        assert initial["calibration"]["id"] == "synthetic-independent-reference", "Refus: autre calibration active."
        recovery = folder / f"recovery_{client.log_path.stem}.dperproj"
        client.call("save_project", {"path": str(recovery), "overwrite": False})
    context = client.call("open_project", {"path": str(folder / "fixture.dperproj")})
    assert context["image_quality_assessment"]["is_exploitable"], context["image_quality_assessment"]
    state = client.call("analyze_drawing", {"path": str(folder / "drawing.pdf"), "expected_revision": context["document_revision"]})
    plan = state["inspection"]["plan"]
    candidates = [item for item in plan["characteristics"] if item["kind"] == "diameter" and item["unit"] == "millimeter"]
    assert len(candidates) == 1, plan
    feature = candidates[0]
    assert abs(feature["nominal"] - 4.0) < 1e-9 and not feature["confirmed"]
    state = client.call("propose_drawing_interpretation", {"expected_revision": state["document_revision"],
        "feature_id": feature["id"], "kind": "diameter", "unit": "millimeter",
        "source_note": "Synthetic PDF page 1: explicit diameter symbol and mm suffix."})
    reviewed = next(item for item in state["inspection"]["plan"]["characteristics"] if item["id"] == feature["id"])
    assert reviewed["source"]["exact_text"] == feature["source"]["exact_text"]
    assert reviewed["interpretation_notes"] and not reviewed["confirmed"]
    capture = state["capture_id"]
    state = client.call("register_inspection", {"capture_id": capture, "expected_revision": state["document_revision"],
        "maximum_error_px": 0.1, "correspondences": [
            {"plan": {"x": 0, "y": 0}, "image": {"x": 50, "y": 50}},
            {"plan": {"x": 100, "y": 0}, "image": {"x": 250, "y": 50}},
            {"plan": {"x": 0, "y": 100}, "image": {"x": 50, "y": 250}}]})
    options = dict(SMOKE_INSPECTION_OPTIONS)
    args = {"capture_id": capture, "expected_revision": state["document_revision"], "feature_id": feature["id"],
            "group": "Synthetic circular contour", "geometry_in_drawing": {"Circle": {"center": {"x": 75, "y": 55}, "radius": 29.0}},
            "options": options, "commit": False}
    guarded_context = client.call("get_measurement_context")
    # A client-provided confidence or historical proof cannot supply missing pixels.
    fabricated = dict(args, geometry_in_drawing={"Circle": {"center": {"x": 0, "y": 30}, "radius": 5.0}},
        confidence=1.0, evidence={"coverage_fraction": 1.0, "rms_residual_px": 0.0},
        outcome={"geometry_state": "validated", "conformity_status": "conforming"})
    client.call("inspect_feature", fabricated, expect_error=True)
    client.call("inspect_feature", dict(args, capture_id="other-synthetic-capture"), expect_error=True)
    guarded_after = client.call("get_measurement_context")
    assert guarded_after["document_revision"] == guarded_context["document_revision"]
    assert guarded_after["measurements"] == guarded_context["measurements"]
    assert not client.call("get_inspection_state")["inspection"]["records"]
    preview = client.call("inspect_feature", args)
    assert preview["record"]["measurement_id"] is None
    assert not client.call("get_measurement_context")["measurements"]
    args.update(expected_revision=preview["document_revision"], commit=True)
    committed = client.call("inspect_feature", args)
    record = committed["record"]
    assert abs(record["value_mm"] - 4.2) < 0.02, record
    assert abs(record["value_mm"] - feature["nominal"]) > 0.15, "Le nominal a remplacé l'observation."
    assert record["outcome"]["conformity_status"] != "conforming"
    context = client.call("get_measurement_context")
    native = next(item for item in context["measurements"] if item["id"] == record["measurement_id"])
    assert native["kind"] == "circle" and native["origin"] == "vision"
    assert len(record["result"]["evidence"]["accepted_points"]) >= 80
    center, edge = native["points_native_px"]
    native_diameter = 2 * math.hypot(edge["x"]-center["x"], edge["y"]-center["y"]) * 0.035
    assert abs(native_diameter-record["value_mm"]) < 1e-9
    construction = client.call("add_measurements", {"capture_id": capture, "expected_revision": context["document_revision"],
        "measurements": [{"kind": "rectangle", "is_construction": True, "points": [{"x": 100, "y": 60}, {"x": 300, "y": 260}],
                          "label": "Construction synthétique", "method": "Visual reference only"}]})
    assert construction[0]["is_construction"] and not construction[0]["components"]
    stale_revision = context["document_revision"]
    args["expected_revision"] = stale_revision
    client.call("inspect_feature", args, expect_error=True)
    assert len(client.call("get_measurement_context")["measurements"]) == 2
    saved = client.call("save_project", {"path": str(folder / "inspection.dperproj"), "overwrite": False})
    preparation = client.call("prepare_report", {"folder": str(folder)})
    selected = [item["key"] for item in preparation["items"] if item["measurement_count"] > 0 and item["name"].startswith("inspection")]
    assert selected, preparation
    exported = client.call("export_report", {"capture_id": capture, "expected_revision": preparation["document_revision"],
        "folder": str(folder), "selected_image_keys": selected[:1], "format": "pdf", "destination": str(folder / "inspection.pdf"),
        "include_audit_page": False, "persist_preferences": False, "overwrite": False})
    pdf_path = Path(exported["report_path"])
    assert pdf_path.read_bytes().startswith(b"%PDF") and exported["manifest_written"]
    manifest = json.loads(Path(exported["manifest_path"]).read_text(encoding="utf-8-sig"))
    assert "automated_inspection" in json.dumps(manifest), "Preuves absentes du manifeste."
    parsed_pdf = subprocess.run(["pdftotext", str(pdf_path), str(folder / "inspection.txt")], check=True, capture_output=True, text=True)
    assert "Syntax Error" not in parsed_pdf.stderr, parsed_pdf.stderr
    text = (folder / "inspection.txt").read_text(encoding="utf-8").casefold()
    assert "vision" in text and "suspendue" in text, "Limites de validation absentes du PDF."
    context = client.call("get_measurement_context")
    client.call("update_measurements", {"capture_id": capture, "expected_revision": context["document_revision"],
        "updates": [{"id": native["id"], "points": [center, {"x": center["x"]+30, "y": center["y"]}]}]})
    changed = client.call("get_inspection_state")
    changed_record = next(item for item in changed["inspection"]["records"] if item["measurement_id"] == native["id"])
    assert changed_record["outcome"]["geometry_state"] != "validated"
    client.call("save_project", {"path": str(folder / "changed.dperproj"), "overwrite": False})
    client.call("open_project", {"path": str(folder / "inspection.dperproj")})
    reopened = client.call("get_inspection_state")
    reopened_record = next(item for item in reopened["inspection"]["records"] if item["measurement_id"] == native["id"])
    assert reopened_record["imported"] and reopened_record["outcome"]["geometry_state"] != "validated"
    bound_summary = run_bound_workflow(client, folder, feature["id"], options, truth)
    dependency_summary = dependency_checks(folder, client)
    registration_summary = registration_landmark_checks(folder, client)
    from adversarial_vision_checks import run_adversarial_checks
    adversarial_summary = run_adversarial_checks(folder, client, options)
    summary = {"passed": True, "tools_discovered": sorted(required), "measured_diameter_mm": record["value_mm"],
               "expected_diameter_mm": 4.2, "nominal_mm": 4.0, "native_matches_robust_fit": True,
               "construction_has_no_measurement_components": True, "stale_revision_rejected": True,
               "fabricated_client_evidence_rejected": True, "wrong_capture_rejected": True,
               "failed_inspections_preserve_revision_and_measurements": True,
               "manual_edit_invalidates": True, "reloaded_evidence_is_historical": True,
               "pdf_and_manifest_written": True, "pdf_parser_clean": True, "physical_qualification": "not_performed"}
    summary.update(bound_summary)
    summary.update(dependency_summary)
    summary.update(registration_summary)
    summary.update(adversarial_summary)
    (folder / "smoke_summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(json.dumps(summary, ensure_ascii=False))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["fixtures", "run"])
    parser.add_argument("folder", type=Path)
    parser.add_argument("--port", type=int, default=18765)
    arguments = parser.parse_args()
    if arguments.mode == "fixtures":
        fixtures(arguments.folder.resolve())
    else:
        run(arguments.folder.resolve(), arguments.port)
