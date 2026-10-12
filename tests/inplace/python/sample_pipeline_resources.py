"""Sample an actual running app and its subprocesses; optionally retain exported preview timings.

This records CPU/RSS, not compositor presentation latency. Preview JSON must be
exported by an app probe; timing fields are never reconstructed from sleep waits.
"""
import argparse
import json
import os
import math
import statistics
from pathlib import Path
import time

ROOT = Path(__file__).resolve().parents[3]


def process_tree(pid):
    rows = {}
    for proc in Path('/proc').iterdir():
        if not proc.name.isdigit():
            continue
        try:
            raw = (proc / 'stat').read_text()
            fields = raw[raw.rfind(')') + 2:].split()
            rows[int(proc.name)] = (int(fields[1]), int(fields[11]) + int(fields[12]), int(fields[21]))
        except (OSError, ValueError, IndexError):
            continue
    selected = {pid}
    while True:
        children = {p for p, row in rows.items() if row[0] in selected}
        expanded = selected | children
        if expanded == selected:
            break
        selected = expanded
    return {p: rows[p] for p in selected if p in rows}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('pid', type=int)
    parser.add_argument('--seconds', type=float, default=30)
    parser.add_argument('--interval', type=float, default=0.25)
    parser.add_argument('--preview-json', type=Path,
                        help='optional app-probe export of actual ocrPreviewJson')
    parser.add_argument('--output', type=Path, default=ROOT / 'tmp/placement-quality/performance/resources')
    args = parser.parse_args()
    output = args.output.resolve()
    if not output.is_relative_to((ROOT / 'tmp').resolve()) or args.seconds <= 0 or args.interval <= 0:
        parser.error('output must be under project tmp/; seconds and interval must be positive')
    output.mkdir(parents=True, exist_ok=True)
    ticks = os.sysconf('SC_CLK_TCK')
    page = os.sysconf('SC_PAGE_SIZE')
    start = last = time.monotonic()
    previous = {}
    samples = []
    with (output / 'samples.jsonl').open('w') as stream:
        while time.monotonic() - start < args.seconds:
            now = time.monotonic()
            rows = process_tree(args.pid)
            if args.pid not in rows:
                break
            delta = sum(max(0, row[1] - previous[p][1]) for p, row in rows.items() if p in previous)
            sample = {'elapsed_s': now - start, 'pids': sorted(rows),
                      'rss_sum_mib': sum(row[2] for row in rows.values()) * page / 1024**2,
                      'cpu_percent_one_core': 100 * delta / ticks / (now - last) if previous else None}
            if args.preview_json:
                try:
                    sample['actual_app_preview'] = json.loads(args.preview_json.read_text())
                except (OSError, ValueError) as error:
                    sample['preview_unavailable'] = str(error)
            stream.write(json.dumps(sample) + '\n')
            stream.flush()
            samples.append(sample)
            previous, last = rows, now
            time.sleep(args.interval)
    cpu_samples = [row['cpu_percent_one_core'] for row in samples if row['cpu_percent_one_core'] is not None]
    summary = {'pid': args.pid, 'samples': len(samples),
               'cpu_percent_one_core_mean': statistics.mean(cpu_samples) if cpu_samples else None,
               'cpu_percent_one_core_p50': statistics.median(cpu_samples) if cpu_samples else None,
               'cpu_percent_one_core_p95': sorted(cpu_samples)[math.ceil(len(cpu_samples) * .95) - 1] if cpu_samples else None,
               'scope': 'actual Linux process tree; RSS sum includes shared pages; CPU is sampled living processes, short-lived children can be missed',
               'presentation_latency': 'not measured',
               'rss_sum_peak_mib': max((row['rss_sum_mib'] for row in samples), default=None)}
    (output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
