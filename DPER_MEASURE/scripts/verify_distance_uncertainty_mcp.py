"""DPM-15 distance model checks on an owned MCP host; never qualify a real measurement."""
import argparse
import copy
import json
import math
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from PIL import Image
from smoke_inspection_mcp import Mcp


def example():
    return {
        "first_point": {"position_px": {"x": 0, "y": 0},
                        "standard_uncertainty_x_px": .2, "standard_uncertainty_y_px": .2,
                        "source": "Synthetic point 1; coordinate uncertainty explicitly supplied"},
        "second_point": {"position_px": {"x": 30, "y": 40},
                         "standard_uncertainty_x_px": .2, "standard_uncertainty_y_px": .2,
                         "source": "Synthetic point 2; coordinate uncertainty explicitly supplied"},
        "scale": {"millimeters_per_pixel": .01, "relative_standard_uncertainty": .01,
                  "source": "Synthetic isotropic scale, no optical qualification claimed"},
        "coverage_factor": 2,
        "correlation_matrix": [[1 if i == j else 0 for j in range(5)] for i in range(5)],
        "correlation_justification": "Independent inputs assumed explicitly for the synthetic test",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path)
    parser.add_argument("--test-port", type=int, required=True)
    args = parser.parse_args()
    if not 18765 <= args.test_port <= 65535:
        parser.error("Use an owned test host >=18765, never the user instance.")
    folder = args.folder.resolve()
    folder.mkdir(parents=True, exist_ok=False)
    sentinel = folder / "sentinel.png"
    Image.new("RGB", (8, 8), (80, 80, 80)).save(sentinel)
    client = Mcp(folder, args.test_port)
    client.call("open_image", {"path": str(sentinel)})
    baseline = client.call("get_measurement_context")
    inspection = client.call("get_inspection_state")
    results = []

    def unchanged():
        assert client.call("get_measurement_context") == baseline
        assert client.call("get_inspection_state") == inspection

    independent = example()
    correlated = copy.deepcopy(independent)
    correlated["correlation_matrix"][0][2] = correlated["correlation_matrix"][2][0] = .5
    correlated["correlation_justification"] = "Explicit rho(x1,x2)=0.5, other inputs independent"
    translation = copy.deepcopy(independent)
    for i, j in ((0, 2), (1, 3)):
        translation["correlation_matrix"][i][j] = translation["correlation_matrix"][j][i] = 1
    translation["correlation_justification"] = "Exact shared x/y translations; independent scale"
    rotated = copy.deepcopy(independent)
    rotated["second_point"]["position_px"] = {"x": -40, "y": 30}
    for name, request, expected_variance in (
        ("independent_distance", independent, 33e-6),
        ("correlated_x_coordinates", correlated, 31.56e-6),
        ("common_translation_cancels_not_scale", translation, 25e-6),
        ("rotated_isotropic_model", rotated, 33e-6),
    ):
        result = client.call("calculate_distance_uncertainty", request)
        assert math.isclose(result["distance_px"], 50, rel_tol=1e-12)
        assert math.isclose(result["distance_mm"], .5, rel_tol=1e-12)
        budget = result["budget"]
        expected_uc = math.sqrt(expected_variance)
        assert math.isclose(budget["combined_standard_uncertainty"], expected_uc, rel_tol=1e-12)
        assert math.isclose(budget["expanded_uncertainty"], 2 * expected_uc, rel_tol=1e-12)
        assert budget["output_unit"] == "millimeter"
        assert [c["input_unit"] for c in budget["components"]] == ["pixel"] * 4 + ["dimensionless"]
        assert result["request"] == request
        assert result["status"] == "calculation_only"
        assert all(result[key] is False for key in ("applied_to_measurement", "source_qualification_verified",
                                                   "conformity_evaluated", "input_provenance_verified"))
        if name == "independent_distance":
            expected_coefficients = [-.006, -.008, .006, .008, .5]
            assert all(math.isclose(c["sensitivity_coefficient"], expected, rel_tol=1e-12)
                       for c, expected in zip(budget["components"], expected_coefficients))
        unchanged()
        results.append({"case": name, "passed": True, "request": request, "response": result})
    for name in ("coincident_points", "zero_scale", "negative_coordinate_u", "negative_relative_u",
                 "blank_source", "zero_coverage", "wrong_matrix_size", "asymmetric_matrix",
                 "indefinite_matrix", "forged_nominal"):
        request = example()
        if name == "coincident_points": request["second_point"]["position_px"] = {"x": 0, "y": 0}
        elif name == "zero_scale": request["scale"]["millimeters_per_pixel"] = 0
        elif name == "negative_coordinate_u": request["first_point"]["standard_uncertainty_x_px"] = -.2
        elif name == "negative_relative_u": request["scale"]["relative_standard_uncertainty"] = -.01
        elif name == "blank_source": request["scale"]["source"] = " "
        elif name == "zero_coverage": request["coverage_factor"] = 0
        elif name == "wrong_matrix_size": request["correlation_matrix"].pop()
        elif name == "asymmetric_matrix": request["correlation_matrix"][0][2] = .5
        elif name == "indefinite_matrix":
            matrix = request["correlation_matrix"]
            matrix[0][1] = matrix[1][0] = .9
            matrix[0][2] = matrix[2][0] = .9
            matrix[1][2] = matrix[2][1] = -.9
        else: request["nominal"] = .5
        response = client.rpc("tools/call", {"name": "calculate_distance_uncertainty", "arguments": request}, return_envelope=True)
        assert "error" in response or response.get("result", {}).get("isError") is True, (name, response)
        unchanged()
        results.append({"case": name, "passed": True, "request": request, "response": response})
    summary = {"passed": len(results), "failed": 0, "session_unchanged": True, "cases": results}
    (folder / "summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")
    print(json.dumps({key: summary[key] for key in ("passed", "failed", "session_unchanged")}))


if __name__ == "__main__":
    main()
