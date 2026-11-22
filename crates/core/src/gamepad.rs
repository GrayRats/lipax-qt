//! Gamepad hotkeys (R-12): the hotkey actions on a controller, for the Steam Deck in desktop mode and couch setups.
//!
//! KGlobalAccel has no gamepad support, so the evdev nodes of the pads (`/dev/input/event*`) are read in the
//! background. Reading is passive: the game still gets every button, so a chord should be one the game does not use
//! (`Select+Start`, `L3+R3`). Nothing under `/dev/input` is opened while the option is off.
//!
//! A chord is two or more buttons held together; the action fires when the press that completes exactly the set arrives.
//! A pad plugged in later is found by rescanning every [`RESCAN`] (the translumo flaw this avoids: it reads the pad
//! only at start-up); one that is unplugged ends its reader and is picked up again when it returns.

use crate::hotkeys::{HotkeyAction, HotkeyEvent};
use crate::settings::{GamepadHotkeys, Hotkeys};
use evdev::{AbsoluteAxisCode, Device, EventType, KeyCode};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::{mpsc, watch};

/// How often `/dev/input` is searched for pads that appeared.
pub const RESCAN: Duration = Duration::from_secs(2);
/// How long [`learn`] waits for a chord.
pub const LEARN_TIMEOUT: Duration = Duration::from_secs(15);

/// Names of the buttons, Xbox layout as evdev reports it (`BTN_SOUTH` is A). The first name of a code is the canonical
/// one; the others are accepted when parsing. Codes of the d-pad are those of `BTN_DPAD_*`: a pad that reports its
/// d-pad as a hat axis is mapped onto them (`hat`).
const BUTTONS: &[(u16, &str)] = &[
    (0x130, "A"), (0x131, "B"), (0x134, "X"), (0x133, "Y"),
    (0x136, "L1"), (0x137, "R1"), (0x138, "L2"), (0x139, "R2"),
    (0x13a, "Select"), (0x13b, "Start"), (0x13c, "Mode"), (0x13d, "L3"), (0x13e, "R3"),
    (0x220, "Up"), (0x221, "Down"), (0x222, "Left"), (0x223, "Right"),
];
const ALIASES: &[(&str, u16)] = &[
    ("South", 0x130), ("East", 0x131), ("West", 0x134), ("North", 0x133),
    ("LB", 0x136), ("RB", 0x137), ("LT", 0x138), ("RT", 0x139),
    ("Back", 0x13a), ("View", 0x13a), ("Menu", 0x13b), ("Guide", 0x13c), ("Home", 0x13c),
    ("ThumbL", 0x13d), ("ThumbR", 0x13e),
];
const BTN_SOUTH: u16 = 0x130;
#[cfg(test)]
const DPAD: [u16; 4] = [0x220, 0x221, 0x222, 0x223];

/// The name of a button code, if it is one a chord can use.
pub fn button_name(code: u16) -> Option<&'static str> {
    BUTTONS.iter().find(|(c, _)| *c == code).map(|(_, n)| *n)
}

fn button_code(name: &str) -> Option<u16> {
    let name = name.trim();
    BUTTONS.iter().map(|(c, n)| (*n, *c)).chain(ALIASES.iter().copied())
        .find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, c)| c)
}

/// `"L1 + R1 + Start"` → the set of codes. At least two different buttons (a single button would fire in the middle of
/// the game); `None` for an empty string, an unknown name or a single button.
pub fn parse_chord(text: &str) -> Option<BTreeSet<u16>> {
    let set: BTreeSet<u16> = text.split('+').map(button_code).collect::<Option<_>>()?;
    (set.len() >= 2).then_some(set)
}

/// The text of a chord in the canonical order of [`BUTTONS`], as the settings keep it.
pub fn chord_text(chord: &BTreeSet<u16>) -> String {
    BUTTONS.iter().filter(|(c, _)| chord.contains(c)).map(|(_, n)| *n).collect::<Vec<_>>().join("+")
}

fn chord_of(action: HotkeyAction, g: &GamepadHotkeys) -> &str {
    match action {
        HotkeyAction::Toggle => &g.toggle,
        HotkeyAction::SelectRegion => &g.select_region,
        HotkeyAction::TranslateOnce => &g.translate_once,
        HotkeyAction::ToggleTranslation => &g.toggle_translation,
        HotkeyAction::TogglePin => &g.toggle_pin,
    }
}

/// Which buttons of one pad are held, and which action the press that just arrived completes.
#[derive(Debug, Default)]
pub struct ChordMatcher {
    chords: Vec<(HotkeyAction, BTreeSet<u16>)>,
    held: BTreeSet<u16>,
}

impl ChordMatcher {
    /// The chords of the settings (an unparsable or empty one is not assigned).
    pub fn new(g: &GamepadHotkeys) -> Self {
        let chords = HotkeyAction::ALL.into_iter().filter_map(|a| parse_chord(chord_of(a, g)).map(|c| (a, c))).collect();
        Self { chords, held: BTreeSet::new() }
    }

    /// A button went down: the action it completes, if the held buttons are now exactly the chord of one.
    pub fn press(&mut self, code: u16) -> Option<HotkeyAction> {
        if !self.held.insert(code) { return None; }
        self.chords.iter().find(|(_, c)| *c == self.held).map(|(a, _)| *a)
    }

    /// A button went up.
    pub fn release(&mut self, code: u16) {
        self.held.remove(&code);
    }

    /// The pad was lost or the settings changed: nothing is held any more.
    pub fn reset(&mut self) {
        self.held.clear();
    }
}

/// Finds the chord a user presses: the most buttons held at once, reported when all are released again.
#[derive(Debug, Default)]
pub struct ChordRecorder {
    held: BTreeSet<u16>,
    peak: BTreeSet<u16>,
}

impl ChordRecorder {
    /// A button went down.
    pub fn press(&mut self, code: u16) {
        self.held.insert(code);
        if self.held.len() > self.peak.len() { self.peak = self.held.clone(); }
    }

    /// A button went up; `Some` when that released the last one of a chord of two or more.
    pub fn release(&mut self, code: u16) -> Option<String> {
        self.held.remove(&code);
        if !self.held.is_empty() { return None; }
        let chord = std::mem::take(&mut self.peak);
        (chord.len() >= 2).then(|| chord_text(&chord))
    }
}

/// A button event of a pad in terms of chord buttons: `(code, pressed)`. Buttons and the d-pad as a hat axis;
/// anything else (sticks, analogue triggers, auto-repeat) is `None`.
fn button_event(kind: EventType, code: u16, value: i32) -> Option<Vec<(u16, bool)>> {
    if kind == EventType::KEY && value != 2 {
        return button_name(code).map(|_| vec![(code, value != 0)]);
    }
    if kind == EventType::ABSOLUTE && (code == AbsoluteAxisCode::ABS_HAT0X.0 || code == AbsoluteAxisCode::ABS_HAT0Y.0) {
        let (negative, positive) = if code == AbsoluteAxisCode::ABS_HAT0X.0 { (0x222, 0x223) } else { (0x220, 0x221) };
        // A hat sits at -1, 0 or 1: leaving one side releases it, entering one presses it.
        return Some(vec![(negative, value < 0), (positive, value > 0)]);
    }
    None
}

/// Whether the device is a gamepad: it has the south button (`BTN_GAMEPAD`). Keyboards, mice and touchpads do not.
fn is_gamepad(device: &Device) -> bool {
    device.supported_keys().is_some_and(|keys| keys.contains(KeyCode(BTN_SOUTH)))
}

/// What the machine offers: the names of the pads that can be read and how many input devices were refused to us.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Probe {
    pub gamepads: Vec<String>,
    /// Device nodes under `/dev/input` the user may not open (the pad may be among them: it needs the `input` group or
    /// a `uaccess` rule of udev).
    pub denied: usize,
}

/// Looks at `/dev/input` once. Opens every node (blocking: call it off the runtime).
pub fn probe() -> Probe {
    let mut probe = Probe::default();
    let Ok(entries) = std::fs::read_dir("/dev/input") else { return probe };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("event")) { continue; }
        match Device::open(&path) {
            Ok(device) if is_gamepad(&device) => probe.gamepads.push(device.name().unwrap_or("gamepad").to_owned()),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => probe.denied += 1,
            Err(_) => {}
        }
    }
    probe.gamepads.sort();
    probe
}

/// Reads the pads while `hotkeys.gamepad.enabled` is on and sends the actions of the chords to `tx`. Pads that appear
/// later are found by rescanning. While `suspended` is set (a chord is being recorded) pressed chords are not acted on.
/// Runs until `tx` is closed.
pub async fn listen(mut hotkeys: watch::Receiver<Hotkeys>, suspended: Arc<AtomicBool>, tx: mpsc::UnboundedSender<HotkeyEvent>) {
    let mut readers: HashMap<PathBuf, tokio::task::JoinHandle<()>> = HashMap::new();
    let mut ticker = tokio::time::interval(RESCAN);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        readers.retain(|_, reader| !reader.is_finished());
        if hotkeys.borrow().gamepad.enabled {
            if tx.is_closed() { break; }
            let found = tokio::task::spawn_blocking(open_new_gamepads).await.unwrap_or_default();
            for (path, device) in found {
                if readers.contains_key(&path) { continue; }
                tracing::info!(target: "gamepad", name = device.name().unwrap_or("?"), path = %path.display(), "gamepad found");
                let Ok(stream) = device.into_event_stream() else { continue };
                readers.insert(path, tokio::spawn(read_pad(stream, hotkeys.clone(), suspended.clone(), tx.clone())));
            }
        } else {
            for (_, reader) in readers.drain() { reader.abort(); }
        }
        tokio::select! {
            _ = ticker.tick() => {}
            changed = hotkeys.changed() => if changed.is_err() { break },
        }
    }
    for (_, reader) in readers { reader.abort(); }
}

fn open_new_gamepads() -> Vec<(PathBuf, Device)> {
    evdev::enumerate().filter(|(_, device)| is_gamepad(device)).collect()
}

/// One pad until it is unplugged.
async fn read_pad(mut stream: evdev::EventStream, mut hotkeys: watch::Receiver<Hotkeys>, suspended: Arc<AtomicBool>, tx: mpsc::UnboundedSender<HotkeyEvent>) {
    let mut matcher = ChordMatcher::new(&hotkeys.borrow_and_update().gamepad);
    loop {
        tokio::select! {
            event = stream.next_event() => {
                let Ok(event) = event else { break };
                for (code, down) in button_event(event.event_type(), event.code(), event.value()).unwrap_or_default() {
                    if !down { matcher.release(code); continue; }
                    let action = matcher.press(code);
                    if let Some(action) = action.filter(|_| !suspended.load(Ordering::Relaxed)) {
                        tracing::debug!(target: "gamepad", action = action.title(), "chord pressed");
                        if tx.send(HotkeyEvent::Pressed(action)).is_err() { return; }
                    }
                }
            }
            changed = hotkeys.changed() => {
                if changed.is_err() { break; }
                matcher = ChordMatcher::new(&hotkeys.borrow_and_update().gamepad);
            }
        }
    }
    tracing::info!(target: "gamepad", "gamepad gone");
}

/// Waits for the user to press a chord on any pad (the most buttons held at once, accepted when released). `None` after
/// [`LEARN_TIMEOUT`] or without a readable pad.
pub async fn learn() -> Option<String> {
    let devices = tokio::task::spawn_blocking(open_new_gamepads).await.ok()?;
    let (found_tx, mut found_rx) = mpsc::unbounded_channel();
    let mut tasks = Vec::new();
    for (_, device) in devices {
        let Ok(mut stream) = device.into_event_stream() else { continue };
        let found_tx = found_tx.clone();
        tasks.push(tokio::spawn(async move {
            let mut recorder = ChordRecorder::default();
            while let Ok(event) = stream.next_event().await {
                for (code, down) in button_event(event.event_type(), event.code(), event.value()).unwrap_or_default() {
                    if down { recorder.press(code); } else if let Some(chord) = recorder.release(code) {
                        let _ = found_tx.send(chord);
                        return;
                    }
                }
            }
        }));
    }
    drop(found_tx);
    let chord = tokio::time::timeout(LEARN_TIMEOUT, found_rx.recv()).await.ok().flatten();
    for task in tasks { task.abort(); }
    chord
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chords(toggle: &str, once: &str) -> GamepadHotkeys {
        GamepadHotkeys { enabled: true, toggle: toggle.into(), translate_once: once.into(), ..Default::default() }
    }

    #[test]
    fn chords_parse_by_name_alias_and_case_and_need_two_buttons() {
        assert_eq!(parse_chord("select + start"), Some([0x13a, 0x13b].into()));
        assert_eq!(parse_chord("Back+Menu"), parse_chord("Select+Start"), "Xbox names are aliases");
        assert_eq!(parse_chord("South+West"), parse_chord("A+X"));
        assert_eq!(parse_chord("L1"), None, "a single button would fire in the middle of the game");
        assert_eq!(parse_chord("L1+L1"), None, "the same button twice is one");
        assert_eq!(parse_chord("L1+Banana"), None);
        assert_eq!(parse_chord(""), None);
    }

    #[test]
    fn a_chord_is_written_in_the_canonical_order_and_parses_back() {
        let chord = parse_chord("Start+L1+R1").unwrap();
        assert_eq!(chord_text(&chord), "L1+R1+Start");
        assert_eq!(parse_chord(&chord_text(&chord)), Some(chord));
    }

    #[test]
    fn the_press_that_completes_exactly_the_chord_fires_once() {
        let mut m = ChordMatcher::new(&chords("Select+Start", "L1+R1+Start"));
        assert_eq!(m.press(0x13a), None);
        assert_eq!(m.press(0x13b), Some(HotkeyAction::Toggle));
        assert_eq!(m.press(0x13b), None, "a repeated press of a held button does nothing");
        // Another button on top: the set is no longer the chord.
        assert_eq!(m.press(0x136), None);
        m.release(0x13a);
        m.release(0x136);
        m.release(0x13b);
        // Buttons in another order, and a longer chord that contains the first one's button.
        assert_eq!(m.press(0x13b), None);
        assert_eq!(m.press(0x137), None);
        assert_eq!(m.press(0x136), Some(HotkeyAction::TranslateOnce));
    }

    #[test]
    fn a_lost_pad_forgets_what_was_held() {
        let mut m = ChordMatcher::new(&chords("Select+Start", ""));
        m.press(0x13a);
        m.reset();
        assert_eq!(m.press(0x13b), None, "Select was released while the pad was away");
    }

    #[test]
    fn the_recorder_reports_the_largest_set_when_everything_is_released() {
        let mut r = ChordRecorder::default();
        r.press(0x136);
        r.press(0x137);
        r.press(0x13b);
        assert_eq!(r.release(0x13b), None);
        assert_eq!(r.release(0x136), None);
        assert_eq!(r.release(0x137).as_deref(), Some("L1+R1+Start"));
        // A single button is not a chord: nothing is reported and the next try starts clean.
        r.press(0x130);
        assert_eq!(r.release(0x130), None);
        r.press(0x13a);
        r.press(0x13b);
        assert_eq!(r.release(0x13a), None);
        assert_eq!(r.release(0x13b).as_deref(), Some("Select+Start"));
    }

    #[test]
    fn buttons_and_the_hat_become_chord_buttons_and_the_rest_is_ignored() {
        assert_eq!(button_event(EventType::KEY, 0x13b, 1), Some(vec![(0x13b, true)]));
        assert_eq!(button_event(EventType::KEY, 0x13b, 0), Some(vec![(0x13b, false)]));
        assert_eq!(button_event(EventType::KEY, 0x13b, 2), None, "auto-repeat");
        assert_eq!(button_event(EventType::KEY, 30, 1), None, "a key of a keyboard");
        assert_eq!(button_event(EventType::ABSOLUTE, AbsoluteAxisCode::ABS_HAT0X.0, -1), Some(vec![(0x222, true), (0x223, false)]));
        assert_eq!(button_event(EventType::ABSOLUTE, AbsoluteAxisCode::ABS_HAT0Y.0, 0), Some(vec![(0x220, false), (0x221, false)]));
        assert_eq!(button_event(EventType::ABSOLUTE, AbsoluteAxisCode::ABS_X.0, 100), None, "a stick");
        assert!(DPAD.iter().all(|c| button_name(*c).is_some()));
    }

    /// A virtual pad made through `/dev/uinput` with the buttons of the chords; `None` (the test is skipped) where the
    /// machine does not allow it.
    fn virtual_pad(name: &str) -> Option<evdev::uinput::VirtualDevice> {
        let mut keys = evdev::AttributeSet::<KeyCode>::new();
        for code in [0x130, 0x136, 0x137, 0x13a, 0x13b] { keys.insert(KeyCode(code)); }
        let mut pad = evdev::uinput::VirtualDevice::builder().ok()?.name(name).with_keys(&keys).ok()?.build().ok()?;
        // The node appears at once but its permissions are set by udev a moment later.
        let node = pad.enumerate_dev_nodes_blocking().ok()?.flatten().next()?;
        for _ in 0..40 {
            if Device::open(&node).is_ok() { return Some(pad); }
            std::thread::sleep(Duration::from_millis(50));
        }
        eprintln!("skipped: the node {} of the virtual pad cannot be read by this user", node.display());
        None
    }

    fn press(pad: &mut evdev::uinput::VirtualDevice, codes: &[u16]) {
        for &code in codes { pad.emit(&[evdev::InputEvent::new(EventType::KEY.0, code, 1)]).unwrap(); }
        for &code in codes.iter().rev() { pad.emit(&[evdev::InputEvent::new(EventType::KEY.0, code, 0)]).unwrap(); }
    }

    /// The whole path on a virtual pad: a pad plugged in after the listener started is found, its chord reaches the
    /// channel, it is ignored while a chord is being recorded, and after the pad is unplugged and plugged in again it
    /// works again.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_pad_plugged_in_later_unplugged_and_plugged_in_again_keeps_working() {
        let hotkeys = Hotkeys { gamepad: chords("Select+Start", "L1+R1"), ..Hotkeys::default() };
        let (hk_tx, hk_rx) = watch::channel(hotkeys);
        let suspended = Arc::new(AtomicBool::new(false));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let listener = tokio::spawn(listen(hk_rx, suspended.clone(), tx));
        let name = format!("lipax-test-pad-{}", std::process::id());
        let wait = |rx: &mut mpsc::UnboundedReceiver<HotkeyEvent>| {
            let rx = rx as *mut mpsc::UnboundedReceiver<HotkeyEvent>;
            // Rescans happen every RESCAN; give three of them.
            async move { tokio::time::timeout(RESCAN * 3 + Duration::from_secs(1), unsafe { &mut *rx }.recv()).await }
        };
        // 1. The pad appears after the listener is running.
        let Some(mut pad) = virtual_pad(&name) else { listener.abort(); return };
        let mut pressed = None;
        for _ in 0..8 {
            press(&mut pad, &[0x13a, 0x13b]);
            if let Ok(Some(event)) = tokio::time::timeout(RESCAN / 2, rx.recv()).await { pressed = Some(event); break; }
        }
        assert!(matches!(pressed, Some(HotkeyEvent::Pressed(HotkeyAction::Toggle))), "{pressed:?}");
        // 2. A chord pressed while one is being recorded does nothing.
        suspended.store(true, Ordering::Relaxed);
        press(&mut pad, &[0x13a, 0x13b]);
        assert!(tokio::time::timeout(Duration::from_millis(500), rx.recv()).await.is_err(), "suspended");
        suspended.store(false, Ordering::Relaxed);
        // 3. Another chord of the same pad.
        press(&mut pad, &[0x136, 0x137]);
        assert!(matches!(wait(&mut rx).await, Ok(Some(HotkeyEvent::Pressed(HotkeyAction::TranslateOnce)))));
        // 4. Unplugged and plugged in again: found again by the rescan.
        drop(pad);
        tokio::time::sleep(Duration::from_millis(300)).await;
        let Some(mut pad) = virtual_pad(&name) else { listener.abort(); return };
        let mut again = None;
        for _ in 0..8 {
            press(&mut pad, &[0x13a, 0x13b]);
            if let Ok(Some(event)) = tokio::time::timeout(RESCAN / 2, rx.recv()).await { again = Some(event); break; }
        }
        assert!(matches!(again, Some(HotkeyEvent::Pressed(HotkeyAction::Toggle))), "{again:?}");
        // 5. Turned off: the reader is gone and the chord does nothing.
        hk_tx.send_modify(|h| h.gamepad.enabled = false);
        tokio::time::sleep(Duration::from_millis(300)).await;
        press(&mut pad, &[0x13a, 0x13b]);
        assert!(tokio::time::timeout(Duration::from_millis(500), rx.recv()).await.is_err(), "off");
        listener.abort();
    }

    /// `learn` reports the chord pressed on a virtual pad.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn learn_reports_the_chord_pressed_on_a_pad() {
        let Some(mut pad) = virtual_pad(&format!("lipax-learn-pad-{}", std::process::id())) else { return };
        let learning = tokio::spawn(learn());
        tokio::time::sleep(Duration::from_millis(700)).await;
        press(&mut pad, &[0x136, 0x137, 0x13b]);
        assert_eq!(learning.await.unwrap().as_deref(), Some("L1+R1+Start"));
    }
}
