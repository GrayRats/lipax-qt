# Inplace regression suite

## Live overlay regression, 1.3.9

The final test results and retained evidence are in
[`tests/overlay/README.md`](../overlay/README.md).
New sources use the existing `tests/inplace/` convention; intermediate outputs
are isolated under `tmp/overlay-bugfix/`.

`qml/tst_overlaylifecycle.qml` verifies that settings echoes, translation updates
and switching the persistent window surfaces preserve a native resize. It also
checks the remembered-size setting and explicit size edits.

The following opt-in tests require the actual user's KWin session and PySide6:

```sh
cargo build --offline -p lipa --bin lipax --features lifecycle-test
python tests/inplace/python/kwin_geometry_probe.py
QT_QPA_PLATFORM=wayland python tests/inplace/python/live_overlay_pipeline.py
QT_QPA_PLATFORM=xcb python tests/inplace/python/live_overlay_pipeline.py
python tests/inplace/python/live_floating_window.py
```

The current live pipeline explicitly selects RapidOCR and local Bergamot en→ru.
It validates the actual engine and installed model paths; it sends no test text
to an external translator. Select the built application with
`LIPAX_TEST_EXECUTABLE`, the translator with `BERGAMOT_BINARY`, and the installed
pair with `LIPAX_BERGAMOT_TEST_MODELS`. Older live evidence used Tesseract and
Google Translate; those historical results are distinct from a new RapidOCR run.
The probe requires Spectacle and the application's normal ScreenShot2 desktop-entry authorization.
For a local binary, use the documented `packaging/run-local.sh` registration.
Only generated test-owned source windows are captured or moved. Translation
data is produced by the real pipeline, not injected. Real QQuickWindow buffers
are checked for glyphs; on one physical output, a compositor screenshot verifies
that both independent field buffers appear at their reported coordinates.
The opt-in lifecycle hook reuses its existing 500 ms snapshot timer to enable
and export the existing OCR preview, retaining real stage p50/p95 and sample
counts. The sampler records actual app/child CPU and RSS; it includes test-hook
readback and export overhead. Compositor image comparisons prove visibility and
coordinates, not presentation latency. Movement must redraw fields without
another translation request. A changed
phrase must produce a new translation. Shutdown must exit normally.

The floating probe uses XTest on the application's XCB top-level. Exit code 77
means KWin did not accept the synthetic pointer motion; this leaves dragging
unverified. A physical pointer is required for native Wayland movement and
resizing validation. `QT_SCALE_FACTOR` exercises Qt coordinate arithmetic only;
it does not validate compositor fractional scaling or multiple physical outputs.

## Dialogue change replay (CrossxMars.exe)

- `rust/dialogue_change.rs` (`inplace_dialogue_change`, registered in
  `crates/core/Cargo.toml`): replays the live-recorded frames
  `fixtures/CrossxMars.exe-live-{mika,d1,d2,d3}.png` with their recorded RapidOCR
  readings through one engine. Checks name and dialogue replacement, no overlaps,
  fields inside the region, and withdrawal of the old translation. `LIPAX_TEST_TRACE=1`
  prints per-scan traces.
- `crates/core/examples/record_dialogues.rs`: maintainer recorder for such a sequence
  (`cargo run -p lipa-core --example record_dialogues CrossxMars.exe-live-d1 ...`). It
  writes `fixtures/<fixture>-local-rapidocr.json`, replacing the previous readings.
  `crates/core/examples/record_replay.rs` is the older single-frame recorder.
- `rust/ocr_ownership.rs`: `a_dialogue_line_cut_in_two_by_the_detector_is_joined_and_not_left_as_overlapping_parts`
  (shared-row join, recorded on the d1 frame).

## Existing offline suite

All new task-specific tests live here. Existing application and Kizurium tests
remain in their original directories. Cargo registers `rust/regression.rs` and
`rust/bench.rs` explicitly in `crates/core/Cargo.toml`.

Run `bash tests/inplace/run.sh` from the repository. It writes all builds,
bytecode, caches, logs, images and measurements under `tmp/`. Dependencies:
Rust dependencies already cached for offline Cargo, system Qt >=6.12 and <7 development tools,
C++ compiler, Pillow, pytest, PySide6, ONNX Runtime and cached RapidOCR models.
Use `LIPAX_RAPID_TEST_MODELS` and `LIPAX_RAPID_TEST_ORT` to select existing
models/runtime; models are not downloaded by this suite. CPU, two threads, no GPU.
Existing HTTP mock tests need permission to bind loopback sockets.

- `rust/`: coordinate, appearance hysteresis, confidence, empty-input/OCR-line
  normalization, connectivity and slant regressions; diagnostic screenshot executable.
- `qml/`: QML lifecycle/geometry and the production JavaScript coordinate helper.
- `qt/`: production QTextLayout helper tests, measurement timings and a test-only
  JSON-lines server for Rust fitting with the shared production Qt helper.
- `python/`: 16 real screenshot pipeline cases, bundled multilingual glyph coverage, and a negative visual comparator
  test. The initially extensionless `dialogue-en-12` JPEG was renamed to `.jpg`
  during the task; both names are supported. `dialogue-en-13.png` is 1821×1032, `-14` is 1867×1063, and `-15` is
  1732×996. Translucent PNG borders are composited onto black explicitly to
  provide an opaque capture-frame input.

The runner builds the Qt measurement server and sets `LIPAX_QT_MEASURE_SERVER`.
Rust screenshot placement then uses the same font constructor and QTextLayout
helper as the application, via test-only IPC. QML renders the resulting entries;
tests inspect actual line counts, content sizes and truncation as well as pixels.
This exercises Rust fitting and the production C++ helper together, but does not
execute the application's in-process CXX-Qt transport or a live compositor.
Without that environment variable, the executable uses `ApproxMeasure` for
CPU diagnostics; its metadata states which measurer was used. IPC timings must
not be reported as application layout latency. Translations are predefined, never sent to a service. OCR can run
independently with `LIPAX_TEST_OCR=1`; output is from LipaX's native RapidOCR
backend, not fixture coordinates. No silent fallback to another OCR backend is allowed. There are no OCR ground-truth annotations
in this dataset, so these tests make no claim about recognition accuracy.

Four additional Qt cases cover long dialogue, long words, hard breaks and mixed
shaping. A fifth case uses `--ocr-blocks`: actual cropped OCR jobs pass through
the application's RapidOCR backend, confidence/meaningfulness gates and
OCR-line refinement before receiving predefined translations. This verifies
transport and rejection gates, not correct transcription. RapidOCR reads the
outlined `dialogue-en-13` dialogue with 99.1% confidence in the observed run;
confidence alone is not proof of transcription or overlay correctness. The default screenshot cases deliberately translate
every detection candidate (including scenery) to stress fitting and clipping.

Artifacts: original image, detected candidates, typography metadata, actual
QML-rendered image, actual layout/overflow diagnostics, outside-boundary difference,
OCR lines and timings.
`render_ms` includes an intentional settling wait; `update_ms` is a synchronous
offscreen update/paint sample, not compositor latency or a delegate-lifecycle
benchmark. Detection candidates can include non-text scenery.

For an independently reviewed reference set, set `LIPAX_VISUAL_REFERENCES` to a
directory containing `<screenshot-stem>.png`. The suite saves differences and
requires channel changes above 8 on no more than 0.1% of pixels. Pin Qt, fonts,
DPI, and render backend when approving references. References are never created
or approved automatically. Without references, only structural and outside-field
pixel invariants are checked; approved-image regression is NOT RUN.

For a paired CPU comparison, save the release `inplace_bench` executable before
editing sources, rebuild it afterwards, and run (with the same cache environment):

```sh
python tests/inplace/python/benchmark.py tmp/phase23/baseline/inplace_bench tmp/cargo-target/release/examples/inplace_bench
```

This alternates baseline/candidate order over three rounds on all screenshots,
records metadata/placement differences, and writes raw samples and summary into
`tmp/phase23/paired-benchmark/`. Both binaries use ApproxMeasure for equivalent
placement conditions. Both binaries receive the same explicit synthetic translation;
this does not run OCR or translation engines. Use `--require-identical` only for
changes expected to preserve all metadata and placement. Differences are saved
without converting a quality change into a speed-only success. Image candidate
correctness still needs labeled data.

The runner respects a supplied `CARGO_TARGET_DIR`, defaults to offline Cargo,
runs workspace tests (explicitly excludes the legacy real Tesseract recognition
case so actual OCR validation uses RapidOCR only), both QML suites, the production
Qt text helper, and the
ignored Qt placement tests with the measurement server. `LIPAX_TEST_OCR=1` is
the default and requires actual RapidOCR models. It also explicitly runs the
opt-in `rapid_native` English/Russian/Japanese recognition test, without the
legacy Tesseract/Paddle comparison benchmark. Those generated clean-line
recognition thresholds are separate from screenshot geometry checks. Set
`LIPAX_BERGAMOT_TEST_BINARY` and `LIPAX_BERGAMOT_TEST_MODELS` to run the native
Bergamot opt-in test; absent paths produce a visible NOT RUN message. Selecting `0` must visibly
report model-backed tests as NOT RUN; it never enables another OCR backend.

`.github/workflows/checks.yml` defines an Arch structural job with system Qt
([official qt6-base package](https://archlinux.org/packages/extra/x86_64/qt6-base/)
reported 6.12.0-2 when checked on 2026-10-12),
a cached Cargo target and explicit model-test exclusion. A manually selected
`model_tests` job runs the full suite on a provisioned self-hosted runner labeled
`lipax-rapidocr`; provide model/runtime paths via its environment. It does not
require translation API keys or send screenshot text to Google. The workflow
has been authored locally; a GitHub Actions execution is separate evidence.
The provisioned self-hosted runner must support Node 24 actions (runner >=2.327.1).
Action versions and options follow the maintained primary documentation:
[checkout](https://github.com/actions/checkout/blob/main/README.md),
[cache](https://github.com/actions/cache/blob/main/README.md), and
[artifact upload](https://github.com/actions/upload-artifact/blob/main/README.md).


Authorized live captures of `engine.exe` and `CrossxMars.exe` can be collected
with `python tests/inplace/python/live_capture.py --capture`. This requires the
host session DBus/KWin connection. Its temporary KWin script is unloaded and
initial minimized/focus state restored. Capture uses Spectacle's registered
KWin backend, not the application's own capture transport. Set
`LIPAX_LIVE_CAPTURE_MANIFEST=tmp/rapid-live/capture/windows.json` to include the
two optional live-frame cases. The current fixture explicitly checks a 1:1
capture/logical scale; other DPRs need a corresponding rendering fixture.

`overlay_quality` records accepted OCR that was not rendered and fallback modes.
These diagnostics expose missing translations even when clipping tests pass.
They do not constitute an annotated OCR/placement accuracy score. Inspect the
rendered image and the missing-block list together.

Set `LIPAX_SAVE_TRANSLATIONS=1` to save user-requested permanent rendered PNGs,
UTF-8 translated text and JSON diagnostics in
`docs/screenshots/test-game/translations/`. Permanent export now requires actual accepted RapidOCR text and a translation-
engine response cache (`--service-cache`, local Bergamot by default). Missing
provider responses fail rather than receiving substitute text. Earlier retained
synthetic exports remain historical diagnostics; neither form is an approved
visual reference.
All intermediate artifacts still stay in `tmp/`. Dataset discovery excludes
this output subdirectory.


The recorded engine.exe case also requires both observed dialogue readings to
be rendered over at least 90% of their OCR boxes, without beside-original labels.
New Rust cases cover word-fragment reconstruction, prevention of neighbouring
OCR-padding overlap and exact cached geometry across an identical next frame.
The recorded CrossxMars.exe regression was rerun on the current source on
2026-10-12: the actual RapidOCR → Rust layout → shared Qt measurement → QML
render test passed. Name, complete dialogue and HUD replacements met the
existing source-coverage checks; `uncovered_text_blocks` was empty and
`repeat_ocr_jobs` was zero. This closes the previously reported recorded-frame
diagnostic failure within this fixture's coverage. It is an offscreen test
with predefined translations, not a live-game compositor test. Evidence is in
`tests/overlay/evidence/crossxmars/`.

The later source15 actual RapidOCR → local Bergamot render also verifies the
automatic subtitle erasure change: its main dialogue now has an opaque plate
over the original English glyphs. Five blocks rendered and the structural
checks passed; unrecognized HUD text outside those fields remains visible.
Before/after images, actual provider metadata and logs are retained in
`tests/overlay/evidence/placement-quality/performance/`. This is offscreen image
evidence; live compositor verification is recorded separately.

New files added in the 2026-10-11 session (Stage 3–5):

- `rust/detection.rs` (`inplace_detection`): block detector tests, `DetectionReport`
  with listed dropped blocks, the local contrast pass and the noise filters.
- `rust/typography.rs` (`inplace_typography`): typography measurement against ab_glyph
  outlines, `category_confidence` calibration over the fixtures and sequence tests
  (weight change, style change, block return).
- `rust/stress.rs` (`inplace_stress`): 200 frames × 12 blocks; checks tracked ≤ 12 and
  that RSS does not grow. Debug timings are not a benchmark.
- `rust/detect_dump.rs` (example `inplace_detect_dump`): diagnostic printer of the
  detector's blocks and report for a PNG/JPEG. It runs no OCR and makes no accuracy claim.
- `qt/typography_probe.cpp`: Qt probe used by the typography test; it runs in an
  environment without system fonts, so only the bundled fonts are available.
- `python/test_typography_qt.py`: 11 Qt tests. Without system fonts, Latin, Cyrillic, Greek
  and CJK come only from the bundled set and Thai gives `.notdef`. With system fonts, Thai
  and Arabic come from the OS.

For an actual running release application, `sample_pipeline_resources.py PID`
records Linux process-tree CPU samples and RSS under
`tmp/placement-quality/performance/`. Include Bergamot child processes by
sampling the app PID. `--preview-json PATH` retains an app probe's exported
`ocrPreviewJson`, including actual stage p50/p95 and sample counts. RSS sums
include shared pages and short-lived children can escape the sampling interval.
This resource sampler does not measure compositor presentation latency; a smoke
run against a shell is tool validation only, not application performance evidence.

The initial P1 2026-10-12 paired release diagnostic (before the later Qt bearing
and Auto subtitle background fixes) is retained in
[performance evidence](../overlay/evidence/placement-quality/performance/README.md).
Across 16 images × 3 rounds, detector metadata was equal while placement/drop
metadata changed. Median first/repeat frame samples rose from 75.168/24.560 to
76.508/25.117 ms; median peak RSS rose from 54.381 to 56.438 MiB. These are observed
diagnostic samples while the quality behavior differs in this synthetic workload,
not an end-to-end performance or translation-engine result.

`python/test_qt_ink_bounds.py` reproduces screenshot 10's negative-spacing,
right-aligned Source Serif field using the actual shared C++ measurement server
and production QML content. The old helper reported 39.65625 px while QML reported
41.640625 px. The revised helper unions positioned `naturalTextRect()` bounds,
matching [Qt 6.12's QQuickText implementation](https://github.com/qt/qtdeclarative/blob/v6.12.0/src/quick/items/qquicktext.cpp).
The regression also verifies that unwrapped longest-word measurements remain
finite. Existing screenshot width/height thresholds remain unchanged.
