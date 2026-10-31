"""Verify drawing-geometry CLI against independently authored fixture expectations.

Uses only the standard library. Catalogs and results go to a new local output
directory; no source PDF, expected value, application session or UI is modified.
"""
from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]


def fingerprints(path: Path) -> dict:
    sha256 = hashlib.sha256()
    fnv = 0xCBF29CE484222325
    with path.open("rb") as source:
        while block := source.read(65536):
            sha256.update(block)
            for byte in block:
                fnv = ((fnv ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return {"sha256": sha256.hexdigest(), "fnv1a64": f"fnv1a64:{fnv:016x}"}


def sha256_file(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def extract(executable: Path, source: Path, destination: Path) -> tuple[dict, dict | None]:
    before = fingerprints(source)
    command = [str(executable), "--cli", "drawing-geometry", str(source), "1", str(destination)]
    execution = subprocess.run(command, cwd=ROOT, capture_output=True, text=True,
                               encoding="utf-8", errors="replace", timeout=90, check=False)
    after = fingerprints(source)
    catalog = json.loads(destination.read_text(encoding="utf-8-sig")) if destination.exists() else None
    report = {
        "source": str(source), "catalog": str(destination), "command": command,
        "returncode": execution.returncode, "stdout": execution.stdout.strip(),
        "stderr": execution.stderr.strip(), "source_before": before, "source_after": after,
        "source_unchanged": before == after,
        "catalog_fingerprint_matches": catalog is not None and catalog.get("document_fingerprint") == before["fnv1a64"],
    }
    report["passed"] = execution.returncode == 0 and report["source_unchanged"] and report["catalog_fingerprint_matches"]
    return report, catalog


def check_fixture(expectation: dict, manifest: dict, execution: dict,
                  catalog: dict | None, tolerance: float) -> dict:
    result = {"id": expectation["id"], "execution": execution, "checks": {}, "circle_matches": []}
    checks = result["checks"]
    checks["cli_and_source_binding"] = execution["passed"]
    expected_sha = manifest[expectation["file"]]["sha256"]
    checks["fixture_matches_manifest"] = execution["source_before"]["sha256"] == expected_sha
    if catalog is None or "geometry" not in catalog:
        result["passed"] = False
        return result
    document = catalog["geometry"]
    primitives = document["primitives"]
    circles = [item for item in primitives if item["kind"] == "circle_candidate"]
    result["primitive_count"] = len(primitives)
    result["kind_counts"] = dict(Counter(item["kind"] for item in primitives))
    result["expected_circle_count"] = expectation["expected_counts"]["circles"]
    result["actual_circle_count"] = len(circles)
    result["ignored_text_or_image_count"] = document["ignored_text_or_image_count"]
    result["warnings"] = document["warnings"]
    checks["page_one"] = document["page"] == 1
    checks["page_size_pt"] = all(abs(actual - expected) <= 1e-5 for actual, expected in zip(
        [document["width_pt"], document["height_pt"]], expectation["page_size_pt"]))
    checks["circle_count"] = len(circles) == result["expected_circle_count"]
    checks["text_was_excluded"] = document["ignored_text_or_image_count"] > 0
    checks["no_text_glyphs_or_images_as_primitives"] = all(
        item["source"]["element_name"] not in {"text", "tspan", "image", "use"}
        and not (item["source"].get("path_id") or "").lower().startswith("glyph")
        for item in primitives
    )
    checks["no_automatic_confirmation"] = all(item["confirmed"] is False for item in primitives)
    expected_circles = [shape for shape in expectation["shapes"] if shape["kind"] == "circle"]
    remaining = list(circles)
    for expected in expected_circles:
        ex, ey = expected["center_pt"]
        radius = expected["radius_pt"]
        if not remaining:
            result["circle_matches"].append({"expected_shape_id": expected["id"], "passed": False, "reason": "circle_missing"})
            continue
        match = min(remaining, key=lambda item: math.hypot(item["circle"]["center"]["x"] - ex,
                    item["circle"]["center"]["y"] - ey) + abs(item["circle"]["radius_pt"] - radius))
        remaining.remove(match)
        actual = match["circle"]
        center_error = math.hypot(actual["center"]["x"] - ex, actual["center"]["y"] - ey)
        radius_error = abs(actual["radius_pt"] - radius)
        result["circle_matches"].append({
            "expected_shape_id": expected["id"], "primitive_id": match["id"],
            "expected_center_pt": [ex, ey], "actual_center_pt": [actual["center"]["x"], actual["center"]["y"]],
            "expected_radius_pt": radius, "actual_radius_pt": actual["radius_pt"],
            "center_error_pt": center_error, "radius_error_pt": radius_error,
            "rms_residual_pt": actual["rms_residual_pt"], "max_residual_pt": actual["max_residual_pt"],
            "coverage_rad": actual["angular_coverage_rad"],
            "passed": center_error <= tolerance and radius_error <= tolerance,
        })
    checks["top_left_centers_and_radii_match_design"] = all(item["passed"] for item in result["circle_matches"])
    result["passed"] = all(checks.values())
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, default=ROOT / "target/debug/dper_measure.exe")
    parser.add_argument("--fixtures", type=Path, default=ROOT / "tests/fixtures/drawing_geometry")
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--industrial-pdf", type=Path)
    parser.add_argument("--geometry-tolerance-pt", type=float, default=0.15)
    args = parser.parse_args()
    output = args.output_dir.resolve()
    allowed = (ROOT / "output/automation-validation").resolve()
    if not output.is_relative_to(allowed) or output == allowed:
        parser.error("Output must be a new subdirectory of output/automation-validation.")
    if output.exists():
        parser.error("Output already exists; choose a new directory to preserve existing artifacts.")
    if not math.isfinite(args.geometry_tolerance_pt) or args.geometry_tolerance_pt <= 0:
        parser.error("Geometry tolerance must be finite and positive.")
    expected = json.loads((args.fixtures / "expected.json").read_text(encoding="utf-8-sig"))
    manifest = json.loads((args.fixtures / "manifest.json").read_text(encoding="utf-8-sig"))
    by_file = {item["file"]: item for item in manifest["files"]}
    if expected["coordinate_system"]["origin"] != "top_left" or expected["coordinate_system"]["unit"] != "pdf_point":
        parser.error("Fixture expectations do not use PDF points with top-left origin.")
    output.mkdir(parents=True, exist_ok=False)
    result = {
        "schema_version": 1, "created_at_utc": datetime.now(timezone.utc).isoformat(),
        "executable": str(args.executable.resolve()), "executable_sha256": sha256_file(args.executable),
        "expectations_sha256": fingerprints(args.fixtures / "expected.json")["sha256"],
        "coordinate_system": expected["coordinate_system"], "tolerance_pt": args.geometry_tolerance_pt,
        "fixtures": [], "industrial": None,
        "limitations": ["Synthetic geometric checks only, not physical calibration or qualification.",
                        "Industrial extraction counts and warnings do not establish drawing interpretation completeness.",
                        "No semantic view/leader recognition, photo inspection or UI session is exercised."],
    }
    try:
        for fixture in expected["fixtures"]:
            source = (args.fixtures / fixture["file"]).resolve()
            execution, catalog = extract(args.executable.resolve(), source, output / f"{source.stem}.geometry.json")
            result["fixtures"].append(check_fixture(fixture, by_file, execution, catalog, args.geometry_tolerance_pt))
        if args.industrial_pdf:
            execution, catalog = extract(args.executable.resolve(), args.industrial_pdf.resolve(), output / "industrial.geometry.json")
            industrial = {"execution": execution, "passed": execution["passed"]}
            if catalog is not None:
                geometry = catalog["geometry"]
                industrial.update({"page": geometry["page"], "page_size_pt": [geometry["width_pt"], geometry["height_pt"]],
                    "primitive_count": len(geometry["primitives"]),
                    "kind_counts": dict(Counter(item["kind"] for item in geometry["primitives"])),
                    "ignored_text_or_image_count": geometry["ignored_text_or_image_count"],
                    "unsupported_path_count": geometry["unsupported_path_count"], "warnings": geometry["warnings"],
                    "acceptance": "Extraction and source preservation only; no exhaustive interpretation or metrology claim."})
            result["industrial"] = industrial
        result["passed"] = all(item["passed"] for item in result["fixtures"]) and (
            result["industrial"] is None or result["industrial"]["passed"])
    except Exception as error:
        result["passed"] = False
        result["error"] = f"{type(error).__name__}: {error}"
    with (output / "result.json").open("x", encoding="utf-8") as destination:
        json.dump(result, destination, ensure_ascii=False, indent=2)
        destination.write("\n")
    print(json.dumps({"passed": result["passed"], "result": str(output / "result.json"),
                      "fixture_circle_counts": [item.get("actual_circle_count") for item in result["fixtures"]],
                      "industrial_primitives": (result["industrial"] or {}).get("primitive_count")}, ensure_ascii=False))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
