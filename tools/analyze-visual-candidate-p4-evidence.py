"""Derive P4 measurements from immutable raw evidence; never certify the full stage."""
import argparse
import hashlib
import json
from pathlib import Path


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8-sig"))


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def percentile(values, percent):
    assert values
    return sorted(values)[(len(values) * percent + 99) // 100 - 1]


def describe(values):
    return {"samples": len(values), "min": min(values), "p50": percentile(values, 50),
            "p95": percentile(values, 95), "max": max(values),
            **({"p99_diagnostic_only": percentile(values, 99)} if len(values) >= 1000 else {})}


def spread(events, field):
    values = [event[field] for event in events]
    return {"min": min(values), "max": max(values), "range": max(values) - min(values)}


def resource_report(folder):
    summary = read_json(folder / "summary.json")
    assert summary["gatePassed"] and summary["sourceUnchanged"] and summary["exitCode"] == 0
    events = read_json(folder / "probe-events.json")
    all_cycles = [event for event in events if event["event"] == "cycle"]
    all_visual = [event for event in events if event["event"] == "visual"]
    cycles = [event for event in all_cycles if not event["warmup"]]
    idle = [event for event in events if event["event"] == "idle"]
    tail = [event for event in idle if event["second"] >= 40]
    visual = [event for event in all_visual if event["cycle"] >= 5]
    process = read_json(folder / "process-samples.json")
    assert len(cycles) == 100 and len(idle) == 61 and len(tail) == 21
    assert all(event["completed"] == 1 and event["unavailable"] == 0 for event in visual)
    result = {
        "summary_sha256": digest(folder / "summary.json"),
        "sessions": len(cycles), "visual_completed": len(visual),
        "peak_scope": "All events, including warmups and idle, plus external process samples; only latency percentiles exclude warmups.",
        "capture_us": describe([event["capture_us"] for event in cycles]),
        "last20s_idle": {key: spread(tail, key) for key in ("private_bytes", "working_set")},
        "last20sessions": {key: spread(cycles[-20:], key) for key in ("private_bytes", "working_set")},
        "observed_process_private_peak": max([event["private_bytes"] for event in all_cycles + all_visual + idle] + [event["privateBytes"] for event in process]),
        "os_process_peak_working_set": max(event["peakWorkingSet"] for event in process),
        "os_process_peak_paged_memory": max(event["peakPagedMemory"] for event in process),
        "idle_cpu_seconds": summary["idleCpuSeconds"],
        "shutdown": [event for event in events if event["event"] == "shutdown"],
        "source_sha256": summary["sourceSha256"],
    }
    gpu = [event["active_gpu"] for event in all_cycles] + [event["released_gpu"] for event in all_cycles] + [event["gpu"] for event in idle]
    result["observed_gpu_usage_peak"] = [max(value[index] for value in gpu) for index in range(2)]
    result["last20s_idle_gpu_range"] = [max(event["gpu"][index] for event in tail) - min(event["gpu"][index] for event in tail) for index in range(2)]
    if visual:
        result["source_cold_query_us"] = describe([event["query_us"] for event in visual])
        result["source_cold_controller_us"] = describe([event["controller_us"] for event in visual])
        result["max_cpu_buffer_bound"] = max(event["buffer_bound"] for event in visual)
        assert result["source_cold_query_us"]["p95"] <= 25000
        assert result["source_cold_controller_us"]["p95"] <= 25000
        assert result["max_cpu_buffer_bound"] <= 16 * 1048576
    assert result["shutdown"][0]["visual_workers_balanced"]
    result["zero_range_last20s"] = all(value["range"] == 0 for value in result["last20s_idle"].values()) and result["last20s_idle_gpu_range"] == [0, 0]
    result["scope"] = "100 new captures/sources with precreated worker; not 100 process launches. OS peaks cover the complete test process, not pure visual allocations. Observed GPU usage includes the independent visible D2D host, capture and driver; explicit visual atlas is one 4MiB allocation."
    return result


def tagged(lines, tag):
    result = []
    for line in lines:
        marker = tag + " "
        if marker in line:
            payload = line.split(marker, 1)[1]
            if payload.startswith("{"):
                result.append(json.loads(payload))
    return result


def analyze(root):
    batch = root / "final-desktop-requalification"
    summary = read_json(batch / "summary.json")
    assert summary["checksPassed"] and summary["sourceUnchanged"]
    assert len(summary["results"]) == 6 and all(result["gatePassed"] for result in summary["results"])
    control = resource_report(root / "qualified-controlled-control")
    positive = resource_report(root / "qualified-controlled-positive")
    for name, source_hash in summary["sourceSha256"].items():
        assert digest(Path(name)) == source_hash, name
        if name.endswith(".rs"):
            assert positive["source_sha256"][name] == control["source_sha256"][name] == source_hash, name
    control.pop("source_sha256")
    positive.pop("source_sha256")
    assert control["visual_completed"] == 0 and positive["visual_completed"] == 100
    lines = (batch / "product-4k-present.txt").read_text(encoding="utf-8-sig").splitlines()
    latency = tagged(lines, "PRODUCT_LATENCY")
    present = tagged(lines, "PRODUCT_PRESENT")
    surfaces = tagged(lines, "VISUAL_PRESENT")
    assert len(latency) == 12 and len(present) == len(surfaces) == 4
    rows = []
    for row in latency:
        assert row["actual_samples"] >= 1000 and row["render_competition"]
        query, owner, hot = (describe(row[key]) for key in ("end_to_end_ns", "owner_event_ns", "hot_ns"))
        assert query["p95"] == row["p95_ns"] <= 25000000
        assert owner["p95"] == row["owner_p95_ns"] <= 1000000
        assert hot["p95"] == row["hot_p95_ns"] < 100000
        rows.append({"fixture": row["fixture"], "session": row["session"], "query_ns": query, "owner_ns": owner, "hot_ns": hot})
    for fixture in ("T01", "T21", "T10", "T10-8k"):
        assert sorted(row["session"] for row in rows if row["fixture"] == fixture) == [0, 1, 2]
    display = []
    for measured, surface in zip(present, surfaces):
        baseline = describe(surface["baseline_display_intervals_us"])
        active = describe(surface["analysis_display_intervals_us"])
        assert baseline["samples"] >= 1000 and active["samples"] >= 1000
        delta = active["p95"] - baseline["p95"]
        assert abs(delta - measured["display_p95_delta_us"]) < .000001 and delta <= 1000
        display.append({"baseline_us": baseline, "analysis_us": active, "p95_delta_us": delta, "surface": surface["surface"]})
    return {"scope": "Measured current-source desktop subgates only; not a complete P4, hardware matrix or independent accuracy qualification", "canAdvance": False,
            "analyzer_sha256": digest(Path(__file__)), "batch_summary_sha256": digest(batch / "summary.json"),
            "control": control, "positive": positive, "warm_product_latency": rows, "display_competition": display,
            "observed_private_peak_positive_minus_control": positive["observed_process_private_peak"] - control["observed_process_private_peak"],
            "peak_attribution_limit": "Process and driver measurements include existing capture/rendering and test instrumentation; do not label their difference a strict pure-visual allocation peak."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("evidence_directory", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    if args.output.exists():
        parser.error("Use a new output path; previous evidence must remain intact.")
    result = analyze(args.evidence_directory)
    args.output.write_bytes((json.dumps(result, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))
    print("Current-source capture/resource/4K measurements validated; full P4 remains incomplete.")


if __name__ == "__main__":
    main()
