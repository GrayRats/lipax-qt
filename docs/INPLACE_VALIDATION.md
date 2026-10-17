# Inplace implementation and verification report

Continuation: [phase 2–3 verification and changes](INPLACE_PHASE_2_3.md).
The results below describe the initial implementation; the continuation records
the later full OCR run and integration of shared Qt measurement into the harness.

Date: 2026-10-11. These are incremental changes to the existing engine, not a
replacement of capture, translation services, OCR backends, or other display modes.
New tests and their runner are in `tests/inplace/`. Generated data is in ignored
`tmp/`. Pre-existing edits in `fit.rs`, QML, placement, `app_icon.h`, and
`docs/ARCHITECTURE.md` were preserved; the pre-existing architecture document and
fitting edits were not rewritten by this task.

## 1. Architecture analysis

Kizurium's real implementation is Python: `ocr/preprocess.py` estimates tiny ink,
upscales and maps boxes back; `ocr/engine.py` normalizes OCR lines and stitches
baselines; `ocr/frame.py`, routing and passes orchestrate recognition.
`layout/grouping.py` and dialogue/name-gap helpers assemble regions.
`typography/metrics.py` measures image ink, strokes, spacing, slant and colors,
with image/box measurement caches. `render/cards.py` builds/fits translation
cards with Pango; `render/overlay.py` renders through GTK/Cairo; `card_cache.py`
is a bounded raster-card cache keyed by text, font, style, geometry and theme.
`live/state.py` has versioned frames and a latest-frame queue, while tracking
and reconciliation preserve block state. Capture/compositor modules handle
Wayland surfaces. There is no dependable exact game-font identification here.

Inspected reference tests include capture geometry, display scaling, glyph
metrics, measurement caching, type measurement and OCR upscaling. The selected
five suites ran from the reference project's directory: **42 passed**. Their
useful principles are directional glyph tests, unchanged-frame caching, explicit
scale conversions, empty-input safety and exercising real image paths.
The new tests implement those scenarios against LipaX APIs independently.
Reference tests were not changed. An initial run from LipaX's cwd had one
source-path failure; running from the reference cwd resolved it.

Kizurium declares AGPL-3.0-or-later; LipaX declares MIT. No reference implementation
or test bodies were copied, and no new application dependency or font was added.
Existing fonts imported earlier have separate OFL licensing/provenance in
`packaging/kizurium-fonts.json`, `fonts.sources` and the font license files.
Existing font checksum/coverage tests passed.

LipaX already separates Rust detector/tracker/classifier/matcher, typography,
background reconstruction, fitting and collision-aware placement. Typed frame,
crop and desktop rectangles and per-axis frame-to-logical transforms already
work, as do cancelled/stale OCR guards and placement-cache invalidation tests.
Its Qt bridge measures text on the GUI thread; QML renders resolved entries and
retains the primary text item. Moving the application to Python/Pango or adding
HarfBuzz separately would duplicate Qt shaping and break architecture/licensing.

Actual defects addressed: HashMap order changed greedy line grouping on identical
images; connected scenery was accepted as gigantic glyphs; appearance stayed
locked forever; boundingRect-height division guessed line counts; measurement
and mixed-script rendering lacked matching context fallback; decorations could
escape the field; custom-region expansion used whole-window bounds.

## 2. Source changes

Modified:

- `.gitignore`: exclude `/tmp/`.
- `crates/core/Cargo.toml`: register regression tests and benchmark example from
  the separate root test directory; no dependency changes.
- `layout/block_detector.rs`: canonical connected-component ordering and a
  scenery-height guard, with a 64 px floor for short OCR crops.
- `layout/engine.rs`, `tracker.rs`: periodically probe appearance and require
  two confident compatible observations before resetting font/style caches.
- `layout/font_matcher.rs`: selection confidence cannot exceed measured evidence
  through an artificial minimum confidence floor.
- `layout/units.rs`: disjoint empty intersections remain at the nearest boundary;
  zero-sized images yield an empty integer crop instead of panicking.
- `layout/place.rs`: intersect screen/window placement bounds with capture bounds
  before fitting and collision resolution.
- `crates/app/src/app_icon.h`: use the shaped-line helper and context font merging.
- `InplaceTranslation.qml`: use the validated coordinate helper and suppress
  content with invalid geometry.
- `InplaceTranslationContent.qml`: clip the whole decorated field, enable
  context font merging/kerning, create eight outline items only when needed,
  and avoid shaping invisible shadow text.
- `crates/app/build.rs`: package the JavaScript resource and track the C++ helper.

Added: `crates/app/src/text_measure.h`, `qml/InplaceGeometry.js`,
`inplace_helpers.qrc`, `tests/inplace/{README.md,run.sh,rust/regression.rs,
rust/bench.rs,qml/tst_inplacegeometry.qml,qt/text_measure.cpp,
python/test_inplace_screenshots.py}`, and this report. No application file removed.

## 3. Glyph recognition and typography

The existing pixel classifier measures cap/x-height, stroke width/contrast,
serif feet, pitch regularity, width proportions and slant. It still estimates
characteristics, not font identity. Components now have deterministic ordering,
and huge connected scenery no longer consumes the limited candidate budget.
A component above `max(64 px, 15% of frame height)` is rejected; this is a
segmentation guard, not an OCR accuracy guarantee or font metric.

Per-block appearance is probed at most every 500 ms. Confidence must be at least
0.6, and two observations must agree on category, weight and italic state with
compatible proportions. Confirmed changes invalidate family/typography and
increment the block revision; weak or isolated changes retain the style and id.
Matching still uses bundled category/coverage/weight/proportion preferences;
its family-width estimates remain approximate, and confidence is not calibrated
against labeled game fonts.

Qt QTextLayout now counts actual shaped lines, preserves explicit/empty breaks,
and uses word-boundary wrapping with the same anywhere fallback as QML Text.Wrap.
Natural advances include kerning, shaping and ligatures. Line spacing conservatively
includes fallback glyph metrics. C++ measurement and QML both enable context font
merging. Existing bundled sans, serif, monospace, display and Noto CJK families
remain the only selectable families; duplicate new font packages were unnecessary.
Tests verify actual loaded Noto Latin/Cyrillic and CJK Japanese/Chinese/Korean glyphs.

Fallback policy: the selected user-facing family is bundled. Qt may use a loaded
or system fallback for missing characters, with context merging across a run.
Unsupported scripts therefore depend on installed fallback coverage and cannot be
guaranteed portable; a family with missing declared language coverage is rejected
by the existing matcher. CJK serif requests still use the bundled CJK sans
substitute. This task adds neither an exact font recognizer nor a full Unicode
cmap coverage engine.

## 4. Overlay behavior

Existing typed crop->frame and frame->desktop transforms remain authoritative.
Coordinates are Qt logical desktop pixels after conversion; DPR is not multiplied
into them again. Each axis keeps its own scale. Rust fits within the intersection
of capture, game client area and the selected monitor; collision and overflow
fallbacks remain in the existing engine. JavaScript rejects malformed/nonfinite
geometry and intersects field fractions with capture/window bounds. Edge rounding
continues to use the pre-existing rounding policy.

The whole content item clips background, primary text, shadows and outlines.
Primary text items survive updates; unused outline delegates are absent, with
stable delegates while a wide outline remains enabled. Periodic font reanalysis
changes typography only after confirmation. Existing tests exercise cancellation,
changing geometry, visibility, wrapping, elision, cache invalidation and other
translation modes. New tests cover negative monitor coordinates, fractional
geometry, capture-edge expansion, invalid JS input and appearance hysteresis.

## 5. Executed verification

Rust: **373 passed, 0 failed, 15 ignored** across unit/integration/doc suites,
including **7 new regression tests**. The initial sandbox run had 10 failures
because HTTP mocks could not bind loopback; the final run with loopback access
passed. A too-aggressive first version of the scenery guard caused classifier
and crop-pipeline failures; the crop floor fixed them and the full suite was rerun.

Application `cargo build -p lipa --offline` and `cargo check` both succeeded.

Qt Quick: existing suites **106 passed**, new QML/JavaScript suite **13 passed**,
zero failures/skips. JavaScript tests execute the production helper inside Qt
Quick Test; there is no separate Node framework. C++ production measurement
helper executable completed with exit 0, exercising hard breaks, empty text,
long-word wrapping, combining characters and mixed scripts.

Python: screenshot pipeline/visual comparator suite passed for the original
13 images (**14 tests**), both with real Tesseract and without OCR. After the
dataset added three differently sized screenshots and a direct multilingual
coverage test, the final full suite passed **18 tests, 0 failures** (16 screenshot
cases plus comparator and glyph-coverage tests). Two warnings concern Pillow getdata
deprecation in the comparator. The translucent border on `dialogue-en-14.png`
exposed an invalid RGB comparison that discarded alpha; ingestion now explicitly
composites source PNGs onto black to represent opaque captured frames. The full
final suite ran with OCR disabled; additional real-OCR cases were run separately.
Russian Tesseract was separately run with `rus`, rather than treating English
recognition on the Russian image as evidence of Russian OCR accuracy. The added `dialogue-en-13` OCR case passed separately;
`-14`/`-15` recognition executed, but their initial combined test had one alpha-
comparison failure before normalization was fixed. A complete OCR-enabled run
against the final 16-image snapshot was NOT RUN.

The screenshot path uses real detector/classifier/matcher, deterministic
translations, Rust placement and the actual QML content component. OCR is tested
independently through LipaX's Tesseract TSV normalizer. The offline Rust placement
harness explicitly uses ApproxMeasure; production Qt metrics are exercised by the
separate C++/QML tests. This is not a live capture/service end-to-end test.

Diagnostics include original/detected/rendered images, actual OCR lines,
typography metadata and outside-field pixel differences under `tmp/visual/`.
Pixels outside allowed plates must remain unchanged. A negative comparator test
rejects an intentionally damaged image. **Approved-reference visual comparison:
NOT RUN** (no independently approved references). **OCR accuracy scoring: NOT
RUN** (no annotated ground truth). No generated coordinates are called ground truth.

## 6. Measurements

AMD Ryzen 5 5600, 6 cores/12 threads; Rust 1.99.0, Qt/PySide 6.12.0,
Tesseract 5.5.3, Python 3.14.7. CPU governor/boost was not pinned.
Rust comparisons use release builds and the same original 13 PNG inputs, five
samples per image for detector/classifier/matcher. Baseline source snapshots of
initially unchanged layout modules are under `tmp/baseline-source`; existing
uncommitted fitting/placement edits were retained. The later added image is not
included in this comparison. Timing samples are in `tmp/*release-bench.jsonl`.

| Median measurement | Baseline | Final |
|---|---:|---:|
| Mask + detection | 6.542 ms | 6.539 ms |
| Glyph analysis, all candidates | 35.423 ms | 18.898 ms |
| Font matching, all candidates | 0.066 ms | 0.066 ms |
| First engine frame | 76.823 ms | 52.867 ms |
| Identical next frame (100 ms later) | 10.532 ms | 9.387 ms |
| Diagnostic placement, ApproxMeasure | 0.390 ms | 0.508 ms |
| Cached diagnostic placement | 0.063 ms | 0.067 ms |
| Median Rust process peak memory | 36.21 MiB | 36.13 MiB |
| Maximum Rust process peak memory | 54.48 MiB | 42.11 MiB |

Candidate sets change after scenery rejection; reduced glyph work is not proof
of better recognition. Placed diagnostic fields increased 77->104; dropped fields
fell 118->97, without asserting these are all text. Second-placement cache hits
were 195/195 baseline and 201/201 final. Final identical frames require no OCR jobs.

The Qt helper microbenchmark (10,000 calls, C++ -O2, same mixed-script text/font)
measured boundingRect at 0.01438 ms/call and shaped-line measurement at 0.01512
ms/call. There is a small cost for more accurate measurement, no claimed speedup.
A full-frame English Tesseract diagnostic median was 9733 ms across the original
13 images, with OMP_THREAD_LIMIT=1; no comparable pre-change OCR measurement
exists and no OCR latency improvement is claimed. Interactive OCR uses cropped
jobs, unlike this full-frame stress sample. Median synchronous offscreen overlay
update/paint was 2.219 ms; no pre-change compositor/update measurement exists.
Rust peak memory excludes OCR subprocesses and the Qt application. Render timing
includes a deliberate settling wait and is not GUI latency. These are reproducible
diagnostics, not representative live-game FPS or a complete application memory profile.

## 7. Remaining limitations

- Live KWin/Wayland/XWayland capture, physical multi-monitor transitions,
  fractional-DPR compositor screenshots and real translation services: NOT RUN,
  except the single-machine dialogue-change check in section 8.
  Geometry/lifecycle/cancellation are covered by automated synthetic/API tests.
- Exact baselines, word-level font identities, rotation and difficult low-contrast
  text are not solved. Global Otsu segmentation can still detect scenery or miss
  text; the block budget remains (default 64, `DEFAULT_MAX_BLOCKS` in `block_detector.rs`, settable as
  `appearance.inplace.max_blocks` from 4 to 256). Giant legitimate decorative glyphs above
  the scenery limit can be rejected. No labeled OCR/font-quality scores exist.
- Font width/category matching remains heuristic; unsupported scripts may require
  installed Qt fallback fonts. Bundled CJK serif is absent.
- Screenshot rendering uses diagnostic approximate fitting, not the full live
  Qt bridge. Approved visual references and live service E2E remain outstanding.
- No baseline OCR/GUI/compositor or full-application memory comparison was collected.
- 15 existing opt-in Rust tests remain ignored; they are not counted as passed.

These changes are working, tested improvements. They do not establish every
item of the original broad definition of done as complete.

All new test sources remain in `tests/inplace/`; the runner uses only `tmp/` for
artifacts. `git diff --check` passed and `tmp/` is ignored. No new font
extraction occurred in the source tree during the task.

## 8. Dialogue change on CrossxMars.exe

Covers the changes in `layout/engine.rs` (stale translation under changed ink,
withdrawal of a field missing from the scan, ownership of repeated and overlapping
readings), `layout/ocr_geometry.rs` (shared-row join), `layout/background.rs`
(exemplar search limited by field area) and `translate/guard.rs` (lost-tail rule).

Automated:

- `tests/inplace/rust/dialogue_change.rs` (`inplace_dialogue_change`) replays four
  live-recorded frames (`CrossxMars.exe-live-{mika,d1,d2,d3}.png` with the
  `-local-rapidocr.json` readings recorded by `crates/core/examples/record_dialogues.rs`
  using real RapidOCR). It checks that the name and the dialogue are replaced, that a
  new dialogue replaces the old one in the same engine, that no fields overlap, that
  all fields stay inside the region, and that the old translation is withdrawn when the
  new text appears. `LIPAX_TEST_TRACE=1` prints per-scan traces.
- `tests/inplace/rust/ocr_ownership.rs`:
  `a_dialogue_line_cut_in_two_by_the_detector_is_joined_and_not_left_as_overlapping_parts`
  covers the shared-row join.
- Exemplar search (`copy_similar`), offline on saved frames of live dialogue crops
  (release, idle machine): 2.7 s → ≈1.0 s per frame.

Live check (manual, not automated): KWin Wayland, Wine, CrossxMars.exe. Space was
pressed through a uinput keyboard, only while the game window was active. The first
overlay update came 0.26 s after the key in two of three transitions and 2.1 s in the
third. The previous translation no longer stayed over the next dialogue. Control
screenshots: `docs/screenshots/test-game/translations/crossxmars-live-*.png`.

This is one machine and one game. No other game, window manager, or GPU was tested,
and no latency distribution is claimed.
