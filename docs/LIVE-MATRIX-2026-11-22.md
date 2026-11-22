# Live test matrix, 2026-11-22 (R-13, R-14)

Where: **nested compositors** on a private D-Bus / runtime directory under `tmp/`, so the user's session is not touched:
a virtual KWin 6.8.3 (ScreenShot2 v5; `tests/inplace/live/run_nested_kwin.sh`, `run_nested_app.sh`) and a headless sway 1.12
(wlroots 0.20, pixman renderer; `tests/inplace/live/run_sway.sh`). No physical monitor, no game, no real GPU path.

## A correction of the first run of this matrix

The first version of this document (earlier on 2026-11-22) listed "scale 1.5" rows that were **not** fractional-scale runs: the
`--scale 1.5` option of `kwin_wayland --virtual` only makes the mode 1.5 times larger at output scale **1** (checked with
`kscreen-doctor -j`: `Virtual-0 scale 1, 2880×1620`). The scripts now start KWin at 1920×1080 and set the real output scale
through KWin's output management (`kscreen-doctor output.<name>.scale.1.5`, `kscreen-doctor -j` then reports `scale 1.5,
1920×1080`). Everything below was run again with that. The earlier observation "the capture is not larger at 1.5" was a result of
the broken option, not of KWin.

## KWin (nested, real output scale)

| Client | Outputs | Scale | Result |
|---|---|---|---|
| Konsole (Wayland) | 1 | 1.0 | PASS, capture 1920×1080 |
| Konsole (Wayland) | 1 | 1.5 | PASS, capture 1374×746 (= client 916×497 × 1.5) |
| Konsole (Wayland) | 2 | 1.0 | PASS, capture 916×497 |
| Konsole (Wayland) | 2 | 1.5 | PASS, capture 1374×746 |
| Qt window (Wayland, fractional scale aware) | 1 and 2 | 1.0 and 1.5 | PASS ×4: the frame equals the client geometry × scale (406×333 at 1.0, 609×500 at 1.5; Qt draws its own frame in a nested KWin) |
| Qt window as an **X11 client** (XWayland) | 1 | 1.0 | PASS, 400×300 |
| Qt window as an X11 client (XWayland) | 2 | 1.5 | PASS, 600×450 = client 400×300 × 1.5 (KWin rescales the X11 buffer: the frame is not pixel-exact) |
| **Wine** (Notepad, XWayland) | 1 | 1.0 | PASS, client 1018×735, capture 1018×735 |
| Wine (Notepad, XWayland) | 1 | 1.5 | PASS, client 678.7×490 logical, capture 1018×735 (= client × 1.5: the window is scale-unaware, KWin reports its logical size) |
| Whole `lipax` (capture + recognition, 0 authorisation refusals) | 1 and 2 | 1.0 and 1.5 | PASS ×4, the application received 1374×746 frames at 1.5 |

Rule that held in every KWin row: **the captured frame is the client geometry in logical px times the output scale**, which is what
`client_crop` assumes (`scale = frame width / client width`). The cost of a capture is small: 60 consecutive `grab_window` calls
took a median of 6.1 ms (1920×1080) and 3.7 ms (1374×746), p95 6.6 / 4.0 ms (P-09: the re-capture during OCR every 150 ms,
`STALE_PROBE`, costs about 4 % of a core at most, so it is not changed).

## Sway (headless, `run_sway.sh`)

| Scale | Result |
|---|---|
| 1.0 | PASS: window found through the IPC (`GET_TREE`), client rectangle 760,390 400×300, focus read, `grim` capture 400×300, a region of it 200×150, an unknown id gives `WindowGone` |
| 1.5 | PASS: capture 600×450 (= 400×300 × 1.5), region 300×225 |

This is the backend of R-14 step 3 (`capture/sway.rs`: `SwayCapture` = `DesktopBackend` + `Capture`, key `sway:<id>`).

**The whole application on sway** (`run_sway_app.sh`, headless sway, a Konsole window chosen by its `sway:<id>` key, capture source
`auto`): PASS at 100 % (3 frames of 900×500, text recognised) and at 150 % (frames of 1350×750 = 900×500 × 1.5, text recognised);
the in-place layer received the window geometry from the sway IPC. KGlobalAccel is absent on sway, as expected: the hotkeys
come from the command line (`lipax --toggle` …) and, since R-12, from the gamepad. The window picker of sway takes the next
window that gets the focus (sway has no interactive picker); that path is only unit-tested for the parsing, the click itself
was not driven.

## Not covered

- A Wine/Proton **game** (a notepad window is not a fullscreen, borderless or 3D game), `kwin` compositing off, unredirected
  fullscreen.
- Portal capture (needs a PipeWire stream of a real window).
- The overlay's placement over a game and the script's focus and minimise handling in a nested compositor.
- A physical second monitor and a physical gamepad.
- Hyprland, niri and the other compositors (no backend); sway has only been run headless: the overlay over a game, the picker's
  focus-click and the translation window were not exercised there.

## PLAN items (re-checked)

| PLAN id | Now |
|---|---|
| P-01 | the nested runner covers Wayland and XWayland clients, Wine, 1 and 2 outputs, 100 % and 150 %; the manifest hash check is still open |
| P-06 | **done 2026-11-22, verified in the nested KWin** (`run_floating_policy.sh`): `keepFloating` in `geometry.js` undoes tiling, fullscreen and maximise of the floating translation window a few turns of the event loop after KWin announces them (KWin ignores a change made from inside the announcing signal: the first attempt, which did that, was withdrawn). The control run without it ends with the tile set and the window fullscreen; with it both are undone and a maximise is switched on and off again (2 events). Not driven: a real LipaX floating window (the harness applies the function to a Konsole window) and a drag to a screen edge |
| P-09 | measured above: no change needed |
| P-12 | multi-output and real fractional scale are now exercised (see the correction above); output-name keyed regions in the settings are still missing |
| P-13 | open, mitigated (PNG encoding is off the GUI thread) |
