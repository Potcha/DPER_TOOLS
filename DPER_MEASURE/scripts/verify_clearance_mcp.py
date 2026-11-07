"""DPM-43: deterministic, synthetic fixed-center clearance through an owned MCP host.

Use only with the test host started by test_inspection_workflow.ps1, never port 8765.
All dimensions below are independent raster truth, not drawing nominals.
"""
import argparse
import json
import math
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
from smoke_inspection_mcp import Mcp


def point(x, y):
    return {"x": x, "y": y}


def fixture(folder):
    from PIL import Image
    image = Image.new("RGB", (1120, 560))
    pixels = image.load()
    requests, truth = [], []
    transition = lambda d: 0.5 + 0.5 * math.tanh(d / 0.65)
    for row in range(2):
        for col in range(4):
            cx, cy = 139.3 + 280 * col, 140.7 + 280 * row
            distances = [58.3 + col * 0.7 + row, 66.1, 68.4, 63.2]
            for y in range(row * 280, (row + 1) * 280):
                for x in range(col * 280, (col + 1) * 280):
                    dx, dy = x - cx, y - cy
                    occupancy = transition(16 - abs(dx)) * transition(11 - abs(dy))
                    for i, (nx, ny) in enumerate([(1, 0), (0, 1), (-1, 0), (0, -1)]):
                        across, along = dx * nx + dy * ny - distances[i], dx * ny - dy * nx
                        occupancy = max(occupancy, transition(across) * transition(12 - across) * transition(28 - abs(along)))
                    shade = round(20 + 210 * occupancy + math.sin(x * .8 + y * .4) * .6)
                    pixels[x, y] = (shade,) * 3
            edges, stops = [], []
            for i, (nx, ny) in enumerate([(1, 0), (0, 1), (-1, 0), (0, -1)]):
                tx, ty = ny, -nx
                distance, half = (16, 7) if i % 2 == 0 else (11, 12)
                mx, my = cx + nx * (distance + .8), cy + ny * (distance + .8)
                edges.append({"start": point(mx - half * tx, my - half * ty),
                              "end": point(mx + half * tx, my + half * ty), "polarity": "Falling"})
                mx, my = cx + nx * (distances[i] + 1.3), cy + ny * (distances[i] + 1.3)
                stops.append({"id": f"B{i + 1}", "strip": {"start": point(mx - 22 * tx, my - 22 * ty),
                    "end": point(mx + 22 * tx, my + 22 * ty), "polarity": "Rising"}})
            requests.append({"reference": {"kind": "observed_rectangle", "edges": edges}, "stops": stops,
                "options": {"sample_count": 96, "search_radius_px": 8.0, "minimum_gradient": 12.0,
                    "polarity": "Any", "minimum_coverage": .7, "outlier_threshold_px": 1.5,
                    "maximum_rms_px": .8, "minimum_arc_sweep_rad": math.pi / 3}})
            truth.append({"center": point(cx, cy), "diameter_px": 2 * min(distances), "row": row + 1, "column": col + 1})
    image.save(folder / "clearance.png")
    calibration = {"id": "synthetic-clearance-scale", "device": "Synthetic", "resolution": [1120, 560],
        "optical_setup": "Synthetic 2D", "mm_per_pixel": .035, "reference_mm": 7, "reference_px": 200}
    (folder / "fixture.dperproj").write_text(json.dumps({"schema_version": 18, "capture_id": "clearance-synthetic",
        "image_file": "clearance.png", "device": "Synthetic", "calibration": calibration,
        "calibrations": [calibration], "measurements": []}), encoding="utf-8")
    return requests, truth


def run(folder, port):
    assert port >= 18765, "Dedicated test port required"
    folder.mkdir(parents=True, exist_ok=False)
    requests, truth = fixture(folder)
    client = Mcp(folder, port)
    tools = client.rpc("tools/list", {})["tools"]
    assert any(t["name"] == "inspect_centered_clearance" for t in tools)
    ctx = client.call("open_project", {"path": str(folder / "fixture.dperproj")})
    assert ctx["image_quality_assessment"]["is_exploitable"], ctx["image_quality_assessment"]
    records = []
    for index, (query, expected) in enumerate(zip(requests, truth)):
        args = {"capture_id": ctx["capture_id"], "expected_revision": ctx["document_revision"],
            "feature_id": f"Pad R{expected['row']}C{expected['column']}", "group": f"Rangee {expected['row']}",
            "request": query, "commit": True}
        # A direct commit without a matching preview must never manufacture a measure.
        client.call("inspect_centered_clearance", args, expect_error=True)
        args["commit"] = False
        preview = client.call("inspect_centered_clearance", args)
        assert preview["record"]["measurement_id"] is None
        observed = preview["record"]["clearance"]["result"]
        assert abs(observed["diameter_px"] - expected["diameter_px"]) < .5
        center = observed["center"]
        assert math.hypot(center["x"] - expected["center"]["x"], center["y"] - expected["center"]["y"]) < .25
        args["expected_revision"] = preview["document_revision"]
        args["commit"] = True
        committed = client.call("inspect_centered_clearance", args)
        record = committed["record"]
        assert record["clearance"]["result"] == observed
        assert abs(record["value_mm"] - observed["diameter_px"] * .035) < 1e-10
        assert record["outcome"]["geometry_state"] == "validated", record["outcome"]
        assert record["outcome"]["conformity_status"] == "indeterminate", record["outcome"]
        assert len(observed["stops"]) == 4 and min(s["gap_px"] for s in observed["stops"]) == 0
        assert record["measurement_id"] is not None
        records.append(record)
        ctx = client.call("get_measurement_context")
    project = folder / "eight-pads.dperproj"
    client.call("save_project", {"path": str(project), "overwrite": False})
    preparation = client.call("prepare_report", {"folder": str(folder)})
    keys = [item["key"] for item in preparation["items"] if item["measurement_count"] > 0]
    export_args = {"capture_id": ctx["capture_id"],
        "expected_revision": preparation["document_revision"], "folder": str(folder),
        "selected_image_keys": keys, "format": "pdf", "destination": str(folder / "clearance.pdf"),
        "include_audit_page": False, "persist_preferences": False, "overwrite": False,
        "audit_notes": "x" * 4001}
    client.call("export_report", export_args, expect_error=True)
    export_args["audit_notes"] = "x" * 4000
    exported = client.call("export_report", export_args)
    manifest = json.loads(Path(exported["manifest_path"]).read_text(encoding="utf-8-sig"))
    assert manifest["audit"]["include_audit_page"] is False
    assert manifest["audit"]["audit_notes"].startswith("x" * 4000)
    pdf = Path(exported["report_path"])
    subprocess.run(["pdftotext", str(pdf), str(folder / "clearance.txt")], check=True, capture_output=True)
    text = (folder / "clearance.txt").read_text(encoding="utf-8")
    assert "Rangee 1" in text and "Rangee 2" in text, "Both persistent groups must be exported"
    assert "Vue générale des groupes" in text.split("\f", 1)[0], "Show grouped measurements before evidence"
    assert "aperçu / aucune" not in text, "Clearance previews belong in the manifest, not in the PDF"
    assert "but" in text.casefold() and "centre" in text.casefold()
    saved = json.loads(project.read_text(encoding="utf-8-sig"))
    assert saved["schema_version"] == 19
    client.call("open_project", {"path": str(project)})
    reopened = client.call("get_inspection_state")
    imported = [r for r in reopened["inspection"]["records"] if r.get("measurement_id")]
    assert len(imported) == 8 and all(r["imported"] for r in imported)
    assert all(r["outcome"]["geometry_state"] != "validated" for r in imported)
    summary = {"passed": True, "synthetic_only": True, "pads": 8, "groups": 2,
        "preview_commit_proof_parity": True, "direct_commits_refused": 8,
        "reopened_evidence_suspended": True, "max_diameter_error_px": max(
            abs(r["clearance"]["result"]["diameter_px"] - t["diameter_px"]) for r, t in zip(records, truth)),
        "pdf": str(pdf), "records": records, "truth": truth}
    (folder / "clearance_summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")
    print(json.dumps({k: v for k, v in summary.items() if k not in ["records", "truth"]}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("folder", type=Path)
    parser.add_argument("--port", type=int, default=18766)
    args = parser.parse_args()
    run(args.folder.resolve(), args.port)
