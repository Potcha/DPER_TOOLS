"""Independent analytic raster fixtures exercised through the owned smoke MCP client.

Radii and expected refusals are specified before any detector call. Generated PNG,
projects and responses stay in the fresh synthetic smoke directory, never user data.
"""
import hashlib
import json
import math


def run_adversarial_checks(folder, client, options):
    from PIL import Image, ImageDraw

    case_folder = folder / "adversarial"
    case_folder.mkdir(exist_ok=False)
    project_template = json.loads((folder / "fixture.dperproj").read_text(encoding="utf-8"))
    # The main smoke deliberately mutates drawing.pdf. Use its preserved original.
    drawing = folder / "drawing_before_source_change.pdf"
    assert drawing.read_bytes().startswith(b"%PDF")
    rasters = {}
    cases = [
        {"id": "clean", "raster": "disk", "prior_px": 60, "polarity": "Any", "diameter_mm": 4.2},
        {"id": "local_saturation", "raster": "local_glare", "prior_px": 60, "polarity": "Any", "diameter_mm": 4.2},
        {"id": "general_saturation", "raster": "general_glare", "prior_px": 60, "polarity": "Any", "refuse": "insuffisant"},
        {"id": "equidistant_ring", "raster": "ring", "prior_px": 60, "polarity": "Any", "refuse": "ambigu"},
        {"id": "ring_inner_polarity", "raster": "ring", "prior_px": 60, "polarity": "Rising", "diameter_mm": 3.78},
        {"id": "ring_outer_polarity", "raster": "ring", "prior_px": 60, "polarity": "Falling", "diameter_mm": 4.62},
        {"id": "ring_resolved_prior", "raster": "ring", "prior_px": 65, "polarity": "Any", "diameter_mm": 4.62},
    ]
    # Persist independent ground truth before fitting; do not derive it from observations.
    truth = {"synthetic_only": True, "generator": "native-centered analytic disks with 4x4 area samples",
             "center_px": [200, 160], "disk_radius_px": 60, "ring_radii_px": [54, 66],
             "independent_mm_per_px": 0.035, "nominal_mm": 4.0,
             "diameter_error_limit_mm": 0.025, "center_error_limit_px": 0.35,
             "cases": cases}
    (case_folder / "ground_truth.json").write_text(json.dumps(truth, indent=2), encoding="utf-8")
    rows = []
    results_path = case_folder / "results.json"
    def write_results(complete=False):
        results_path.write_text(json.dumps({"passed": complete and len(rows) == len(cases),
            "complete": complete, "expected_case_count": len(cases), "cases": rows}, indent=2,
            ensure_ascii=False), encoding="utf-8")
    write_results()
    for case in cases:
        if case["raster"] not in rasters:
            # Samples are centered on native integer pixel coordinates. Drawing
            # at a larger resolution then resizing shifts that convention by
            # (scale-1)/(2*scale) native pixels, so no resize is used here.
            pixels = bytearray()
            offsets = (-0.375, -0.125, 0.125, 0.375)
            for iy in range(320):
                for ix in range(400):
                    total = 0
                    for oy in offsets:
                        for ox in offsets:
                            x, y = ix+ox, iy+oy
                            distance_squared = (x-200)**2 + (y-160)**2
                            if case["raster"] == "ring":
                                value = 255 if 54**2 <= distance_squared <= 66**2 else 40
                            else:
                                value = 40 if distance_squared <= 60**2 else 190
                                if case["raster"] == "local_glare" and 245 <= x <= 275 and 145 <= y <= 175:
                                    value = 255
                                elif case["raster"] == "general_glare" and distance_squared <= 80**2:
                                    value = 255
                            total += value
                    pixels.extend(bytes([round(total/16)])*3)
            image = Image.frombytes("RGB", (400, 320), bytes(pixels))
            draw = ImageDraw.Draw(image)
            # Texture far from profiles keeps global sharpness from masking contour tests.
            for bx, by in [(10, 10), (350, 10), (10, 260), (350, 260)]:
                for iy in range(8):
                    for ix in range(8):
                        shade = 55 if (ix+iy) % 2 else 215
                        draw.rectangle((bx+ix*5, by+iy*5, bx+(ix+1)*5-1, by+(iy+1)*5-1), fill=(shade,)*3)
            rasters[case["raster"]] = image
        path = case_folder / (case["id"] + ".png")
        rasters[case["raster"]].save(path)
        project = dict(project_template, capture_id="synthetic-adversarial-"+case["id"], image_file=path.name)
        project_path = path.with_suffix(".dperproj")
        project_path.write_text(json.dumps(project, indent=2), encoding="utf-8")
        context = client.call("open_project", {"path": str(project_path)})
        assert context["image_available"] and context["capture_id"] == project["capture_id"]
        assert context["image_quality_assessment"]["is_exploitable"], context["image_quality_assessment"]
        state = client.call("analyze_drawing", {"path": str(drawing), "expected_revision": context["document_revision"]})
        features = [f for f in state["inspection"]["plan"]["characteristics"]
                    if f["kind"] == "diameter" and f["unit"] == "millimeter"]
        assert len(features) == 1 and features[0]["nominal"] == truth["nominal_mm"]
        capture = state["capture_id"]
        state = client.call("register_inspection", {"capture_id": capture,
            "expected_revision": state["document_revision"], "maximum_error_px": 0.1,
            "correspondences": [
                {"plan": {"x": 0, "y": 0}, "image": {"x": 50, "y": 50}},
                {"plan": {"x": 100, "y": 0}, "image": {"x": 250, "y": 50}},
                {"plan": {"x": 0, "y": 100}, "image": {"x": 50, "y": 250}}]})
        case_options = dict(options, search_radius_px=8.0, polarity=case["polarity"])
        args = {"capture_id": capture, "expected_revision": state["document_revision"],
            "feature_id": features[0]["id"], "group": "DPM-27 "+case["id"],
            "geometry_in_drawing": {"Circle": {"center": {"x": 75, "y": 55}, "radius": case["prior_px"]/2}},
            "options": case_options, "commit": True}
        before = client.call("get_measurement_context")
        result = client.call("inspect_feature", args, expect_error="refuse" in case)
        row = {"case": case["id"], "png_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
               "expected": case, "passed": True}
        if "refuse" in case:
            error = "\n".join(block.get("text", "") for block in result.get("content", []))
            assert case["refuse"] in error.casefold(), error
            after = client.call("get_measurement_context")
            assert after["document_revision"] == before["document_revision"]
            assert after["measurements"] == before["measurements"] == []
            assert client.call("get_inspection_state")["inspection"]["records"] == []
            row.update(refused=True, error=error, no_partial_mutation=True)
        else:
            record = result["record"]
            assert abs(record["value_mm"]-case["diameter_mm"]) < truth["diameter_error_limit_mm"], record
            measured = record["result"]["geometry"]["Circle"]
            center = measured["center"]
            assert math.hypot(center["x"]-200, center["y"]-160) < truth["center_error_limit_px"], measured
            assert abs(record["value_mm"]-truth["nominal_mm"]) > 0.15
            assert record["outcome"]["conformity_status"] in {"indeterminate", "not_applicable"}
            assert {"calibration_unqualified", "uncertainty_unknown"} <= set(record["outcome"]["blocking_reasons"])
            assert record["measurement_id"] is not None
            row.update(measured_mm=record["value_mm"], center_px=center,
                       coverage=record["result"]["evidence"]["coverage_fraction"],
                       conformity_status=record["outcome"]["conformity_status"])
        # Opening another project must respect the application's dirty-context guard.
        client.call("save_project", {"path": str(case_folder / (case["id"]+"_result.dperproj")), "overwrite": False})
        rows.append(row)
        write_results()
    write_results(complete=True)
    return {"adversarial_cases_passed": len(rows), "ambiguous_contour_refused_without_mutation": True,
            "global_saturation_refused_without_mutation": True, "local_saturation_measurement_preserved": True,
            "polarity_and_prior_resolve_distinct_observed_radii": True}
