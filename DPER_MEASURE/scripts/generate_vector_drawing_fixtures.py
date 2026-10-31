"""Create synthetic vector drawing PDFs and independent expectations with stdlib only.

Usage: python scripts/generate_vector_drawing_fixtures.py --output-dir NEW_DIRECTORY
The output directory must not exist; no real drawing or existing output is overwritten.
PDF drawing coordinates and expectations use points with a top-left origin. These
coordinates locate geometry only: no physical scale is inferred from the PDF size.
"""

import argparse
import hashlib
import json
import math
from pathlib import Path


SYNTHETIC_NOTICE = "DONNEES SYNTHETIQUES - AUCUN PLAN INDUSTRIEL REEL"
KAPPA = 4.0 * (math.sqrt(2.0) - 1.0) / 3.0


def number(value):
    return f"{value:.8f}".rstrip("0").rstrip(".") or "0"


def pdf_string(text):
    return text.encode("cp1252").replace(b"\\", b"\\\\").replace(b"(", b"\\(").replace(b")", b"\\)")


class Drawing:
    def __init__(self, width=400, height=320):
        self.width, self.height = width, height
        self.commands = [f"q 1 0 0 -1 0 {height} cm 0 G 0 g 0.8 w".encode("ascii")]

    def command(self, value):
        self.commands.append(value.encode("ascii"))

    def text(self, x, baseline_y, text, size=11):
        # Undo the outer y flip for upright Helvetica glyphs.
        self.commands.append(
            f"BT /F1 {number(size)} Tf 1 0 0 -1 {number(x)} {number(baseline_y)} Tm (".encode("ascii")
            + pdf_string(text) + b") Tj ET"
        )

    def translated(self, x, y):
        self.command(f"q 1 0 0 1 {number(x)} {number(y)} cm")

    def restore(self):
        self.command("Q")

    def polyline(self, points, close=False):
        self.command(f"{number(points[0][0])} {number(points[0][1])} m")
        for x, y in points[1:]:
            self.command(f"{number(x)} {number(y)} l")
        self.command("h S" if close else "S")

    def rectangle(self, x, y, width, height):
        self.command(f"{number(x)} {number(y)} {number(width)} {number(height)} re S")

    def circle(self, x, y, radius):
        k = radius * KAPPA
        self.command(f"{number(x + radius)} {number(y)} m")
        for values in [
            (x + radius, y + k, x + k, y + radius, x, y + radius),
            (x - k, y + radius, x - radius, y + k, x - radius, y),
            (x - radius, y - k, x - k, y - radius, x, y - radius),
            (x + k, y - radius, x + radius, y - k, x + radius, y),
        ]:
            self.command(" ".join(number(value) for value in values) + " c")
        self.command("h S")

    def polygon_circle(self, x, y, radius, segments=48):
        self.polyline([
            (x + radius * math.cos(2 * math.pi * index / segments),
             y + radius * math.sin(2 * math.pi * index / segments))
            for index in range(segments)
        ], close=True)

    def pdf_bytes(self):
        content = b"\n".join(self.commands + [b"Q"]) + b"\n"
        objects = [
            b"<< /Type /Catalog /Pages 2 0 R >>",
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            (f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {self.width} {self.height}] "
             "/Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>").encode("ascii"),
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
            b"<< /Length " + str(len(content)).encode("ascii") + b" >>\nstream\n" + content + b"endstream",
        ]
        pdf = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
        offsets = [0]
        for index, obj in enumerate(objects, 1):
            offsets.append(len(pdf))
            pdf.extend(f"{index} 0 obj\n".encode("ascii") + obj + b"\nendobj\n")
        xref = len(pdf)
        pdf.extend(b"xref\n0 6\n0000000000 65535 f \n")
        for offset in offsets[1:]:
            pdf.extend(f"{offset:010d} 00000 n \n".encode("ascii"))
        pdf.extend(f"trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode("ascii"))
        return bytes(pdf)


def bbox(cx, cy, radius):
    return [cx - radius, cy - radius, cx + radius, cy + radius]


def circle_expectation(shape_id, view_id, center, radius, representation="four_cubic_beziers"):
    return {"id": shape_id, "view_id": view_id, "kind": "circle", "center_pt": center,
            "radius_pt": radius, "source_bbox_pt": bbox(*center, radius),
            "representation": representation, "physical_dimension_from_pdf": None}


def principal_fixture():
    drawing = Drawing()
    drawing.text(145, 19, "SYNTHETIQUE - CERCLE ET RENVOI", 10)
    drawing.text(165, 40, "UNITS: mm", 10)
    drawing.translated(75, 55)
    drawing.circle(0, 0, 29)
    drawing.restore()
    diagonal = 29 / math.sqrt(2)
    drawing.polyline([(75 + diagonal, 55 + diagonal), (125, 105), (125, 238), (145, 238)])
    drawing.text(35, 266, "Ø4.000 ±0.100 mm", 14)
    drawing.text(15, 294, "Taille graphique sans echelle metrologique implicite.", 9)
    drawing.text(15, 311, SYNTHETIC_NOTICE, 8)
    expected = {
        "id": "circle-leader", "file": "01_circle_leader.pdf", "page_size_pt": [400, 320],
        "views": [{"id": "main", "source_bbox_pt": [20, 15, 140, 100]}],
        "shapes": [circle_expectation("circle-main", "main", [75, 55], 29)],
        "labels": [{"text": "Ø4.000 ±0.100 mm", "baseline_pt": [35, 266],
                    "source_bbox_pt": [30, 250, 200, 272], "candidate_shape_ids": ["circle-main"],
                    "nominal_mm": 4.0, "lower_limit_mm": 3.9, "upper_limit_mm": 4.1}],
        "expected_counts": {"views": 1, "circles": 1, "rectangles": 0, "leader_polylines": 1},
        "smoke_photo_ground_truth": {
            "image_size_px": [400, 320], "center_px": [200, 160], "radius_px": 60,
            "independent_calibration": {"reference_length_mm": 7.0, "reference_length_px": 200.0,
                                        "mm_per_pixel": 0.035},
            "actual_diameter_mm": 4.2, "drawing_nominal_mm": 4.0,
            "registration": {"scale": 2.0, "rotation_rad": 0.0, "translation_px": [50, 50],
                "correspondences": [
                    {"plan": {"x": 0, "y": 0}, "image": {"x": 50, "y": 50}},
                    {"plan": {"x": 100, "y": 0}, "image": {"x": 250, "y": 50}},
                    {"plan": {"x": 0, "y": 100}, "image": {"x": 50, "y": 250}}]},
            "projected_search_center_px": [200, 160], "projected_search_radius_px": 58,
            "expected_observed_radius_px": 60,
            "acceptance_note": "Les pixels doivent etablir 4.2 mm, pas imposer 4.0 mm depuis le texte ni 4.06 mm depuis le rayon graphique projete. Aucune qualification physique par cette donnee synthetique."
        },
        "association_is_ambiguous": False,
    }
    return drawing, expected


def concentric_fixture():
    drawing = Drawing()
    drawing.text(20, 21, "SYNTHETIQUE - DEUX CONTOURS CONCENTRIQUES", 10)
    drawing.text(20, 42, "UNITS: mm", 10)
    drawing.translated(180, 132)
    drawing.circle(0, 0, 30)
    drawing.circle(0, 0, 55)
    drawing.restore()
    drawing.text(35, 254, "Ø6.000 ±0.050 mm", 14)
    drawing.text(20, 278, "Cote sans renvoi : aucun contour selectionne par la source.", 9)
    drawing.text(15, 311, SYNTHETIC_NOTICE, 8)
    return drawing, {
        "id": "concentric-ambiguous", "file": "02_concentric_ambiguous.pdf", "page_size_pt": [400, 320],
        "views": [{"id": "concentric", "source_bbox_pt": [115, 65, 245, 200]}],
        "shapes": [circle_expectation("inner", "concentric", [180, 132], 30),
                   circle_expectation("outer", "concentric", [180, 132], 55)],
        "labels": [{"text": "Ø6.000 ±0.050 mm", "baseline_pt": [35, 254],
                    "source_bbox_pt": [30, 238, 200, 260], "candidate_shape_ids": ["inner", "outer"],
                    "nominal_mm": 6.0, "lower_limit_mm": 5.95, "upper_limit_mm": 6.05}],
        "expected_counts": {"views": 1, "circles": 2, "rectangles": 0, "leader_polylines": 0},
        "association_is_ambiguous": True,
        "required_behavior": "Conserver les deux hypotheses ; ne jamais choisir le rayon proche du nominal pour forcer la conformite.",
    }


def symmetric_views_fixture():
    drawing = Drawing()
    drawing.text(20, 20, "SYNTHETIQUE - DEUX VUES ET SYMETRIE", 10)
    drawing.text(20, 40, "UNITS: mm", 10)
    for x, name, polygon in [(35, "VUE A", True), (240, "VUE B", False)]:
        drawing.translated(x, 65)
        drawing.rectangle(0, 0, 125, 110)
        drawing.command("[4 3] 0 d")
        drawing.polyline([(-8, 55), (133, 55)])
        drawing.polyline([(62.5, -8), (62.5, 118)])
        drawing.command("[] 0 d")
        if polygon:
            drawing.polygon_circle(62.5, 55, 16, segments=48)
        else:
            drawing.circle(62.5, 55, 16)
        drawing.restore()
        drawing.text(x + 40, 201, name, 11)
    drawing.text(110, 240, "24.000 ±0.200 mm", 13)
    drawing.text(110, 267, "Ø3.000 ±0.050 mm", 13)
    drawing.text(15, 292, "Vues symetriques : appariement sans autre repere ambigu.", 9)
    drawing.text(15, 311, SYNTHETIC_NOTICE, 8)
    shapes = []
    for view_id, x in [("A", 35), ("B", 240)]:
        shapes.extend([
            {"id": f"rectangle-{view_id}", "view_id": view_id, "kind": "rectangle",
             "source_bbox_pt": [x, 65, x + 125, 175], "physical_dimension_from_pdf": None},
            circle_expectation(f"hole-{view_id}", view_id, [x + 62.5, 120], 16,
                "closed_polyline_48_segments" if view_id == "A" else "four_cubic_beziers"),
        ])
    return drawing, {
        "id": "two-symmetric-views", "file": "03_two_symmetric_views.pdf", "page_size_pt": [400, 320],
        "views": [{"id": "A", "source_bbox_pt": [25, 55, 170, 190]},
                  {"id": "B", "source_bbox_pt": [230, 55, 375, 190]}],
        "shapes": shapes,
        "labels": [
            {"text": "24.000 ±0.200 mm", "baseline_pt": [110, 240], "source_bbox_pt": [105, 225, 290, 246],
             "candidate_shape_ids": ["rectangle-A", "rectangle-B"], "nominal_mm": 24.0,
             "lower_limit_mm": 23.8, "upper_limit_mm": 24.2},
            {"text": "Ø3.000 ±0.050 mm", "baseline_pt": [110, 267], "source_bbox_pt": [105, 252, 290, 273],
             "candidate_shape_ids": ["hole-A", "hole-B"], "nominal_mm": 3.0,
             "lower_limit_mm": 2.95, "upper_limit_mm": 3.05}],
        "expected_counts": {"views": 2, "circles": 2, "rectangles": 2, "centerline_segments": 4},
        "association_is_ambiguous": True,
        "required_behavior": "Preserver les vues A et B et l'ambiguite d'appariement ; ne pas fusionner les deux cercles en une seule observation.",
    }


def generate(output_dir):
    # Exclusive directory creation is required even when an existing directory is empty.
    output_dir.mkdir(parents=True, exist_ok=False)
    expected = {"schema_version": 1, "synthetic_only": True,
                "coordinate_system": {"origin": "top_left", "x": "right", "y": "down", "unit": "pdf_point",
                                      "bbox_format": "[left, top, right, bottom]"},
                "expectations_source": "Explicit generator design values; never an extractor or registration result.",
                "physical_scale_warning": "PDF points do not establish a physical calibration; no nominal-to-PDF-size conversion is assumed.",
                "fixtures": []}
    manifest = {"schema_version": 1, "synthetic_only": True, "files": [],
                "coordinate_system": expected["coordinate_system"],
                "bbox_note": "View boxes are design ROIs; shape boxes are analytic stroke-centerline extents. Text boxes are reserved layout regions, not extracted font bounds."}
    for drawing, fixture in [principal_fixture(), concentric_fixture(), symmetric_views_fixture()]:
        data = drawing.pdf_bytes()
        with (output_dir / fixture["file"]).open("xb") as stream:
            stream.write(data)
        expected["fixtures"].append(fixture)
        manifest["files"].append({"file": fixture["file"], "sha256": hashlib.sha256(data).hexdigest(),
                                  "bytes": len(data), "pages": 1, "page_size_pt": fixture["page_size_pt"],
                                  "views": fixture["views"], "expected_counts": fixture["expected_counts"],
                                  "font": "Helvetica / WinAnsiEncoding", "synthetic_only": True})
    for name, body in [("expected.json", expected), ("manifest.json", manifest)]:
        with (output_dir / name).open("x", encoding="utf-8", newline="\n") as stream:
            json.dump(body, stream, ensure_ascii=False, indent=2, allow_nan=False)
            stream.write("\n")
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, required=True, help="New output directory; must not exist.")
    args = parser.parse_args()
    try:
        manifest = generate(args.output_dir)
    except FileExistsError:
        parser.error("The output directory already exists; choose a new directory. No overwrite is allowed.")
    print(json.dumps({"output_dir": str(args.output_dir.resolve()), "pdf_count": len(manifest["files"]),
                      "expectations": "expected.json", "manifest": "manifest.json"}, ensure_ascii=False))


if __name__ == "__main__":
    main()

