"""Reproduce DPM-15 correlated scalar budgets using an owned MCP test host."""
import argparse
import copy
import json
import math
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from PIL import Image
from smoke_inspection_mcp import Mcp
from verify_uncertainty_budget_mcp import fixture_budget


def correlated_budget(rho=0.5, unit="millimeter"):
    request = fixture_budget(unit)
    request.pop("independence_justification")
    request["components"][1]["sensitivity_coefficient"] = 1
    request["correlation_justification"] = "Synthetic explicit correlation model; software test only."
    request["correlation_matrix"] = [[1, rho], [rho, 1]]
    return request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path)
    parser.add_argument("--test-port", type=int, required=True)
    args = parser.parse_args()
    if not 18765 <= args.test_port <= 65535:
        parser.error("Use an owned dedicated host >= 18765, never the user instance.")
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

    cases = []
    for rho in (0, .5, -.5, 1, -1):
        cases.append((f"rho_{rho}", correlated_budget(rho), math.sqrt(.03**2 + .04**2 + 2*rho*.03*.04)))
    for unit in ("degree", "pixel"):
        cases.append((unit, correlated_budget(.5, unit), math.sqrt(.0037)))
    negative = correlated_budget(.5)
    negative["components"][1]["sensitivity_coefficient"] = -1
    cases.append(("signed_sensitivity", negative, math.sqrt(.0013)))
    for name, request, expected in cases:
        result = client.call("calculate_correlated_uncertainty_budget", request)
        calculation = result["calculation"]
        assert result["status"] == "calculation_only"
        assert not any(result[key] for key in ("applied_to_measurement", "source_qualification_verified", "conformity_evaluated"))
        assert math.isclose(calculation["combined_standard_uncertainty"], expected, rel_tol=1e-12)
        assert math.isclose(calculation["expanded_uncertainty"], 2*expected, rel_tol=1e-12)
        assert calculation["output_unit"] == request["output_unit"]
        assert calculation["correlation_matrix"] == request["correlation_matrix"]
        assert calculation["components"][1]["sensitivity_coefficient"] == request["components"][1]["sensitivity_coefficient"]
        unchanged()
        results.append({"case": name, "passed": True, "request": request, "result": result})

    invalid = []
    for name, updates in (
        ("missing_justification", {"correlation_justification": " "}),
        ("wrong_shape", {"correlation_matrix": [[1, .5]]}),
        ("asymmetric", {"correlation_matrix": [[1, .5], [.4, 1]]}),
        ("wrong_diagonal", {"correlation_matrix": [[.9, .5], [.5, 1]]}),
        ("out_of_bounds", {"correlation_matrix": [[1, 1.1], [1.1, 1]]}),
        ("zero_coverage", {"coverage_factor": 0}),
        ("forged_qualification", {"qualified": True}),
    ):
        invalid.append((name, dict(correlated_budget(), **updates)))
    indefinite = correlated_budget()
    third = copy.deepcopy(indefinite["components"][1])
    third.update(name="third", sensitivity_coefficient=0)
    indefinite["components"].append(third)
    indefinite["correlation_matrix"] = [[1, -.9, -.9], [-.9, 1, -.9], [-.9, -.9, 1]]
    invalid.append(("indefinite_despite_positive_requested_variance", indefinite))
    cancelled = correlated_budget(-1)
    cancelled["components"][1]["standard_uncertainty"] = .03
    invalid.append(("zero_after_cancellation", cancelled))
    for name, request in invalid:
        response = client.rpc("tools/call", {"name": "calculate_correlated_uncertainty_budget", "arguments": request}, return_envelope=True)
        assert "error" in response or response.get("result", {}).get("isError") is True, (name, response)
        unchanged()
        results.append({"case": name, "passed": True, "rejected": True, "request": request, "response": response})
    summary = {"passed": len(results), "failed": 0, "session_unchanged": True, "cases": results}
    (folder / "summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")
    print(json.dumps({key: summary[key] for key in ("passed", "failed", "session_unchanged")}))


if __name__ == "__main__":
    main()
