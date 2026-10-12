Paired diagnostic measurement, 2026-10-12, system Qt 6.12.0 for helper tests.

`paired-summary.json` and `paired-samples.jsonl` compare the preserved pre-P1
release executable with the initial P1 release candidate (before the later
Qt bearing and automatic subtitle erasure changes) over 16 screenshots ×
3 rounds. Both receive the same explicit synthetic fitting translation, use
ApproxMeasure and alternate order. SHA256, CPU and platform are retained.
No concurrent builds/OCR were requested during measurement; CPU governor and
boost were not pinned. Detector metadata was unchanged; placement/drop/cache
metadata changed, so this is not a pure performance equivalence comparison.

Baseline → candidate median: first frame 75.168 → 76.508 ms; repeated frame
24.560 → 25.117 ms; placement 2.176 → 2.541 ms; cached placement 0.387 →
0.717 ms. Peak process RSS median 54.381 → 56.438 MiB, max 58.457 → 60.504 MiB.
The measurements exclude actual OCR, translation engines, Qt layout transport,
QML rendering and compositor presentation. They do not establish whole-pipeline
speed or a performance improvement.

Separate local verification: main QML 126 passed; inplace QML 18 passed; ignored
Qt placement regressions 2 passed (6 other tests filtered). These are test
results, not compositor latency measurements. Production Qt text-helper binary
was compiled and executed; its raw helper sample remains under tmp.

The subsequent shared-header regression is in `ink-bounds-before.log` and
`ink-bounds-after.log`: old helper 1 failed/1 passed; corrected helper 2 passed.
The test compares real C++ measurement against actual production QML and retains
the existing one-pixel rounding threshold. Background erasure regression logs
show old AutoSubtitle behavior failed both relevant cases; corrected behavior
passed all four background tests. `actual-source15-after-rendered.png` and its
metadata retain the subsequent actual RapidOCR → local Bergamot render: five
blocks rendered, with 17 previously cached actual provider phrases. The main
dialogue has an opaque erase plate and its original English glyphs are obscured;
unrecognized HUD text outside the replacement fields remains visible. The
structural checks passed. This is an offscreen QML image, not a compositor probe.
