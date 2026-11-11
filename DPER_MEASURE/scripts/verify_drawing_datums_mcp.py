"""Generate synthetic datum PDFs and optionally verify them on an owned MCP host."""

import argparse
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from generate_vector_drawing_fixtures import Drawing
from smoke_inspection_mcp import Mcp


def write_fixture(path, duplicate):
    drawing = Drawing()
    # The cross is rendered in Helvetica. The overlaid ring completes the
    # position symbol; ActualText gives Poppler the single Unicode character.
    drawing.command("/Span << /ActualText <FEFF2316> >> BDC")
    drawing.text(35, 55, "+", 14)
    drawing.command("EMC")
    drawing.circle(39, 51, 2)
    drawing.text(45, 55, " | Ø0,1 mm | A | B", 14)
    drawing.text(35, 105, "DATUM A")
    drawing.text(35, 150, "REPÈRE A" if duplicate else "B")
    with path.open("xb") as file:
        file.write(drawing.pdf_bytes())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path)
    parser.add_argument("--test-port", type=int)
    args = parser.parse_args()
    if args.test_port is not None and not 18765 <= args.test_port <= 65535:
        parser.error("Use the port of an owned test host.")
    folder = args.folder.resolve()
    folder.mkdir(parents=True, exist_ok=False)
    cases = [("repere-unique.pdf", "candidate_view_unresolved"),
             ("repere-duplique.pdf", "target_ambiguous")]
    for index, (name, _) in enumerate(cases):
        write_fixture(folder / name, duplicate=index != 0)
    if args.test_port is None:
        return

    client = Mcp(folder, args.test_port)
    before = client.call("get_measurement_context")["measurements"]
    for name, expected in cases:
        revision = client.call("get_inspection_state")["document_revision"]
        client.call("analyze_drawing", {"path": str(folder / name), "expected_revision": revision})
        state = client.call("get_inspection_state")
        structure = state["drawing_structure"]
        a, b = structure["datum_references"]
        assert (a["label"], a["state"], b["label"], b["state"]) == ("A", expected, "B", "target_missing"), structure
        assert a["dependent_source"]["pdf_bbox"] and a["dependent_source"]["word_ids"], a
        declarations = structure["datum_declarations"]
        if expected == "candidate_view_unresolved":
            assert a["candidate_source_line_id"] == declarations[0]["source"]["source_line_id"], structure
        else:
            assert len(declarations) == 2 and a["candidate_source_line_id"] is None, structure
        assert all(item["source"]["bbox"] and item["source"]["word_ids"] for item in declarations)
        assert client.call("get_measurement_context")["measurements"] == before
    print("PASS: provenance, unique unresolved candidate, duplicate A and missing B; no photo measurement.")


if __name__ == "__main__":
    main()
