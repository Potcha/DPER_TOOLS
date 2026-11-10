"""DPM-15 three-point angle model: synthetic checks on an owned MCP host."""
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
    def point(x, y, label):
        return {"position_px": {"x": x, "y": y}, "standard_uncertainty_x_px": .2,
                "standard_uncertainty_y_px": .2, "source": "Synthetic " + label}
    return {"first_point": point(40, 0, "first point"), "vertex": point(0, 0, "vertex"),
            "second_point": point(0, 40, "second point"), "coverage_factor": 2,
            "correlation_matrix": [[int(i == j) for j in range(6)] for i in range(6)],
            "correlation_justification": "Explicit independent coordinate inputs for synthetic recipe"}


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
    correlated["correlation_matrix"][1][3] = correlated["correlation_matrix"][3][1] = .5
    correlated["correlation_justification"] = "Explicit rho(y1,yv)=0.5; other inputs independent"
    reversed_points = copy.deepcopy(independent)
    reversed_points["first_point"], reversed_points["second_point"] = reversed_points["second_point"], reversed_points["first_point"]
    mirrored = copy.deepcopy(independent)
    mirrored["second_point"]["position_px"]["y"] = -40
    for name, request, variance_rad in (("independent_90_degrees", independent, .0001),
                                       ("correlated_vertex", correlated, .000075),
                                       ("reversed_rays", reversed_points, .0001),
                                       ("negative_orientation", mirrored, .0001)):
        result = client.call("calculate_angle_uncertainty", request)
        assert math.isclose(result["angle_degrees"], 90, abs_tol=1e-12)
        assert math.isclose(result["first_ray_length_px"], 40, rel_tol=1e-12)
        assert math.isclose(result["second_ray_length_px"], 40, rel_tol=1e-12)
        budget = result["budget"]
        expected_uc = math.degrees(math.sqrt(variance_rad))
        assert math.isclose(budget["combined_standard_uncertainty"], expected_uc, rel_tol=1e-12)
        assert math.isclose(budget["expanded_uncertainty"], 2 * expected_uc, rel_tol=1e-12)
        assert budget["output_unit"] == "degree"
        assert [c["input_unit"] for c in budget["components"]] == ["pixel"] * 6
        assert result["request"] == request
        assert result["input_order"] == ["x1", "y1", "xv", "yv", "x2", "y2"]
        assert result["status"] == "calculation_only"
        assert all(result[k] is False for k in ("applied_to_measurement", "source_qualification_verified",
                                               "conformity_evaluated", "input_provenance_verified"))
        if name == "independent_90_degrees":
            expected = [0, -1/40, 1/40, 1/40, -1/40, 0]
            assert all(math.isclose(c["sensitivity_coefficient"], math.degrees(v), abs_tol=1e-12)
                       for c, v in zip(budget["components"], expected))
        unchanged()
        results.append({"case": name, "passed": True, "request": request, "response": result})
    for name in ("zero_ray", "collinear_zero", "collinear_straight", "negative_coordinate_u",
                 "blank_source", "zero_coverage", "wrong_matrix_size", "asymmetric_matrix",
                 "indefinite_matrix", "forged_scale", "forged_nominal", "unknown_nested_coordinate"):
        request = example()
        if name == "zero_ray": request["first_point"]["position_px"] = {"x": 0, "y": 0}
        elif name == "collinear_zero": request["second_point"]["position_px"] = {"x": 80, "y": 0}
        elif name == "collinear_straight": request["second_point"]["position_px"] = {"x": -80, "y": 0}
        elif name == "negative_coordinate_u": request["vertex"]["standard_uncertainty_x_px"] = -.2
        elif name == "blank_source": request["vertex"]["source"] = " "
        elif name == "zero_coverage": request["coverage_factor"] = 0
        elif name == "wrong_matrix_size": request["correlation_matrix"].pop()
        elif name == "asymmetric_matrix": request["correlation_matrix"][0][2] = .5
        elif name == "indefinite_matrix":
            matrix = request["correlation_matrix"]
            matrix[0][1] = matrix[1][0] = .9
            matrix[0][2] = matrix[2][0] = .9
            matrix[1][2] = matrix[2][1] = -.9
        elif name == "unknown_nested_coordinate": request["vertex"]["position_px"]["nominal"] = 90
        else: request["scale" if name == "forged_scale" else "nominal"] = 90
        response = client.rpc("tools/call", {"name": "calculate_angle_uncertainty", "arguments": request}, return_envelope=True)
        assert "error" in response or response.get("result", {}).get("isError") is True, (name, response)
        unchanged()
        results.append({"case": name, "passed": True, "request": request, "response": response})
    summary = {"passed": len(results), "failed": 0, "session_unchanged": True, "cases": results}
    (folder / "summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")
    print(json.dumps({k: summary[k] for k in ("passed", "failed", "session_unchanged")}))


if __name__ == "__main__":
    main()
