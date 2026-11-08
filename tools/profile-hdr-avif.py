"""Repeat the real bounded encoder with explicit stage timings, without a display.

Run different builds sequentially with the same sizes/seed. Compressed output
hashes and unchanged memory gates accompany timings; a faster run alone never
qualifies a different encoder. Provider includes nested read/convert timings.
"""
import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--probe", type=Path, required=True)
    parser.add_argument("--label", required=True)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--large-repeats", type=int, default=1)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if min(args.repeats, args.large_repeats) < 1:
        parser.error("repeat counts must be positive")
    reports = []
    for width, height, repeats in [(3840, 2160, args.repeats), (8256, 5504, args.large_repeats)]:
        for repeat in range(repeats):
            with tempfile.TemporaryDirectory(prefix="yue-avif-profile-") as directory:
                path = Path(directory) / "noise.avif"
                result = subprocess.run([str(args.probe.resolve()), "--spool-resources", str(width), str(height), str(path)], capture_output=True, check=True)
                report = json.loads(result.stdout)
                stage_rows = [json.loads(line) for line in result.stderr.splitlines() if line.startswith(b'{"encode_profile":')]
                if len(stage_rows) != 1:
                    raise AssertionError("one real encoder stage report required")
                stages = stage_rows[0]["encode_profile"]
                if stages["encode"]["calls"] != report["tiles"] or stages["provider_inclusive"]["calls"] != report["tiles"]:
                    raise AssertionError("incomplete tile timings")
                if stages["header"]["calls"] != 1 or stages["publish"]["calls"] != 1:
                    raise AssertionError("incomplete publication timing")
                if report["extra_heap"] > 32 * 1024 * 1024 or report["largest_allocation"] >= 16 * 1024 * 1024:
                    raise AssertionError("original memory gate failed")
                if sum(v["ms"] for k, v in stages.items() if k not in ("source_read", "color_convert")) > report["elapsed_ms"] + 1:
                    raise AssertionError("stages overlap or exceed measured total")
                with path.open("rb") as source:
                    digest = hashlib.file_digest(source, "sha256").hexdigest()
                report.update(label=args.label, repeat=repeat, sha256=digest, stages=stages)
                reports.append(report)
                print(json.dumps(report), flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    summary = {}
    for width in (3840, 8256):
        samples = [r for r in reports if r["width"] == width]
        if len({r["sha256"] for r in samples}) != 1:
            raise AssertionError("identical input produced different output")
        summary[str(width)] = {"median_ms": statistics.median(r["elapsed_ms"] for r in samples), "runs": len(samples)}
    args.output.write_text(json.dumps(dict(label=args.label, seed="xorshift32-0x941cf17a-pq10-rgba", results=reports, summary=summary), indent=2) + "\n", encoding="utf8")


if __name__ == "__main__":
    main()
