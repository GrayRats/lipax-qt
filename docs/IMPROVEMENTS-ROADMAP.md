# LipaX — Improvement Roadmap (post-1.4.5)

Written 2026-11-21 against LipaX **1.4.5** (HEAD `e313e3b`); revised the same day after re-checking the claims that were
first marked unverified (see "Revision log" at the end). It complements, and does not replace,
[IMPROVEMENTS.md](IMPROVEMENTS.md) (what is already implemented), [AUDIT-1.4.4.MD](AUDIT-1.4.4.MD) and the
older `PLAN.md` (P-01…P-14, written at 1.3.9 from a Kizurium analysis). Items already tracked there are
only cross-referenced, not repeated.

**Scope: LipaX is, and stays, a Linux-only, Wayland-only project. KDE Plasma 6 / KWin is the primary and best-supported
target; other Wayland compositors of 2026 (Sway and other wlroots-based ones, Hyprland, niri, COSMIC, GNOME/Mutter) are
candidate secondary targets.** No proposal here assumes or leads to a Windows or macOS build, or to X11-only support.
Translumo is used as a source of *ideas and tuned parameters*, not of code; every Windows-specific mechanism in it
(WinRT OCR, DXGI, `RegisterHotKey`, XInput) has a Linux-native counterpart in the items below, or is intentionally
skipped (TTS is not wanted at all). "Integration" means deeper use of KWin/Plasma (R-10); "reach" means additional
Wayland compositors (R-14).

Inputs: a read-through of the LipaX code and docs, and the architecture study of Translumo
(`project/Translumo/ARCHITECTURE.md`, graph in `project/Translumo/graphify-out/`).

**Reading guide.** Effort: S ≤ 1 day, M 2–4 days, L ≥ 1 week (estimates, not measurements).
Evidence tags: **[code]** checked in this repo, **[translumo]** observed in Translumo,
**[hypothesis]** reasoning that has not been measured or run here — verify before committing to it.
**[code]** items were re-checked against the working tree on 2026-11-21 unless noted.

## 0. Where LipaX stands relative to Translumo

| Area | Translumo | LipaX 1.4.5 | Verdict |
|---|---|---|---|
| Translated text placement | chat-style log window only | in-place overlay with font matching, background reconstruction, tracking; separate window | LipaX far ahead |
| OCR | Windows OCR + EasyOCR (Python, NVIDIA) | Tesseract, RapidOCR (in-process `ort`), MeikiOCR, PaddleOCR worker | LipaX ahead (no Python for the main engines) |
| OCR result selection | ML validity score + cross-engine vote on the *same* frame | sequential escalation on one engine's own confidence (`ocr/mod.rs`, `is_better`) | **Translumo ahead** |
| Translators | Google, DeepL, Yandex | Bergamot (offline), Google, Yandex, DeepL, Microsoft, custom | LipaX ahead |
| Rolling translation log | chat window (50 items) | history in a separate window, latest line on screen | partial gap |
| Hotkeys | `RegisterHotKey` + XInput gamepad | KGlobalAccel, 5 actions; separate CLI/desktop actions (`--show`, `--capture`, `--start-autotranslate`, …) | different platform; no gamepad; hotkey actions not reachable from the CLI |
| UI languages | en, ru, zh-CN | Russian only (no `qsTr` in `crates/app/qml`) | **gap** |
| Platform | Windows (out of scope for LipaX) | Linux, Wayland, KWin only today | gap: other Wayland compositors (R-14); opportunity: deeper KWin integration (R-10) |
| Tests / CI | none, build-only CI | 580 `#[test]`/`#[tokio::test]` attributes (counted), structural CI job | LipaX ahead |
| Anki / dictionary | AnkiConnect popup (unfinished) | none | optional gap |

## 1. Priorities at a glance

| ID | Proposal | Value | Effort | Evidence |
|---|---|---|---|---|
| R-01 | Frame-level OCR consensus and text-validity scoring | **Low (measured: no gain on the test set)**; see R-01b | M–L | [translumo] [code] + measured |
| R-02 | GPU OCR on AMD: show the active provider, document, benchmark | High (for the maintainer's hardware) | S–M | [code] + [hypothesis] |
| R-03 | UI internationalization (`qsTr`, English first) | High | M | [code] — **QML done 2026-11-21**, Rust-side messages open |
| R-04 | Rolling "chat log" mode in the translation window | Medium–High | S–M | [translumo] — **implemented 2026-11-21** |
| R-05 | Persistent Bergamot process (no model reload per call) | Medium (measured ≈0.17 s per new line) | M–L | [code] + measured — **implemented 2026-11-21** |
| R-06 | Bergamot pivot through English + source-language detection | Medium | M | [code] — **implemented 2026-11-21** (unit-tested only) |
| R-07 | KWin restart resilience (`NameOwnerChanged`) | Medium | S | [code] AUDIT — **implemented 2026-11-21** (not live-tested) |
| R-08 | Secrets: API keys out of the plain `config.toml` | Low–Medium | S–M | [code] — **implemented 2026-11-21** (opt-in wallet) |
| R-09 | CI: clippy/fmt gates, package build, release job | Medium | S–M | [code] — **implemented 2026-11-21** except `fmt` |
| R-10 | Deeper KWin/Plasma integration: nested-KWin test rig, version probing, focus-aware pause, CLI for all hotkey actions | High (robustness) | M | [code] + [hypothesis] |
| R-11 | Word-level lookup / Anki export from the overlay | Low–Medium | M | [translumo] |
| R-12 | Gamepad hotkeys for Steam Deck / couch setups | Low | M | [translumo] |
| R-13 | Close the open live-verification debt | High (trust) | M | [code] PLAN/AUDIT |
| R-14 | Other Wayland compositors behind a backend trait (tiered) | Strategic | L | [code] + [hypothesis] |

Recommended order: **R-13 → R-03 → R-01 → R-02 → R-05 → R-07/R-08/R-09/R-10 → R-04 → R-14 (trait refactor first) → the rest.**
Rationale: correctness and latency of what exists first, then audience (English UI), then new capability.

## 2. Proposals

### R-01 Frame-level OCR consensus and a text-validity score — re-rated to Low after measurement

**Measured 2026-11-21** with the new `crates/core/examples/engine_disagreement.rs` (Tesseract and RapidOCR on the same
full frame, 16 recorded English dialogue frames of one game, 120 hand-transcribed texts from
`tests/inplace/fixtures/ground-truth.json`, CER against the closest substring, RapidOCR on CPU with 4 threads):

| Set | Texts | CER Tesseract | CER RapidOCR | CER of a per-text ORACLE (best of two) | Texts where they differ by > 0.10 | ms/frame Tesseract / RapidOCR |
|---|---|---|---|---|---|---|
| working, frames 0–9 | 60 | 0.295 | 0.008 | 0.007 | 32 (53 %), **RapidOCR better in all 32** | 803 / 673 |
| control, frames 10–15 | 60 | 0.277 | 0.014 | 0.012 | 31 (52 %), **RapidOCR better in all 31** | 904 / 706 |

Even a perfect selector between the two engines would improve the mean CER by ≈0.001 (0.008 → 0.007; 0.014 → 0.012), and
Tesseract was never the better reading by more than 0.10. So a validity scorer or a consensus mode **cannot pay off for
these engines on this material**; the proposal below is kept only as the record of what was considered. Limits of the
measurement: English only, one game, dialogue/UI frames, no Japanese/Chinese/Korean (MeikiOCR vs RapidOCR is untested), no
noisy-background or HUD frames, and the transcription is hand-made. Re-measure before building anything for other languages.

**R-01b — the useful consequence (new, small).** With a RapidOCR model installed, `Auto` still reads with Tesseract first and
only escalates when Tesseract's own confidence is low (`ocr/mod.rs`, `AUTO_ACCEPT`). On these frames Tesseract is both much
less accurate (CER ≈0.28–0.30 vs ≈0.01) and not faster (≈0.8–0.9 s vs ≈0.7 s per full frame). Whether Tesseract's confidence is
low *often enough* to trigger the escalation on such frames was **not** measured here (the example does not run `Auto`).
Worth doing: (1) run the real `Auto` on the same frames and count how often it ends on Tesseract's wrong text;
(2) if it does, prefer RapidOCR first whenever its model and runtime are present (Tesseract stays the zero-download default
and the fallback). This is a behaviour change to the default engine path and needs the maintainer's decision.

**Original proposal (not recommended any more):**

**Problem.** `Auto` runs Tesseract first and escalates (Meiki → Rapid → Paddle) while the confidence is below
`AUTO_ACCEPT`; the first "more sure" engine wins [code: `crates/core/src/ocr/mod.rs`]. Each engine's confidence is
only comparable to itself, and engines never see the same frame side by side. Translumo's best idea is the opposite:
score every engine's text with a *language-aware validity model* and vote when several agree
[translumo: `TextDetectionResult.CompareTo`, `GetBestDetectionResult`: ≥3 agreeing engines win, score threshold 2.1].

**Proposal.**
1. A small **dependency-free validity scorer** per language instead of porting ML.NET: character-class ratio
   (script match), dictionary/word-list hit rate, n-gram plausibility, penalties for isolated symbols and
   mixed-script garbage. Output a float used only to *rank* candidates.
2. Optional "compare mode" for fixed regions (HUD, dialogue): run two engines on the same frame, choose by score;
   if both agree after normalization, accept immediately (shortcut that also saves time).
3. Keep today's sequential escalation as the default; the scorer replaces the raw-confidence comparison inside
   `is_better`, and the consensus mode is opt-in per region.

**Why not copy Translumo exactly.** Its ML.NET models and training data are not in the repo and the vote
cannot fire with two engines [translumo]. A transparent heuristic scorer is testable with the existing
`ocr_accuracy` CER/WER bench and recorded frames in `tests/inplace/fixtures`.

**Verification.** Extend `crates/core/examples/ocr_accuracy.rs` with the scorer; require CER no worse than the current
`Auto` on the 16-frame set and report latency per engine path. Interacts with `PLAN.md` P-11 (short-token voting,
marked done): reuse that code rather than adding a second voting mechanism.

### R-02 GPU OCR on AMD — High for the maintainer, M

**Facts [code].** `crates/core/Cargo.toml` enables `ort` features `cuda`, `rocm`, `migraphx`; `ocr/rapid.rs`
(`gpu_providers`, ~l.104–111) probes CUDA, MIGraphX and ROCm. A user setting `recognition.rapid_use_gpu` exists with a
checkbox ("Использовать GPU, если доступен") and a help text in `SettingsWindow.qml` (~l.1718–1729). If no provider is
available the app falls back to CPU and only writes an `info` line to the log (`rapid.rs` ~l.488). This machine has
`onnxruntime-cpu 1.29.0-3` installed [`pacman -Q`], which has no GPU providers, so here OCR is CPU even with a
Radeon RX 6700 XT. I found no place that shows the *actually used* provider in the Status tab or the OCR inspector,
and no GPU-vs-CPU benchmark in the repo; documented CPU numbers are ~300 ms for a 1920×1080 frame, ~55 ms per line
(`docs/RapidOCR.md`).

**Proposal.**
1. Surface the active execution provider in the OCR inspector and the Status tab ("RapidOCR: CPU / MIGraphX / ROCm"),
   and show a visible hint next to the checkbox when the checkbox is on but the runtime has no GPU provider. The setting
   already exists; the missing part is feedback, since a ticked box that silently does nothing is the real defect.
2. Document the AMD path in `docs/RapidOCR.md` and in `optdepends` (a ROCm/MIGraphX ONNX Runtime build).
3. Benchmark CPU vs GPU with `ocr_accuracy` on the recorded frames; record the numbers.
4. **[hypothesis]** gfx1031 (RDNA 2) is not an officially supported ROCm target; it likely needs an override such as
   `HSA_OVERRIDE_GFX_VERSION=10.3.0`. Do not document this until it has been run on the actual card.
   If ROCm proves fragile, evaluate an ONNX Runtime WebGPU/Vulkan build as an alternative.

**Decision rule.** Ship the visibility and docs regardless; only advertise GPU speed-ups once measured.

### R-03 UI internationalization — High, M

**Status 2026-11-21 — QML part implemented (uncommitted).** All 805 Cyrillic literals in `crates/app/qml/*.qml` are wrapped in
`qsTr()` (a mechanical, scripted change; the 724 unique strings were more than the ~230 first estimated); the 735 messages
are in `crates/app/i18n/lipax_en.ts` (Russian source, English translation, all finished); `build.rs` compiles them with
`lrelease` into the executable; `ui_language.rs` picks the language from `LIPAX_UI_LANGUAGE`, `LANGUAGE`, `LC_MESSAGES`,
`LANG`, `LC_ALL` (a `C` locale is ignored, nothing usable → Russian as before); `qt6-tools` was added to `makedepends` and
CI. Checked: 128 QML tests pass unchanged, 4 unit tests for the language choice, and a real run with
`LIPAX_UI_LANGUAGE=en` shows the English settings window (offscreen snapshot). **Languages (added the same day):** English, French, Simplified Chinese, Japanese and Spanish catalogues (`crates/app/i18n/lipax_{en,fr,zh_CN,ja,es}.ts`, 735 messages each, all finished); French, Chinese, Japanese and Spanish were machine-translated by Claude Haiku from the English text and have had no native-speaker review; each was checked by compiling with `lrelease` and an offscreen snapshot (CJK glyphs render). `LIPAX_UI_LANGUAGE`/locale `fr`, `zh*` (Traditional users get the Simplified catalogue), `ja`, `es` select them; any other non-Russian locale gets English.
**Language setting added:** `general.language` (`auto`, `ru`, `en`, `fr`, `zh_CN`, `ja`, `es`) with a selector in «Приложение» (languages named in themselves); `LIPAX_UI_LANGUAGE` beats the setting, which beats the locale; applied after a restart. **Still open:** (1) messages built in Rust
(statuses, errors, diagnostics rows, tray menu, notifications) are still Russian; (2) ~~a persistent language setting~~ (done); (3) the CI guard against new bare Cyrillic literals; (4) concatenated strings (e.g. the Rapid model
prompt) translate as fragments, which is fine for English but fixed word order may not suit other languages; (5) English text
was written by me and has not had a native review.

**Original facts [code].** `crates/app/qml` had no `qsTr`; `SettingsWindow.qml` (4272 lines) holds ~560 Russian string
literals. The README is bilingual but the app and most docs are not, which blocks non-Russian users and Arch/AUR
reviewers.

**Proposal.** Wrap strings in `qsTr()` / `QT_TR_NOOP`, add Qt Linguist `.ts` files (ru as source, en first, then
optional zh/uk/de), load `QTranslator` from the existing language setting, ship `.qm` via the resource system
(see the `qt-cmake-project` / `qt-qml` skills for the standard pattern). Do it in several mechanical commits per QML file
so the 4272-line file never needs a flag day. Add a QML test that fails on new bare Cyrillic literals.

### R-04 Rolling chat-log mode — Medium–High, S–M

**Status 2026-11-21 — implemented (uncommitted).** New settings `appearance.window.log_mode` (default off) and
`appearance.window.log_entries` (2–10, default 5); a switch and a spin box in «Оформление перевода → Журнал переводов»;
`TranslationWindowContent.qml` keeps the previous translations in a session log (`logPrevious`) and draws them above the
current one at 0.8× size, fading from 0.6 to 0.25 opacity with age; the view scrolls to the newest entry; the same text twice is
not logged twice; turning the mode off clears the log. The log lives in the window (not in `history.rs`), so it works with the
history disabled. Verified: a new QML test (129 tests pass), the settings tests (37), and a rendered image of five successive
translations. Translations of the new strings exist in all five catalogues. **Not done from the first sketch:** copy-on-click
(a click already means “drag” or “pin” in this window) and showing the original per log entry; the log is per window, not per
capture region. **Original proposal:**

Translumo's main UI is a scrolling log of the last 50 translations [translumo: `ChatWindowModel`]. LipaX shows only the
latest translation in the window and keeps history elsewhere [code]. Add a window style "log": last N entries,
fading older lines, copy-on-click, optional original line in a smaller font. Reuses `history.rs`; no pipeline change.
Particularly useful for fast dialogue where the user misses a line.

### R-05 Persistent Bergamot process — Medium (re-rated after measurement), M–L

**Status 2026-11-21 — implemented (uncommitted).** `packaging/bergamot-server.cpp` is a small front end over the
bergamot-translator library API: it loads the model, warms it up, prints `LIPAX-BERGAMOT-SERVER 1`, then answers
length-prefixed requests (4-byte big-endian length + UTF-8) until stdin closes. `build-bergamot.sh` copies it into the
checkout, adds the CMake target and builds it next to the CLI; the root `PKGBUILD` installs `/usr/lib/lipax/bergamot-server`
and `test-package.sh` checks it. On the Rust side `translate/bergamot/worker.rs` keeps one worker per model config:
started lazily with a 30 s handshake timeout, reused, stopped after 3 min idle, restarted when the model config or its
directory changes, retried once with a fresh process if a reused one died, and **any** failure to start or talk to it falls
back to the old one-shot CLI (60 s cooldown before the next attempt; `LIPAX_BERGAMOT_PERSISTENT=0` turns it off). Requests of one
model are serialized. **Measured** with `crates/core/examples/bergamot_latency.rs` (en→ru, 2 threads, 30 different
lines, one call each): median **180 ms → 11 ms** per line (max 200 → 27 ms), first call ≈320 ms. Verified: 9 new unit tests
with a fake server (reuse, died while idle, silent/mute server, timeout, idle stop, replaced model, server discovery), the
real server driven over the protocol, and the example above. **Not done / caveats:** the **AUR** `PKGBUILD` is untouched
because it builds the v1.4.5 tag, which has no `bergamot-server.cpp` — add the `install` line there at the next tag bump;
the app was not live-tested in a game; memory of an idle model stays resident for up to 3 minutes; the persistent mode is not
in the Settings UI (environment variable only); cold-cache numbers and other language pairs were not measured.

**Original analysis:**

`translate_many` batches several texts into one call, but a process is spawned per call
[code: `translate/bergamot.rs`, `run()`]. **Measured on 2026-11-21** (installed engine `/usr/lib/lipax/bergamot`,
model `en-ru`, `--cpu-threads 2`, warm page cache, wall time of the whole process including startup and model load):

| Input | Wall time |
|---|---|
| 1 line, run 1 / 2 / 3 | 0.196 s / 0.174 s / 0.183 s |
| 5 lines, one process | 0.178 s |
| 20 lines, one process | 0.217 s |

So the fixed cost (process start + model load) is ≈0.17 s and the translation of one short line is a few milliseconds.
A persistent worker would save ≈0.17 s for every *uncached* line, i.e. roughly the cost of one RapidOCR line; it would
not change the cost of cached or repeated lines (the translation cache already handles those). This is worth having for
fast dialogue but is **not** the "latency dominates" case the first version of this roadmap assumed, hence Medium.
(Cold-cache numbers, other language pairs and the `big`/`tiny` model classes were not measured.)

**Feasibility constraint [code].** The stock `bergamot` CLI reads stdin until EOF (see the comment in `exchange()`),
so a long-lived process cannot be driven line by line with the current binary. A persistent mode needs a small
purpose-built C++ front end over the bergamot-translator library API (read a line → write a line, flush), built by
`packaging/build-bergamot.sh` next to the CLI, with a version/handshake and a fallback to today's spawn-per-call path.
That touches the PKGBUILD and the pinned-engine build, which is why this is M–L and should come after R-13.

**If done.** One worker per language pair, idle shutdown, restart on crash, per-request timeout, and a before/after
comparison in the existing stage timings (`Timings`). Closes the remaining part of `PLAN.md` P-04 (batching is done).
**Cheaper alternatives to evaluate first:** (1) raise `--cpu-threads` only if multi-line batches show a gain
(unmeasured); (2) translate the next likely line speculatively is *not* proposed (privacy, complexity).

### R-06 Bergamot pivoting and source-language detection — Medium, M

README: "no automatic pivoting through another language"; `auto` source is unsupported [code]. Proposal: when a direct
pair is missing but `xx→en` and `en→yy` models are installed, chain them (cache both hops, flag lower confidence in
the UI); add cheap language ID (script-based for ru/ja/ko/zh, a small trigram detector for Latin languages) to choose
the source when the region says "auto". Together these multiply usable language pairs without new models.

**Implemented 2026-11-21 (Bergamot only, unit-tested, not run against real models).**
`translate/bergamot.rs`: `choose_route`/`route` (direct model wins; otherwise `xx→en` + `en→yy` when both are installed),
`translate_via_english` and `translate_many_via_english` (a batch is two process/worker calls), `resolve_source`;
`translate/langid.rs`: script-based detection (kana → ja, han → zh, hangul → ko, Cyrillic → ru/uk); wired into
`translate/mod.rs`. **Not done:** a trigram detector for Latin text (Latin gives a clear "select the language" error);
caching of the intermediate English text; a "lower confidence" marker in the UI (the route is only logged); the settings UI
has no "auto" source choice yet, so detection is reachable through `translation.source_language` in the config only.
Only `en-ru` is installed on this machine, so a real two-hop run (e.g. `de→ru`) needs the `de-en` model downloaded first.

### R-07 KWin restart resilience — Medium, S

AUDIT-1.4.4 finding: `OnceCell<ClientGeometry>` in `capture/kwin.rs:19` keeps the window table and script stale until LipaX
restarts. Subscribe to `org.freedesktop.DBus.NameOwnerChanged` for KWin, drop and re-create the geometry client,
re-install the script, and show a status message. Add a test with a fake bus name owner change.

**Implemented 2026-11-21.** `capture/kwin.rs`: the `OnceCell` is replaced by `Slot<T>` (client + failure timer + restart
counter); `watch_kwin_restarts` follows `NameOwnerChanged` for `org.kde.KWin`: a lost name drops the client, a new owner
drops it, clears the 30 s failure throttle and bumps `KwinCapture::restarts()`. The floating-window service in
`app/src/bridge.rs` re-applies the placement and re-subscribes to moves on that counter. Unit tests cover `Slot`
(create once, drop on restart, throttle reset). **Not done:** the status message in the UI (only `tracing` lines) and a
live check — `tests/inplace/live/run_nested_kwin.sh` with `kwin_wayland --replace` is the natural rig (see R-10).

### R-08 Secrets handling — Medium, S–M

**Status 2026-11-21 — implemented (uncommitted).** (1) `Debug` of `TranslationSettings` and `Translator` no longer prints API
keys (test `debug_output_never_contains_api_keys`). (2) The `0600` mode is enforced by `files::write_atomic` (tested there).
(3) **Wallet storage, opt-in:** setting `general.keys_in_wallet` (default off; checkbox «Хранить API-ключи в кошельке» in
Settings → Translation, translated in all five catalogues). New `crates/core/src/secrets.rs` talks to the freedesktop Secret
Service directly through `zbus` (no new dependency; `plain` session over the private session bus, items with attributes
`application=lipax`, `key=<field>`). Rules: `save()` writes the four keys to the wallet and blanks them in `config.toml` only
if the wallet took **all** of them, otherwise they stay in the file; `load()` fills an empty key from the wallet, a key still
in the file wins and moves at the next save (migration); clearing a field removes the item; a locked wallet is reported as
unavailable (the unlock prompt is never started, so no dialog appears over a game); every call has a 4 s limit, a failure
pauses wallet use for 60 s, unchanged values are not rewritten; if the keys could not be read at start an empty key never deletes
what the wallet holds. Status tab shows a row «Кошелёк для API-ключей» when the option is on. Tests: 6 with an in-memory store
(round trip, fallback, migration, clearing, the unread-keys guard, option off) plus `secrets::tests::real_wallet_round_trip`
(`cargo test -p lipa-core -- --ignored real_wallet`; **run against this machine's `ksecretd`: passes, including Cyrillic
values, and leaves no item behind**). **Not verified:** GNOME Keyring / KeePassXC (same protocol, not tried); the whole app
saving keys through the checkbox in a live session; an unlock-while-running scenario. **Known limits:** `plain` transport (the value
is not encrypted on the bus, as in most simple clients); environment variables (`DEEPL_API_KEY`…) are unchanged; the
wallet call runs on the thread that saves, with the 4 s limit. **Original proposal:**

API keys (`deepl_api_key`, `yandex_api_key`, `microsoft_api_key`, `custom_api_key`) are plain `String` settings fields
[code: `settings/sections.rs`, `reaction.rs`]; no keyring usage was found. The settings file itself is already mode
`0600` (`~/.config/lipa/config.toml`, checked), so other local users cannot read it; the remaining exposure is backups,
dotfile repos and screen sharing of the file. Proposal: optionally store keys through the Secret Service / KWallet API
(`keyring` crate) with the TOML as fallback, ensure the mode stays `0600` after rewrites (add a test), and never log
keys. Priority lowered to Low–Medium after the permission check. Translumo's hard-coded AES password is the cautionary
example [translumo: `AesEncryptionService`].

### R-09 CI hardening — Medium, S–M

**Status 2026-11-21 — implemented except `fmt` (uncommitted, workflow not run on GitHub).** `checks.yml` now has: a
`cargo clippy --workspace --all-targets --locked -- -D warnings` step in `structural` (passes locally after fixing the two
warnings in `ui_language.rs`; CI uses Arch's current `rust`, so a newer clippy may add lints later); a manual `package` job
(`workflow_dispatch` input `package_build`, `makepkg` as an unprivileged user in `archlinux:base-devel`; slow and **never
run**); a `release` job on `v*` tags that, after `structural`, checks that the tag equals `pkgver` in `PKGBUILD` and the
workspace version in `Cargo.toml` and runs `gh release create --generate-notes`. No tarball is attached: the AUR recipe
downloads GitHub's own `archive/refs/tags/v<version>.tar.gz`, which exists for every tag. **`fmt`:** an advisory step (`continue-on-error`, prints the number of differing hunks) was added; a blocking gate is **not** done —
the tree is not rustfmt-formatted (2361 diffs in 120 files, no `rustfmt.toml`), so a gate would need either a one-off
reformat commit or a style decision (the code uses long lines). **Original proposal:**

`checks.yml` has one structural job (OCR off) and a manual self-hosted RapidOCR job; no clippy/fmt, no package
build, no release job [code]. Add: `cargo fmt --check`, `cargo clippy -D warnings` (start with
`--workspace --all-targets` and allow-list existing warnings), an Arch-container `makepkg` build of the root
`PKGBUILD`, and a tag-triggered release that attaches the source tarball the AUR recipe expects.
Translumo's CI (build-only, wrong .NET version) is what *not* to do [translumo].

### R-10 Deeper KWin / Plasma integration — High (robustness), M

LipaX depends on KWin-specific surfaces: ScreenShot2 over D-Bus (`capture/kwin.rs`, with the
`X-KDE-DBUS-Restricted-Interfaces` desktop entry), an embedded KWin script for geometry and floating-window placement
(`capture/geometry.js`), `org.kde.kwin.Scripting`, KGlobalAccel (`hotkeys.rs`), layer-shell overlays and KWindowEffects blur
[code, ARCHITECTURE.md]. KWin stays the reference target, so the risk here is **drift and unverifiability** on a rolling distribution. Proposals:

1. **Nested KWin test rig** (`kwin_wayland --virtual` / `--x11`-less headless) so geometry script, ScreenShot2 and overlay
   placement can run in CI and locally without the user's real session. This is the unfinished core of `PLAN.md` P-01 and
   unblocks R-13. **Probed 2026-11-21 (feasibility confirmed):** the installed KWin 6.8.3 started with
   `kwin_wayland --virtual --socket lipax-nest --width 1920 --height 1080 --scale 1.5 --output-count 2 --no-lockscreen
   --no-global-shortcuts --no-kactivities` under `dbus-run-session` (private bus, separate `XDG_RUNTIME_DIR`; the user's
   real KWin was untouched). Inside it: `org.kde.KWin.ScreenShot2` interface **version 5** with `CaptureWindow`,
   `CaptureArea`, `CaptureActiveWindow`, `CaptureScreen`, `CaptureWorkspace`, `CaptureInteractive`; `/Scripting`
   (`loadScript`, `start`, `isScriptLoaded`); `zwlr_layer_shell_v1` v5; `wp_fractional_scale_manager_v1`; `xdg_wm_base` v6;
   two outputs `Virtual-0` / `Virtual-1` side by side (2880×1620 px each, second at x = 2880), so multi-output and
   scale 1.5 tests (PLAN P-12) are possible. **Authorization then tested (same day), and it works:**
   `tests/inplace/live/run_nested_kwin.sh` starts the nested KWin with isolated `XDG_*` directories under `tmp/`, writes a
   desktop entry with `X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2` and `Exec=` the path of the `geometry`
   example into the isolated data dir, refreshes the isolated KDE service cache (`kbuildsycoca6`), opens a Konsole window,
   finds its uuid through KWin's `WindowsRunner` (`krunner1.Match`), and runs `lipa-core`'s `geometry` example: client
   geometry, window frames and `grab_window` all succeed (`capture: 916x497`, 644 distinct colours; exit 0, PASS), with 2
   outputs at scale 1.5. No file of the user's session is touched and no process outlives the run (checked). The script
   returns 0/1/2 (captured / refused or failed / environment problem).
   **Full app inside the nested session — done 2026-11-22:** `tests/inplace/live/run_nested_app.sh` starts `lipax` itself (authorized by an isolated desktop entry, settings written for the run, translation unconfigured so nothing leaves the machine); it started, its KWin geometry tracker came up, it created its windows (main window, layer-shell translation surface) on `Virtual-0`, captured 916×497 frames of the Konsole window through ScreenShot2 and recognised text; PASS twice. **Still not covered:** the overlay's placement over the game, the script's focus/minimize/floating handling, and any assertion on pixel content or scale. Observation, not a conclusion: the
   captured frame equals the logical window size although the outputs were at scale 1.5 (Konsole may not use fractional
   scale; `wl_output` reports integer scale 1), so scale-correctness of capture still needs its own check.
2. **Version/capability probing at startup** — **partly done 2026-11-22:** the Status tab has a row «KWin» (`diagnostics.rs::kwin_row`: KWin version from `supportInformation`, ScreenShot2 interface `Version`; warning when the interface does not answer). Verified live on KWin 6.8.3 / ScreenShot2 v5 (`cargo test -p lipa-core -- --ignored live_kwin_row`). **Not done:** no minimum interface version is enforced (none was established for the options LipaX passes), no Plasma version, no periodic CI run against Arch's `kwin`. Original text: read KWin's and Plasma's versions and the ScreenShot2 interface `version`
   property, log them in the Status tab, and degrade with a clear message when a needed method or option is missing
   (the `NoAuthorized` case is already handled separately [code]). Pair it with a periodic CI run against the current
   Arch `plasma-desktop`/`kwin` packages so a Plasma update that breaks scripting is caught before users hit it.
3. **Focus-aware pause and profile selection** — **partly done 2026-11-22.** Found: capture/OCR already pause while the game is not active, but only when `appearance.inplace.only_when_active` is on (default off; `bridge.rs` geometry loop sets `shared.paused`). Added: a *minimised* game window now always pauses recognition (`ClientGeometry::is_minimized`, `KWin::window_minimized`; state test extended; **not live-tested**, a minimise in the nested KWin was not scripted). **Not done:** profile auto-select on focus — it means capturing whichever known game gets focus without the user choosing a window, a behaviour change that needs a decision. Original text: Already present [code: `geometry.js`]: the script reports `Focus`
   (active window and whether it is LipaX's own), `Minimized`, `Identity` (class, caption, pid) and geometry, and profiles
   are keyed by class + program name; the overlay is shown only over a visible, usable game. Still open
   [hypothesis: confirm in `pipeline/` before building]: auto-select the profile when its game gets focus, and pause
   capture/OCR (not only the overlay) while the game is unfocused or minimized, to save CPU. The 1.4.x notes list a
   "noticeable CPU share" of the process as a known limitation, so this is worth measuring.
4. **Fullscreen behaviour** — verify (not assume) how KWin treats a fullscreen/unredirected Proton game versus the
   layer-shell overlay layer, including `kwin` compositing on/off and fractional scale; record the result in the live
   matrix of R-13 and document the supported configurations in the README instead of the current "depends on your desktop".
5. **Extend the existing CLI.** **Done 2026-11-21:** `--toggle`, `--select-region`, `--translate-once`, `--toggle-overlay`, `--toggle-pin` (`instance.rs`; `bridge.rs::run_hotkey` is shared with KGlobalAccel; no-op without a running instance; not in the `.desktop` menu). Original text: `crates/app/src/instance.rs` already provides single-instance forwarding over D-Bus and
   flags/desktop actions for `show`, `settings`, `capture`, `start-autotranslate`, `stop-autotranslate`, `quit`
   [code]. The five hotkey actions (`toggle`, `select_region`, `translate_once`, `toggle_overlay`, `toggle_pin`;
   `hotkeys.rs`) are not reachable from it. Add them, so any shortcut mechanism (Plasma custom shortcuts, KRunner, scripts,
   and other compositors' keybinds, see R-14) can drive the same actions. Small and low risk.

Packaging stays Arch-first (PKGBUILD/AUR). A Flatpak would sandbox away the KWin D-Bus scripting LipaX relies on
[hypothesis, unverified], so it is not planned.

### R-11 Word lookup / Anki export — Low–Medium, M

Translumo has an unfinished cursor popup with "Add to Anki" via AnkiConnect on `127.0.0.1:8765`
[translumo: `TranslationOverlay`, `AnkiService`], and a commercial sibling (Lookupper) for language learning.
LipaX already knows word boxes from OCR and has history. Possible feature: hover a word in the overlay →
dictionary popup (offline wiktionary/dict data) → export original, translation and the captured crop to Anki.
Do this only if the project wants a learning audience; it is orthogonal to the translator core.

### R-12 Gamepad hotkeys — Low, M

Translumo mirrors each hotkey with an XInput binding [translumo: `HotKeysServiceManager`, `GamepadService`]. KGlobalAccel has
no gamepad support. On KDE gaming setups (Steam Deck desktop mode) this could use `evdev`/SDL in a background thread with
a rebind UI. Learn from its flaw: it reads the controller only at startup, so hot-plug must be handled.

**Done 2026-11-22** (`crates/core/src/gamepad.rs`, settings `hotkeys.gamepad.*`, Settings → Hotkeys → «Геймпад»). Design: the evdev
nodes under `/dev/input` are read by the `evdev` crate while `hotkeys.gamepad.enabled` is on (off by default: nothing is
opened otherwise); pads are devices with `BTN_SOUTH`; a rescan every 2 s finds pads plugged in later, a reader ends when its
pad is unplugged and the next rescan picks the pad up again. A chord is two or more buttons held at once (a single button
would fire during play); the action fires on the press that completes exactly the set. The d-pad is accepted both as buttons
and as a hat axis. Reading is passive, so the game still receives the buttons (documented in the UI). Rebind UI: «Записать»
waits up to 15 s for a chord (largest set held, accepted on release) while chord actions are suspended. Actions go through
the same `HotkeyEvent::Pressed` channel as KGlobalAccel, so `run_hotkey` is shared.
Verified: unit tests (parsing, matcher, recorder, hat/button mapping), an end-to-end test on a **virtual pad made through
`/dev/uinput`** (pad plugged in after the listener started, suspension, second chord, unplug + replug, switched off) and
`learn`, and a QML test of the settings page. **Not verified:** a physical Steam Deck / Xbox pad (the dev machine has none),
and permissions on a machine where the user may not read `/dev/input/event*` (the page then says so and names the `input`
group / udev `uaccess`). Analogue triggers and sticks are not buttons here (pads that report L2/R2 only as axes cannot use
them in a chord).

### R-13 Close the live-verification debt — High, M

Open items from `PLAN.md` ("not done": P-01, P-06, P-08, P-09, P-12, P-13), AUDIT-1.4.4 and IMPROVEMENTS.md
"Ограничения": portal capture never checked live; no multi-monitor / fractional-scale check; no real fullscreen
Proton game; two recorded-game screenshot tests were red even on clean HEAD; stale overlay-evidence manifest hashes
(W11/W12). **Re-triage done 2026-11-21** (checked against the tree; `PLAN.md` ids):

| PLAN id | Topic | Status now | Evidence |
|---|---|---|---|
| P-01 | Trustworthy Wayland/XWayland checks, nested KWin | **partly done** | nested runner `tests/inplace/live/run_nested_kwin.sh` exists and PASSes (ScreenShot2 v5, scripting, layer-shell, 2 outputs, scale 1.5; capture authorized, see R-10 item 1); still open: `platform=` startup log (only a `platformName()` use in `app_icon.h`), the XWayland case, manifest hash check |
| P-06 | Floating window must not snap/tile/maximize | **open** | `geometry.js` has no `tileChanged` / `maximizedChanged` / `fullScreenChanged` handling |
| P-08 | Reference screenshots, unique-colour test | **partly** | `tests/inplace/python/test_inplace_screenshots.py` and `docs/screenshots/test-game` references exist; the red recorded-game tests remain |
| P-09 | Rare re-capture during OCR | **open** | `STALE_PROBE` is still a fixed 150 ms (`pipeline/mod.rs:208`); measure the `capture` stage before changing |
| P-12 | Monitors and fractional scale | **open, testable** | no output-name keyed regions in settings, no scale tests; nested KWin offers `--output-count` and `--scale` |
| P-13 | No PNG via disk / gst PNG | **open, mitigated** | backdrop PNG encoding was moved off the GUI thread (commit `814fc23`) but PNG is still used |

Then run a single dated
**live test matrix** (single/dual monitor × 100 %/150 % scale × Wine/Proton window × borderless/fullscreen) and record
results in `docs/`. Each unverified claim in the README that depends on the desktop should either be verified or softened.

**Matrix run 2026-11-22 (nested compositors only):** `docs/LIVE-MATRIX-2026-11-22.md`. KWin: Wayland and XWayland clients, a Wine window,
1 and 2 outputs, 100 % and a **real** 150 % output scale (the first version of the matrix used `--scale`, which does not set a
scale: corrected there), and the whole application at the four output/scale combinations: all PASS; captured frame = client
geometry × output scale in every row. Capture costs 4–6 ms (P-09: nothing to change). Sway headless: PASS at 100 % and 150 %.
**Still open in R-13:** a Wine/Proton *game* (fullscreen, borderless, compositing off), portal capture, the overlay's placement
over a game and a physical second monitor. **P-06 done** (`keepFloating`, verified in the nested KWin with a control run).

### R-14 Other Wayland compositors — Strategic, L

**Starting point [code].** Everything compositor-specific sits behind a few seams: capture (`capture/kwin.rs`, `portal.rs`,
`CaptureCapabilities` flags), window geometry (embedded KWin script), global shortcuts (`hotkeys.rs`, KGlobalAccel) and
overlay windows (layer-shell via LayerShellQt). The portal capture path already works without KWin but disables the
in-place overlay, because it has no global coordinates.

**What a backend must provide** — a trait (`DesktopBackend` or similar) with four capabilities, each optional so a
compositor can be supported in tiers:

| Capability | KWin today | Candidate mechanisms elsewhere |
|---|---|---|
| Window capture | ScreenShot2 | `ext-image-copy-capture-v1` + `ext-image-capture-source-v1` (staging protocol); `wlr-screencopy` on wlroots; portal ScreenCast + PipeWire as universal fallback |
| Window list and geometry | KWin script | `ext-foreign-toplevel-list-v1` (identity only, no geometry); `wlr-foreign-toplevel-management`; compositor IPC (Hyprland `hyprctl`, niri IPC, Sway IPC) |
| Global shortcuts | KGlobalAccel | `xdg-desktop-portal` GlobalShortcuts; the compositor's own keybinds calling the CLI (R-10 item 5, partly exists) |
| Overlay above the game | layer-shell | `wlr-layer-shell` (wlroots, Smithay-based, KWin); none in Mutter |

**Proposed tiers** (what LipaX can promise per compositor):
- **Tier 1 — full:** KWin (current).
- **Tier 2 — window or region capture + separate translation window + keybinds via CLI:** Sway and other wlroots-based
  compositors, Hyprland, niri. The in-place overlay needs a way to learn the game window's position; Hyprland and Sway
  expose it through their IPC, niri has IPC too [hypothesis: confirm geometry fields per compositor before promising].
- **Tier 3 — portal-only, separate window, manual region:** GNOME/Mutter and anything else. Mutter has no layer-shell,
  so the pinned translation window and the in-place overlay would need a different approach, probably a GNOME Shell
  extension [hypothesis; the layer-shell question is reported as still disputed in 2026 sources].

**Findings from the 2026 survey (partial, verify before relying on them):**
- `ext-image-copy-capture-v1` is a staging protocol; check compositor and version support first. niri had an open PR to add
  it in April 2026; Hyprland lists `ext-foreign-toplevel-list` support. A KDE bug asking KWin to implement the capture
  protocols is marked "resolved intentional", i.e. KWin keeps its own ScreenShot2 rather than the ext protocol, so a
  generic ext-protocol backend would not replace the KWin backend [sources below].
- GlobalShortcuts portal: reported working with KDE and Hyprland portals; GNOME support is reported but conflicting;
  Sway (wlr portal) and COSMIC reported unsupported, hence the CLI fallback for those.
- `wlr-layer-shell`: reported as implemented by wlroots compositors, Smithay-based COSMIC and KWin, and not by Mutter.

**Plan.** (1) Extract the trait without changing behaviour and keep KWin as the only implementation (a pure refactor,
guarded by existing tests). (2) Finish the CLI so every hotkey action is reachable (R-10 item 5; the single-instance CLI already exists). (3) Add one wlroots/Hyprland/niri
backend as a spike, in the order you can actually test: pick the compositor you will run daily. (4) Document tiers in the
README with an honest support matrix. Do not claim a compositor until R-13-style live checks have been run on it.
Prerequisite: R-13 and R-10 item 1, so the primary backend is trusted and testable before a second one multiplies the matrix.

**Progress 2026-11-22.** Step 1 **done** (`crates/core/src/desktop.rs`): trait `DesktopBackend` (`window_state`: geometry, focus, minimised),
KWin implements it; the app's geometry loop calls the trait and the pure `overlay_policy`; behaviour unchanged; a test drives the
policy through a fake second backend. Step 2 (CLI for every hotkey action) was done under R-10 item 5. Step 3 **done at library
level** for sway (`crates/core/src/capture/sway.rs`): `SwayCapture` implements `DesktopBackend` (window list, client rectangle and focus
from the i3 IPC `GET_TREE`) and `Capture` (`grim` cut to the client rectangle), with the capabilities `CaptureCapabilities::WLROOTS`.
Verified in a **headless sway 1.12** (`tests/inplace/live/run_sway.sh`, sway and wlroots unpacked from the repository packages into
`tmp/`, nothing installed): the window is found, its geometry and focus read, the frame captured at the native resolution (400×300 at
100 %, 600×450 at 150 %), a region cut, a vanished window reported as `WindowGone`; unit tests parse a `GET_TREE` reply. Step 4 done:
`support_for`, the Status row «Рабочий стол», the README table. **Wired into the application 2026-11-22:** `AnyCapture` serves `sway:` keys (capture, `window_state`, capabilities), the geometry
loop and the frame outline use `AnyCapture::window_state` for KWin and sway alike, and with `capture.source = auto` in a sway session the
window picker takes the next window that gets the focus. The whole application ran in a headless sway (`run_sway_app.sh`): frames
captured at 100 % and 150 %, text recognised, geometry known. **What is not done:** the overlay over a game and the focus-click picker were not exercised
on sway; Hyprland (`hyprctl`) and niri (IPC) have no backend; the floating translation window has no sway placement (that is a KWin
script); GNOME/Mutter is untouched.

Sources: [wayland.app: ext-image-copy-capture-v1](https://wayland.app/protocols/ext-image-copy-capture-v1),
[wayland.app: ext-foreign-toplevel-list-v1](https://wayland.app/protocols/ext-foreign-toplevel-list-v1),
[KDE bug 513785](https://bugs.kde.org/show_bug.cgi?id=513785),
[Wayland protocol surface comparison: margo vs niri vs Hyprland](https://kenanpelit.github.io/margo/protocol-comparison/),
[xdg-desktop-portal-hyprland releases](https://www.github.com/hyprwm/xdg-desktop-portal-hyprland/releases?page=2),
[GNOME GlobalShortcuts portal issue](https://gitlab.gnome.org/GNOME/xdg-desktop-portal-gnome/-/issues/85),
[Arch Wiki: Wayland](https://wiki.archlinux.org/title/Wayland).

## 3. Things deliberately not proposed

- **Porting Translumo's ML.NET models or its `HttpWebRequest`/WinRT stack** — Windows-bound and the models are not in its repo.
- **Embedded Python for the main path** — LipaX's in-process `ort` approach is a strength; keep Python only for PaddleOCR.
- **A Windows/macOS build, or an X11-only backend** — out of scope: LipaX is Linux + Wayland by decision. The value is in being the best Linux
  game-text translator, not a Translumo clone.
- **Text-to-speech** — not required by the project owner; Translumo's TTS is deliberately not carried over.
- **Cloud OCR** — it would conflict with the offline/privacy positioning (Bergamot + local OCR).

## 4. Risks and open questions

1. R-01's scorer helps only if candidate texts differ materially across engines; measure the disagreement rate on the
   recorded frames before building it.
2. R-02 depends on the user's ROCm stack, not on LipaX code; success is not guaranteed on gfx1031.
3. R-14 multiplies the live-test matrix by the number of compositors; tiers and an honest support matrix keep the promise small.
4. R-10 items 1 and 4 depend on whether a nested KWin session can host ScreenShot2 and the overlay; if not, R-13 stays a manual live matrix.
5. The test count (580) is the number of test attributes, not a passing run; the suite was not executed for this document.
   File sizes and CI contents come from a read-only survey.

## 5. Revision log

- **2026-11-21 (first version).** Initial roadmap from a code read-through and the Translumo study.
- **2026-11-21 (scope).** Linux-only, then Wayland-only with KWin primary; TTS dropped; R-14 added from a partial web survey.
- **2026-11-21 (execution start).** R-13 re-triage table added; R-05 measured (≈0.17 s fixed cost per Bergamot call) and
  re-rated from High to Medium; recommended order changed so R-05 follows R-03/R-01/R-02.
- **2026-11-21 (R-03 setting, R-04).** `general.language` + selector; rolling translation log in the translation window.
- **2026-11-21 (languages).** fr, zh_CN, ja, es catalogues added (Haiku translations, unreviewed); `build.rs`/`ui_language.rs` generalized.
- **2026-11-21 (R-01 measured).** `engine_disagreement` example: RapidOCR CER 0.8–1.4 % vs Tesseract 28–30 % on the test frames; a
  perfect selector gains ≈0.1 point. R-01 re-rated to Low; new R-01b (RapidOCR-first in `Auto`) needs a decision.
- **2026-11-21 (R-02, R-03).** R-02 item 1: Status row "GPU для RapidOCR / MeikiOCR" (`ocr::rapid::gpu_report`, test added).
  R-03: QML wrapped in `qsTr`, English catalogue, `lrelease` in `build.rs`, language selection, verified with a snapshot.
- **2026-11-21 (nested runner).** Added and ran `tests/inplace/live/run_nested_kwin.sh`: ScreenShot2 capture of a client
  window inside an isolated nested KWin PASSes (916×497 frame, 2 outputs, scale 1.5). P-01 re-marked as partly done.
- **2026-11-21 (nested KWin probe).** R-10 item 1 feasibility confirmed on this machine (details in R-10); P-01 and
  P-12 re-marked as unblocked/testable.
- **2026-11-21 (R-08 wallet).** Opt-in Secret Service storage (`secrets.rs`, `general.keys_in_wallet`, checkbox, Status row,
  catalogues, tests, real-wallet round trip passed on `ksecretd`).
- **2026-11-21 (R-08, R-09).** API keys redacted from `Debug` (test added); clippy `-D warnings` gate, manual package job and
  tag-triggered release job added to `checks.yml`; `fmt` gate left out (tree not formatted).
- **2026-11-21 (R-05).** Persistent `bergamot-server` + Rust worker with one-shot fallback; 180 ms → 11 ms median per uncached line.
- **2026-11-21 (re-check).** Verified against the tree and the machine:
  - R-08: settings file is already `0600` → priority lowered.
  - R-02: a GPU setting and UI checkbox already exist; the gap is feedback about the provider actually used.
  - R-10 item 3: focus/minimized/identity reporting already exists in `geometry.js`; only profile auto-select and
    pause-on-unfocus remain open.
  - R-10 item 5 / R-14: a single-instance CLI already exists; the task is to expose the five hotkey actions.
  - Test count (580 attributes) and the absence of `qsTr` (0 occurrences, `SettingsWindow.qml` 4272 lines) confirmed.
  - Still unverified: ROCm on gfx1031, LipaX's ScreenShot2 authorization inside nested KWin, per-compositor IPC geometry, Flatpak.
- **2026-11-21 (R-07).** KWin restart resilience implemented in `capture/kwin.rs` + `bridge.rs`; unit-tested, not live-tested.
- **2026-11-21 (R-10 item 5).** CLI flags for the five hotkey actions; unit-tested, CLI smoke-tested without a running instance, not tested against a running one.
- **2026-11-21 (R-06).** Pivot through English and script-based source detection for Bergamot; unit-tested only.
- **2026-11-22 (R-10 item 2).** Status row «KWin» (version + ScreenShot2 interface version), live-checked; R-09 row marked in the table.
- **2026-11-22 (R-10 items 1 and 3, R-09).** `run_nested_app.sh` (whole app in nested KWin, PASS); a minimised game window pauses recognition; advisory `rustfmt` step in CI.
