"""Paired release diagnostics; not an OCR-accuracy or compositor benchmark.

Run with two release inplace_bench executables, saved before and after changes.
Images and results are written exclusively below the project's tmp directory.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess

from PIL import Image

ROOT = Path(__file__).resolve().parents[3]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--output", type=Path, default=ROOT / "tmp/phase23/paired-benchmark")
    args = parser.parse_args()
    output = args.output.resolve()
    if not output.is_relative_to((ROOT / "tmp").resolve()) or args.rounds < 1:
        parser.error("output must be below project tmp/ and rounds must be positive")
    output.mkdir(parents=True, exist_ok=True)
    binaries = {"baseline": args.baseline.resolve(), "candidate": args.candidate.resolve()}
    env = os.environ.copy()
    # Both binaries use the same ApproxMeasure path for comparable CPU diagnostics.
    env.pop("LIPAX_QT_MEASURE_SERVER", None)
    env.pop("LIPAX_DIAGNOSTICS_DIR", None)
    results = {key: [] for key in binaries}
    with (output / "samples.jsonl").open("w") as log:
        sources = sorted(p for p in (ROOT / "docs/screenshots/test-game").iterdir()
                         if p.is_file() and (p.suffix.lower() in {".png", ".jpg", ".jpeg"} or not p.suffix))
        for source in sources:
            rgba = Image.open(source).convert("RGBA")
            png = output / (source.stem + ".png")
            Image.alpha_composite(Image.new("RGBA", rgba.size, (0, 0, 0, 255)), rgba).save(png)
            for round_index in range(args.rounds):
                # Alternate order to reduce systematic warmup/order bias.
                order = list(binaries) if round_index % 2 == 0 else list(reversed(binaries))
                pair = {}
                for key in order:
                    data = json.loads(subprocess.check_output([str(binaries[key]), str(png)],
                        env=env, text=True, timeout=120))
                    data.update(source=source.name, implementation=key, round=round_index)
                    results[key].append(data)
                    pair[key] = data
                    log.write(json.dumps(data) + "\n")
                    log.flush()
                for field in ("blocks", "placed", "dropped", "fits", "hits"):
                    assert pair["baseline"][field] == pair["candidate"][field], (source.name, field)
    summary = {"rounds": args.rounds, "images": len(results["baseline"]) // args.rounds,
        "binary_sha256": {key: hashlib.sha256(path.read_bytes()).hexdigest() for key, path in binaries.items()},
        "platform": platform.platform(), "python": platform.python_version(),
        "cpu": next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines()
                     if line.startswith("model name")), "unknown"),
        "conditions": "release binaries, no concurrent benchmark jobs, CPU boost/governor unpinned; ApproxMeasure placement",
        "exact_metadata_and_placement_equal": True}
    for key, rows in results.items():
        samples = {}
        for label, column in [("detection_ms", 0), ("glyph_ms", 1), ("matching_ms", 2)]:
            samples[label] = statistics.median(statistics.median(t[column] for t in row["timings_ms"]) for row in rows)
        for label in ("first_frame_ms", "repeat_frame_ms", "placement_ms", "cached_placement_ms"):
            samples[label] = statistics.median(row[label] for row in rows)
        samples["peak_memory_median_mib"] = statistics.median(int(row["peak_memory"].split()[1]) / 1024 for row in rows)
        samples["peak_memory_max_mib"] = max(int(row["peak_memory"].split()[1]) / 1024 for row in rows)
        summary[key] = samples
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
