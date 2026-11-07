"""Synthetic per-occurrence MCP checks, called by smoke_inspection_mcp.

The caller owns the dedicated test instance. All generated artifacts stay in
the fresh occurrence-checks subdirectory of its synthetic output folder.
"""
import copy
import hashlib
import json
import math
from pathlib import Path
import subprocess


def _fixtures(folder):
    from PIL import Image, ImageDraw
    from generate_vector_drawing_fixtures import Drawing

    destination = folder / "occurrence-checks"
    destination.mkdir(exist_ok=False)
    scale = 4
    image = Image.new("RGB", (400 * scale, 320 * scale), (190, 190, 190))
    raster = ImageDraw.Draw(image)
    centers = [(125, 160), (275, 160)]
    radii = [46, 50]
    for (x, y), radius in zip(centers, radii):
        raster.ellipse(tuple(value * scale for value in
            (x - radius, y - radius, x + radius, y + radius)), fill=(40, 40, 40))
    # Keep the smoke fixture's textured corners for its acquisition-quality gate.
    for bx, by in [(10, 10), (350, 10), (10, 260), (350, 260)]:
        for iy in range(8):
            for ix in range(8):
                shade = 55 if (ix + iy) % 2 else 215
                raster.rectangle(((bx + ix * 5) * scale, (by + iy * 5) * scale,
                    (bx + (ix + 1) * 5) * scale - 1,
                    (by + (iy + 1) * 5) * scale - 1), fill=(shade,) * 3)
    drawing = Drawing()
    drawing.text(36, 48, "2×Ø3.36 ±0.10 mm")
    for x in (60, 135):
        drawing.circle(x, 75, 24)
    landmarks = [(90, 62), (104, 62), (90, 92)]
    for x, y in landmarks:
        drawing.polyline([(x - 1, y), (x + 1, y)])
        drawing.polyline([(x, y - 1), (x, y + 1)])
        image_x, image_y = 2 * x + 5, 2 * y + 10
        raster.line(((image_x - 2) * scale, image_y * scale,
                     (image_x + 2) * scale, image_y * scale), fill=(40, 40, 40), width=scale)
        raster.line((image_x * scale, (image_y - 2) * scale,
                     image_x * scale, (image_y + 2) * scale), fill=(40, 40, 40), width=scale)
    image.resize((400, 320), Image.Resampling.LANCZOS).save(destination / "two_occurrences.png")
    pdf = drawing.pdf_bytes()
    (destination / "two_occurrences.pdf").write_bytes(pdf)
    template = json.loads((folder / "fixture.dperproj").read_text(encoding="utf-8"))
    calibration = copy.deepcopy(template["calibration"])
    assert calibration["id"] == "synthetic-independent-reference"
    assert calibration["resolution"] == [400, 320]
    assert calibration["mm_per_pixel"] == 0.035
    project = {"schema_version": template["schema_version"],
        "capture_id": "synthetic-occurrence-capture", "image_file": "two_occurrences.png",
        "device": "Synthetic fixture", "device_id": "synthetic", "optical_setup": "Synthetic 2D plane",
        "calibration": calibration, "calibrations": [calibration], "measurements": []}
    (destination / "fixture.dperproj").write_text(json.dumps(project, indent=2), encoding="utf-8")
    truth = {"synthetic_only": True, "pdf_sha256": hashlib.sha256(pdf).hexdigest(),
        "independent_mm_per_px": 0.035, "drawing_nominal_mm": 3.36,
        "drawing_radius_pt": 24, "registration_scale": 2,
        "registration_translation_px": [5, 10], "landmarks_plan": landmarks,
        "occurrences_left_to_right": [
            {"center_px": [(value * scale + 0.5) / scale - 0.5 for value in center],
             "radius_px": radius, "diameter_mm": 2 * radius * 0.035}
            for center, radius in zip(centers, radii)]}
    (destination / "ground_truth.json").write_text(
        json.dumps(truth, indent=2, ensure_ascii=False), encoding="utf-8")
    return destination, truth


def _reject_without_mutation(client, arguments):
    before_context = client.call("get_measurement_context")
    before_inspection = client.call("get_inspection_state")
    client.call("inspect_bound_feature", arguments, expect_error=True)
    assert client.call("get_measurement_context") == before_context
    assert client.call("get_inspection_state") == before_inspection


def _assert_observation(record, member, expected, truth, view_id):
    proof = record["occurrence"]
    assert proof["member"] == member
    assert proof["view_id"] == view_id and proof["assignment_fingerprint"]
    assert proof["source_measurement_ids"] == []
    assert record["drawing_binding_id"] is None
    assert record["outcome"]["conformity_status"] == "indeterminate"
    assert "drawing_occurrence_group_unassessed" in record["outcome"]["blocking_reasons"]
    assert not {"drawing_occurrence_assignment_changed", "source_pixels_changed",
        "registration_missing_or_stale", "calibration_changed"}.intersection(record["outcome"]["blocking_reasons"])
    observed = record["result"]["geometry"]["Circle"]
    center = observed["center"]
    assert math.hypot(center["x"] - expected["center_px"][0],
                      center["y"] - expected["center_px"][1]) < 0.5, observed
    assert abs(observed["radius"] - expected["radius_px"]) < 0.3, observed
    assert abs(observed["radius"] - truth["drawing_radius_pt"] * truth["registration_scale"]) > 1.5
    assert abs(record["value_mm"] - expected["diameter_mm"]) < 0.02
    assert abs(record["value_mm"] - truth["drawing_nominal_mm"]) > 0.1
    assert abs(record["value_mm"] - 2 * observed["radius"] * truth["independent_mm_per_px"]) < 1e-9
    evidence = record["result"]["evidence"]
    assert len(evidence["accepted_points"]) >= 80
    assert evidence["coverage_fraction"] >= 0.8 and evidence["rms_residual_px"] <= 0.5
    other = next(item for item in truth["occurrences_left_to_right"] if item != expected)
    for point in evidence["accepted_points"]:
        own_distance = math.hypot(point["x"] - expected["center_px"][0], point["y"] - expected["center_px"][1])
        other_distance = math.hypot(point["x"] - other["center_px"][0], point["y"] - other["center_px"][1])
        assert abs(own_distance - expected["radius_px"]) < 1.0
        assert own_distance < other_distance


def _assert_roundtrip_result(restored, original):
    """JSON float parsing can differ by a final bit; preserve every sample."""
    restored_circle = restored["geometry"]["Circle"]
    original_circle = original["geometry"]["Circle"]
    assert set(restored["geometry"]) == set(original["geometry"]) == {"Circle"}
    for key in ("x", "y"):
        assert abs(restored_circle["center"][key] - original_circle["center"][key]) < 1e-12
    assert abs(restored_circle["radius"] - original_circle["radius"]) < 1e-12
    restored_evidence, original_evidence = restored["evidence"], original["evidence"]
    for key in ("sample_count", "endpoint_extent_verified"):
        assert restored_evidence[key] == original_evidence[key]
    for key in ("rms_residual_px", "max_residual_px", "coverage_fraction"):
        assert abs(restored_evidence[key] - original_evidence[key]) < 1e-12
    for key in ("accepted_points", "rejected_points"):
        assert len(restored_evidence[key]) == len(original_evidence[key])
        for actual, expected in zip(restored_evidence[key], original_evidence[key]):
            assert set(actual) == set(expected) == {"x", "y"}
            assert abs(actual["x"] - expected["x"]) < 1e-12
            assert abs(actual["y"] - expected["y"]) < 1e-12


def run_occurrence_checks(folder, client, options):
    destination, truth = _fixtures(Path(folder))
    opened = client.call("open_project", {"path": str(destination / "fixture.dperproj")})
    capture = opened["capture_id"]
    calibration_before = client.call("get_measurement_context")["calibration"]
    state = client.call("analyze_drawing", {"path": str(destination / "two_occurrences.pdf"),
        "expected_revision": opened["document_revision"]})
    groups = state["drawing_structure"]["groups"]
    assert len(groups) == 1 and groups[0]["expected_count"] == 2
    feature_id = groups[0]["declaration_id"]
    catalog = client.call("analyze_drawing_geometry", {"page": 1,
        "expected_revision": state["document_revision"]})
    primitives = client.call("get_drawing_geometries", {"page": 1})["items"]
    circles = sorted((item for item in primitives if item["kind"] == "circle_candidate"),
        key=lambda item: item["circle"]["center"]["x"])
    assert len(circles) == 2
    assert all(abs(item["circle"]["radius_pt"] - 24) < 0.1 for item in circles)
    view = client.call("define_drawing_view", {"expected_revision": catalog["document_revision"],
        "page": 1, "label": "Deux occurrences synthétiques distinctes", "feature_ids": [feature_id],
        "geometry_ids": [item["id"] for item in primitives]})["view"]
    candidates = client.call("get_drawing_binding_candidates", {"feature_id": feature_id, "view_id": view["id"]})
    by_geometry = {item["geometry_id"]: item["id"] for item in candidates["candidates"]}
    # Numbering deliberately opposes spatial order to catch index-based routing.
    chosen = [by_geometry[item["id"]] for item in reversed(circles)]
    assignment_args = {"feature_id": feature_id, "view_id": view["id"],
        "candidate_ids": chosen, "justification": "Deux cercles synthétiques, ordre explicite droite puis gauche."}
    assigned = client.call("assign_drawing_occurrences", dict(assignment_args,
        expected_revision=candidates["document_revision"]))
    members = assigned["assignment"]["members"]
    assert [item["number"] for item in members] == [1, 2]
    assert [item["geometry_id"] for item in members] == [item["id"] for item in reversed(circles)]
    correspondences = [{"plan": {"x": x, "y": y}, "image": {"x": 2 * x + 5, "y": 2 * y + 10}}
        for x, y in truth["landmarks_plan"]]
    registration_args = {"capture_id": capture, "maximum_error_px": 0.1,
        "correspondences": correspondences}
    # A numerically correct unscoped registration must still refuse observation.
    unscoped = client.call("register_inspection", dict(registration_args,
        expected_revision=assigned["document_revision"]))
    base_args = {"capture_id": capture, "feature_id": feature_id, "group": "Deux occurrences observées",
        "source_measurement_ids": [], "options": dict(options), "commit": False}
    _reject_without_mutation(client, dict(base_args, expected_revision=unscoped["document_revision"],
        occurrence_id=members[0]["id"]))
    registered = client.call("register_inspection", dict(registration_args, drawing_view_id=view["id"],
        expected_revision=unscoped["document_revision"]))
    assert abs(registered["inspection"]["registration"]["transform"]["scale"] - 2) < 1e-9
    assert registered["inspection"]["registration_scope"]["view_id"] == view["id"]
    assert client.call("get_measurement_context")["calibration"] == calibration_before
    revision = registered["document_revision"]
    _reject_without_mutation(client, dict(base_args, expected_revision=revision))
    _reject_without_mutation(client, dict(base_args, expected_revision=revision, occurrence_id="unknown-occurrence"))
    _reject_without_mutation(client, dict(base_args, expected_revision=revision,
        occurrence_id=members[0]["id"], commit=True))

    committed_records = []
    native_ids = []
    accepted_sets = []
    for index, (member, expected) in enumerate(zip(members, reversed(truth["occurrences_left_to_right"]))):
        before = client.call("get_measurement_context")
        args = dict(base_args, occurrence_id=member["id"], expected_revision=before["document_revision"])
        # The first occurrence's preview cannot authorize the other occurrence.
        preview = client.call("inspect_bound_feature", args)
        record = preview["record"]
        _assert_observation(record, member, expected, truth, view["id"])
        assert record["source_fingerprint"] == registered["inspection"]["registration_fingerprint"]
        assert record["calibration_id"] == calibration_before["id"]
        assert record["calibration_fingerprint"] is not None
        assert record["measurement_id"] is None
        assert record["occurrence"]["preview_revision"] == preview["document_revision"]
        assert len(client.call("get_measurement_context")["measurements"]) == index
        if index == 0:
            _reject_without_mutation(client, dict(base_args, occurrence_id=members[1]["id"],
                expected_revision=preview["document_revision"], commit=True))
        result = client.call("inspect_bound_feature", dict(args,
            expected_revision=preview["document_revision"], commit=True))
        record = result["record"]
        _assert_observation(record, member, expected, truth, view["id"])
        assert record["result"] == preview["record"]["result"]
        assert record["source_fingerprint"] == preview["record"]["source_fingerprint"]
        assert record["measurement_id"] is not None and record["measurement_id"] not in native_ids
        assert record["occurrence"]["preview_revision"] == preview["document_revision"]
        context = client.call("get_measurement_context")
        assert len(context["measurements"]) == index + 1
        native = next(item for item in context["measurements"] if item["id"] == record["measurement_id"])
        assert native["origin"] == "vision" and native["kind"] == "circle"
        assert native["source_measurement_ids"] == []
        center, edge = native["points_native_px"]
        assert abs(2 * math.hypot(edge["x"] - center["x"], edge["y"] - center["y"])
            * truth["independent_mm_per_px"] - record["value_mm"]) < 1e-9
        native_ids.append(native["id"])
        committed_records.append(record)
        accepted_sets.append({(point["x"], point["y"]) for point in record["result"]["evidence"]["accepted_points"]})
        current = client.call("get_inspection_state")
        observation_group = current["drawing_occurrence_observations"][0]
        assert len(observation_group["observed_occurrence_ids"]) == index + 1
        assert observation_group["conformity"] == "indeterminate"
        committed = [item for item in current["inspection"]["records"] if item["measurement_id"] is not None]
        assert len(committed) == index + 1
        assert all(item["occurrence"] is not None for item in committed)
        if index == 0:
            assert all(item["occurrence"]["member"]["id"] != members[1]["id"] for item in committed)
    assert accepted_sets[0].isdisjoint(accepted_sets[1])
    assert abs(sum(item["value_mm"] for item in committed_records) / 2 - truth["drawing_nominal_mm"]) < 0.02
    group = client.call("get_inspection_state")["drawing_structure"]["groups"][0]
    assert group.get("value_mm") is None and group.get("conformity_status") in (None, "indeterminate")
    assert client.call("get_measurement_context")["calibration"] == calibration_before

    context = client.call("get_measurement_context")
    native = next(item for item in context["measurements"] if item["id"] == native_ids[0])
    child = client.call("add_measurements", {"capture_id": capture,
        "expected_revision": context["document_revision"], "measurements": [{"kind": "distance",
            "points": native["points_native_px"], "label": "Descendant occurrence 1",
            "source_measurement_ids": [native_ids[0]]}]})[0]
    assert child["dependency_stale"] is False
    project = destination / "observed_occurrences.dperproj"
    client.call("save_project", {"path": str(project), "overwrite": False})
    preparation = client.call("prepare_report", {"folder": str(destination)})
    report_key = next(item["key"] for item in preparation["items"] if item["name"].startswith(project.stem))
    exported = client.call("export_report", {"capture_id": capture,
        "expected_revision": preparation["document_revision"], "folder": str(destination),
        "selected_image_keys": [report_key], "format": "pdf",
        "destination": str(destination / "observed_occurrences.pdf"), "include_audit_page": False,
        "persist_preferences": False, "overwrite": False})
    assert Path(exported["report_path"]).read_bytes().startswith(b"%PDF")
    assert exported["manifest_written"]
    text = subprocess.check_output(["pdftotext", "-layout", exported["report_path"], "-"]).decode("utf-8")
    (destination / "observed_occurrences.txt").write_text(text, encoding="utf-8")
    assert "occurrence 1" in text.casefold() and "occurrence 2" in text.casefold()
    assert "conformité" in text.casefold() and "suspend" in text.casefold()
    manifest = json.loads(Path(exported["manifest_path"]).read_text(encoding="utf-8-sig"))
    sessions = [picture["session"] for picture in manifest["pictures"]]
    assert len(sessions) == 1
    exported_records = [item for item in sessions[0]["automated_inspection"]["records"]
        if item.get("measurement_id") in native_ids]
    assert len(exported_records) == 2
    for record, original in zip(sorted(exported_records, key=lambda item: item["measurement_id"]),
                                sorted(committed_records, key=lambda item: item["measurement_id"])):
        assert record["occurrence"] == original["occurrence"]
        assert record["result"] == original["result"] and record["value_mm"] == original["value_mm"]

    before = client.call("get_measurement_context")
    reassigned = client.call("assign_drawing_occurrences", dict(assignment_args,
        expected_revision=before["document_revision"]))
    assert reassigned["assignment"]["members"] == members
    changed = client.call("get_measurement_context")
    by_id = {item["id"]: item for item in changed["measurements"]}
    for measurement_id in native_ids + [child["id"]]:
        assert by_id[measurement_id]["dependency_stale"] is True
        assert by_id[measurement_id]["components"] == []
    for record in client.call("get_inspection_state")["inspection"]["records"]:
        if record.get("measurement_id") in native_ids:
            assert record["outcome"]["geometry_state"] != "validated"
            assert "measurement_revision_changed" in record["outcome"]["blocking_reasons"]

    client.call("save_project", {"path": str(destination / "reassigned_occurrences.dperproj"), "overwrite": False})
    client.call("open_project", {"path": str(project)})
    reopened = client.call("get_inspection_state")
    assert not reopened["drawing_occurrence_assignment_status"][0]["current"]
    restored = [item for item in reopened["inspection"]["records"] if item.get("measurement_id") in native_ids]
    assert reopened["drawing_occurrence_observations"][0]["observed_occurrence_ids"] == []
    assert len(restored) == 2
    for record in restored:
        original = next(item for item in committed_records if item["measurement_id"] == record["measurement_id"])
        assert record["imported"] is True and record["occurrence"] == original["occurrence"]
        _assert_roundtrip_result(record["result"], original["result"])
        assert record["outcome"]["geometry_state"] != "validated"
        assert record["outcome"]["conformity_status"] == "indeterminate"
    _reject_without_mutation(client, dict(base_args, occurrence_id=members[0]["id"],
        expected_revision=reopened["document_revision"]))
    summary = {"two_occurrences_observed_on_distinct_pixels": True,
        "occurrence_numbering_opposes_spatial_order": True,
        "occurrence_preview_required_and_target_scoped": True,
        "occurrence_missing_unknown_and_unscoped_refused_without_mutation": True,
        "occurrence_calibration_independent_of_registration": True,
        "occurrence_values_mm": [record["value_mm"] for record in committed_records],
        "occurrence_expected_values_mm": [3.5, 3.22],
        "occurrence_nominal_mean_does_not_grant_group_conformity": True,
        "occurrence_pdf_and_manifest_preserve_separate_proofs": True,
        "occurrence_same_ids_reassignment_invalidates_measurements_and_descendant": True,
        "occurrence_reloaded_evidence_requires_fresh_assignment": True,
        "physical_qualification": "not_performed"}
    (destination / "summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")
    return summary
