"""Verify explicit scalar budgets on a dedicated owned MCP test host."""
import argparse
import copy
import json
import math
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from PIL import Image
from smoke_inspection_mcp import Mcp


def fixture_budget(output_unit="millimeter"):
    return {
        "output_unit": output_unit,
        "coverage_factor": 2,
        "independence_justification": "Synthetic independent inputs, analytic software check only.",
        "components": [
            {"name": "localization", "input_unit": "pixel", "standard_uncertainty": .3,
             "sensitivity_coefficient": .1, "source": "Synthetic 0.1 output units per pixel"},
            {"name": "reference", "input_unit": output_unit, "standard_uncertainty": .04,
             "sensitivity_coefficient": -1, "source": "Synthetic independent reference"},
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path)
    parser.add_argument("--test-port", type=int, required=True)
    args = parser.parse_args()
    if not 18765 <= args.test_port <= 65535:
        parser.error("Use a dedicated owned host port >= 18765, never the user instance.")
    folder = args.folder.resolve()
    folder.mkdir(parents=True, exist_ok=False)
    sentinel = folder / "session-sentinel.png"
    Image.new("RGB", (8, 8), (100, 100, 100)).save(sentinel)
    client = Mcp(folder, args.test_port)
    client.call("open_image", {"path": str(sentinel)})
    baseline = client.call("get_measurement_context")
    tool = next(t for t in client.rpc("tools/list", {})["tools"] if t["name"] == "calculate_uncertainty_budget")
    assert tool["inputSchema"]["additionalProperties"] is False
    results = []

    def unchanged():
        after = client.call("get_measurement_context")
        for key in ("capture_id", "document_revision", "measurements", "calibration", "can_undo", "can_redo"):
            assert after[key] == baseline[key], key

    for unit in ("millimeter", "degree", "pixel"):
        result = client.call("calculate_uncertainty_budget", fixture_budget(unit))
        assert result["status"] == "calculation_only"
        assert not any(result[key] for key in ("applied_to_measurement", "source_qualification_verified", "conformity_evaluated"))
        value = result["calculation"]
        assert value["output_unit"] == unit
        assert math.isclose(value["combined_standard_uncertainty"], .05, rel_tol=1e-12)
        assert math.isclose(value["expanded_uncertainty"], .1, rel_tol=1e-12)
        assert value["components"][0]["sensitivity_input_unit"] == "pixel"
        assert value["components"][0]["sensitivity_output_unit"] == unit
        assert value["components"][1]["sensitivity_coefficient"] == -1
        unchanged()
        results.append({"case": unit, "expanded_uncertainty": value["expanded_uncertainty"], "passed": True})

    invalid = []
    for name, path, value in (
        ("negative_standard_uncertainty", ("components", 0, "standard_uncertainty"), -.1),
        ("zero_coverage", ("coverage_factor",), 0),
        ("missing_independence", ("independence_justification",), " "),
        ("unknown_input_unit", ("components", 0, "input_unit"), "radian"),
        ("unsupported_correlation", ("correlations",), [[1, .5], [.5, 1]]),
        ("forged_qualification", ("qualified",), True),
        ("duplicate_name", ("components", 1, "name"), " localization "),
        ("overflow", ("components", 0, "sensitivity_coefficient"), 1e309),
    ):
        request = copy.deepcopy(fixture_budget())
        target = request
        for key in path[:-1]:
            target = target[key]
        # JSON has no infinity; exercise overflow with finite factors instead.
        if name == "overflow":
            request["components"][0]["standard_uncertainty"] = 1e308
            value = 10
        target[path[-1]] = value
        invalid.append((name, request))
    for name, request in invalid:
        envelope = client.rpc("tools/call", {"name": "calculate_uncertainty_budget", "arguments": request}, return_envelope=True)
        assert "error" in envelope or envelope.get("result", {}).get("isError") is True, (name, envelope)
        unchanged()
        results.append({"case": name, "rejected": True, "passed": True})
    summary = {"cases": results, "session_unchanged": True, "journal": str(client.log_path)}
    (folder / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
