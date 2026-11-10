"""Check the DPM-15 diameter model over MCP with synthetic inputs."""

import argparse
import json
import math
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from smoke_inspection_mcp import Mcp

TOOL = "calculate_diameter_uncertainty"


def example(rho=0):
    return {
        "radius_px": 200,
        "standard_uncertainty_radius_px": 0.2,
        "radius_source": "Synthetic radius",
        "scale": {
            "millimeters_per_pixel": 0.01,
            "relative_standard_uncertainty": 0.01,
            "source": "Synthetic isotropic scale",
        },
        "coverage_factor": 2,
        "correlation_matrix": [[1, rho], [rho, 1]],
        "correlation_justification": f"Synthetic model with rho={rho}",
    }


def verify(client):
    def session():
        return client.call("get_measurement_context"), client.call("get_inspection_state")

    baseline = session()
    cases = []
    for rho, variance in ((0, 0.001616), (0.5, 0.001776), (-0.5, 0.001456), (1, 0.001936)):
        request = example(rho)
        result = client.call(TOOL, request)
        assert result["request"] == request and result["status"] == "calculation_only"
        assert result["diameter_px"] == 400 and result["diameter_mm"] == 4
        assert result["input_order"] == ["radius_px", "relative_scale"]
        assert all(result[key] is False for key in (
            "applied_to_measurement", "source_qualification_verified", "conformity_evaluated",
            "input_provenance_verified",
        ))
        budget = result["budget"]
        assert budget["output_unit"] == "millimeter"
        assert [c["input_unit"] for c in budget["components"]] == ["pixel", "dimensionless"]
        assert [c["sensitivity_coefficient"] for c in budget["components"]] == [0.02, 4]
        assert [c["source"] for c in budget["components"]] == [request["radius_source"], request["scale"]["source"]]
        assert math.isclose(budget["combined_standard_uncertainty"], math.sqrt(variance), rel_tol=1e-12)
        assert math.isclose(budget["expanded_uncertainty"], 2 * math.sqrt(variance), rel_tol=1e-12)
        assert session() == baseline
        cases.append({"case": f"rho={rho}", "response": result})

    bad_scale = example()["scale"] | {"millimeters_per_pixel": 0}
    forged_scale = example()["scale"] | {"qualified": True}
    for name, rho, changes in (
        ("zero_radius", 0, {"radius_px": 0}),
        ("zero_scale", 0, {"scale": bad_scale}),
        ("negative_radius_uncertainty", 0, {"standard_uncertainty_radius_px": -0.2}),
        ("missing_source", 0, {"radius_source": " "}),
        ("missing_justification", 0, {"correlation_justification": ""}),
        ("invalid_k", 0, {"coverage_factor": 0}),
        ("invalid_matrix_size", 0, {"correlation_matrix": [[1]]}),
        ("invalid_matrix_symmetry", 0, {"correlation_matrix": [[1, 0.5], [0, 1]]}),
        ("zero_combined_uncertainty", -1, {"standard_uncertainty_radius_px": 2}),
        ("forged_nominal", 0, {"nominal_mm": 4}),
        ("forged_scale_qualification", 0, {"scale": forged_scale}),
    ):
        request = example(rho) | changes
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

    folder = args.folder.resolve()
    folder.mkdir(parents=True, exist_ok=False)
    client = Mcp(folder, args.test_port)
    # The owned host expects one setup mutation before its read-only checks.
    from PIL import Image

    sentinel = folder / "sentinel.png"
    Image.new("RGB", (8, 8), (80, 80, 80)).save(sentinel)
    client.call("open_image", {"path": str(sentinel)})
    cases = verify(client)
    summary = {"passed": len(cases), "failed": 0, "session_unchanged": True}
    (folder / "summary.json").write_text(json.dumps({**summary, "cases": cases}, indent=2), encoding="utf-8")
    print(json.dumps(summary))


if __name__ == "__main__":
    main()
