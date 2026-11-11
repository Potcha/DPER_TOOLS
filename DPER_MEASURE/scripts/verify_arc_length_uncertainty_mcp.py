"""Verify the DPM-15 arc-length uncertainty calculation over MCP."""

import argparse
import json
import math
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from smoke_inspection_mcp import Mcp

TOOL = "calculate_arc_length_uncertainty"


def example(rho=0):
    return {
        "radius_px": 100,
        "standard_uncertainty_radius_px": 1,
        "radius_source": "Rayon synthétique",
        "sweep_deg": 90,
        "standard_uncertainty_sweep_deg": 0.9,
        "sweep_source": "Ouverture synthétique",
        "scale": {
            "millimeters_per_pixel": 0.01,
            "relative_standard_uncertainty": 0.01,
            "source": "Échelle synthétique",
        },
        "coverage_factor": 2,
        "correlation_matrix": [[1, rho, 0], [rho, 1, 0], [0, 0, 1]],
        "correlation_justification": f"Corrélation synthétique rayon/ouverture : {rho}",
    }


def verify(client):
    def session():
        return client.call("get_measurement_context"), client.call("get_inspection_state")

    baseline = session()
    cases = []
    for rho, variance_factor in ((0, 3), (0.5, 4), (-0.5, 2)):
        request = example(rho)
        result = client.call(TOOL, request)
        assert result["request"] == request and result["status"] == "calculation_only"
        assert result["input_order"] == ["radius_px", "sweep_deg", "relative_scale"]
        assert math.isclose(result["arc_length_px"], 50 * math.pi, rel_tol=1e-12)
        assert math.isclose(result["arc_length_mm"], math.pi / 2, rel_tol=1e-12)
        assert all(result[key] is False for key in (
            "applied_to_measurement", "source_qualification_verified",
            "conformity_evaluated", "input_provenance_verified",
        ))
        budget = result["budget"]
        assert budget["output_unit"] == "millimeter"
        assert [c["input_unit"] for c in budget["components"]] == ["pixel", "degree", "dimensionless"]
        expected = math.pi / 100 * math.sqrt(variance_factor)
        assert math.isclose(budget["expanded_uncertainty"], expected, rel_tol=1e-12)
        assert session() == baseline
        cases.append({"case": f"rho={rho}", "response": result})

    for name, changes in (
        ("zero_sweep", {"sweep_deg": 0}),
        ("signed_sweep", {"sweep_deg": -90}),
        ("full_circle", {"sweep_deg": 360}),
        ("zero_radius", {"radius_px": 0}),
        ("negative_sweep_uncertainty", {"standard_uncertainty_sweep_deg": -0.9}),
        ("missing_source", {"sweep_source": " "}),
        ("invalid_matrix", {"correlation_matrix": [[1, 0], [0, 1]]}),
        ("forged_nominal", {"nominal_mm": 4}),
    ):
        request = example() | changes
        response = client.rpc("tools/call", {"name": TOOL, "arguments": request}, return_envelope=True)
        assert "error" in response or response.get("result", {}).get("isError") is True, (name, response)
        assert session() == baseline
        cases.append({"case": name, "request": request, "response": response})
    return cases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path)
    parser.add_argument("--test-port", type=int, required=True)
    args = parser.parse_args()
    if not 18765 <= args.test_port <= 65535:
        parser.error("Use an owned test host >=18765.")

    from PIL import Image

    folder = args.folder.resolve()
    folder.mkdir(parents=True, exist_ok=False)
    client = Mcp(folder, args.test_port)
    sentinel = folder / "sentinel.png"
    Image.new("RGB", (8, 8), (80, 80, 80)).save(sentinel)
    client.call("open_image", {"path": str(sentinel)})
    cases = verify(client)
    summary = {"passed": len(cases), "failed": 0, "session_unchanged": True}
    (folder / "summary.json").write_text(
        json.dumps({**summary, "cases": cases}, indent=2), encoding="utf-8"
    )
    print(json.dumps(summary))


if __name__ == "__main__":
    main()
