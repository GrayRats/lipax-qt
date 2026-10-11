# Inplace regression suite

All new task-specific tests live here. Existing application and Kizurium tests
remain in their original directories. Cargo registers `rust/regression.rs` and
`rust/bench.rs` explicitly in `crates/core/Cargo.toml`.

Run `bash tests/inplace/run.sh` from the repository. It writes all builds,
bytecode, caches, logs, images and measurements under `tmp/`. Dependencies:
Rust dependencies already cached for `--offline`, Qt 6 development tools,
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
checks exact metadata/placement equality, and writes raw samples and summary into
`tmp/phase23/paired-benchmark/`. Both binaries use ApproxMeasure for equivalent
placement conditions. Image candidate correctness still needs labeled data.


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
`docs/screenshots/test-game/translations/`. These are deterministic test
translations, not translation-service results or approved visual references.
All intermediate artifacts still stay in `tmp/`. Dataset discovery excludes
this output subdirectory.


The recorded engine.exe case also requires both observed dialogue readings to
be rendered over at least 90% of their OCR boxes, without beside-original labels.
New Rust cases cover word-fragment reconstruction, prevention of neighbouring
OCR-padding overlap and exact cached geometry across an identical next frame.
CrossxMars.exe still fails the placement quality diagnostic; passing its
structural renderer checks does not close that known defect.

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
