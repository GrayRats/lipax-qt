//! The state of one region: its phase in the cycle (a checked state machine), the retry budget after errors and
//! how far its capture is extended upwards.
use super::{Event, PreviewBox, Sink};
use crate::{detect::ChangeDetector, layout::engine::InplaceEngine, settings::{CaptureRegion, NormRect, Settings}};
use std::{collections::HashMap, time::{Duration, Instant}};

pub(super) const ERROR_BUDGET: Duration = Duration::from_secs(60);

/// Where a region is in its cycle. Every change is logged (`pipeline.phase`), so a stuck region
/// or a surprising transition can be read from the log; the inspector shows the current one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    /// Nothing changed on screen (also: no text found).
    #[default]
    WaitingFrame,
    /// The picture changed; waiting until it stays still.
    Debouncing,
    Recognizing,
    Translating,
    /// The translation is on screen.
    Showing,
    /// An error; the next attempt is scheduled.
    RetryWait,
    /// Automatic retries stopped (budget spent or rate limit); a manual retry is needed.
    Halted,
}

impl Phase {
    /// Every phase, for tables and tests.
    pub const ALL: [Phase; 7] = [Self::WaitingFrame, Self::Debouncing, Self::Recognizing, Self::Translating, Self::Showing, Self::RetryWait, Self::Halted];

    /// The state machine of a region. A cycle goes
    /// `WaitingFrame → Debouncing → Recognizing → Translating → Showing`, and from `Showing` back to
    /// waiting or to the next change. Reading and translating cannot be skipped into: nothing goes
    /// to `Translating` except from `Recognizing`, and `Recognizing` is entered from a quiet or a
    /// stable state, never from a failed one except by a retry. Any phase can fail into
    /// `RetryWait` (a delay is scheduled) or `Halted` (retries are over, a manual retry is needed);
    /// those two lead back only through `Recognizing` (the retry) or by the picture settling.
    pub fn allows(self, to: Phase) -> bool {
        use Phase::*;
        if self == to || matches!(to, RetryWait | Halted) { return true; }
        match self {
            WaitingFrame => matches!(to, Debouncing | Recognizing | Showing),
            Debouncing => matches!(to, WaitingFrame | Recognizing | Showing),
            Recognizing => matches!(to, Translating | Showing | WaitingFrame),
            Translating => matches!(to, Showing | WaitingFrame),
            Showing => matches!(to, WaitingFrame | Debouncing | Recognizing),
            RetryWait => matches!(to, WaitingFrame | Debouncing | Recognizing | Showing),
            // Only a manual retry leaves it, and that reads at once (no waiting for the picture to settle).
            Halted => matches!(to, WaitingFrame | Recognizing | Showing),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::WaitingFrame => "ожидание кадра",
            Self::Debouncing => "ждёт, пока текст остановится",
            Self::Recognizing => "распознавание",
            Self::Translating => "перевод",
            Self::Showing => "перевод показан",
            Self::RetryWait => "повтор после ошибки",
            Self::Halted => "автоповтор остановлен",
        }
    }
}

#[derive(Default)]
pub(super) struct RegionState {
    pub(super) detector: ChangeDetector,
    pub(super) last_text: String,
    pub(super) dirty_since: Option<Instant>,
    /// Первое изменение после последнего OCR.
    pub(super) dirty_first: Option<Instant>,
    /// This tick is read although the frame never settled (`MAX_UNSTABLE` passed): the fields whose text is still
    /// being typed out are then held back by the layout engine.
    pub(super) unsettled: bool,
    pub(super) last_attempt: Option<Instant>,
    pub(super) retry_at: Option<Instant>,
    pub(super) first_error: Option<Instant>,
    pub(super) failures: u32,
    pub(super) halted: bool,
    pub(super) reason: String,
    /// Движок «поверх оригинала»: поля, их шрифты и фон живут, пока жива область.
    pub(super) inplace: Option<InplaceEngine>,
    pub(super) phase: Phase,
    /// Transitions the state machine does not allow (always 0 unless the pipeline has a bug).
    pub(super) illegal: u32,
    /// The OCR lines showed fields glued together: read the region again at once (without waiting for the
    /// picture to change), now that the detector divides the block. Consumed by the next tick.
    pub(super) rescan: bool,
    /// `auto` engine: fields for which Tesseract was not enough read straight with PaddleOCR from
    /// then on (the second engine costs time only once, not on every change of the text).
    pub(super) field_engine: HashMap<u64, &'static str>,
    /// Inspector: what the engine said about each field the last time it was read.
    pub(super) field_ocr: HashMap<u64, String>,
    /// How many OCR runs in a row were dropped because the picture changed under them.
    pub(super) stale: u8,
    /// Confidence the text of each field was accepted with: a solid earlier reading is not replaced by a weak
    /// look-alike flip (`ocr::lookalike`).
    pub(super) field_confidence: HashMap<u64, f32>,
    /// Inspector: the lines of each field's last reading and the corner of the crop they are relative to.
    pub(super) field_lines: HashMap<u64, ((u32, u32), Vec<crate::ocr::OcrLine>)>,
    /// Inspector: fields whose text was ignored as unsure, with when; shown for a few seconds.
    pub(super) ignored: HashMap<u64, (Instant, PreviewBox)>,
    /// Inspector: boxes of the last frame sent to the screen.
    pub(super) preview_boxes: Vec<PreviewBox>,
    /// In-place mode: how far above the drawn region the capture starts (a fraction of the window height), because
    /// its top edge cut a line of text (`appearance.inplace.extend_up`). Kept while the region stays the same.
    pub(super) reach_up: f64,
    /// The drawn region `reach_up` was found for; another region starts again from 0.
    pub(super) reach_for: Option<NormRect>,
    /// What the frame the layout engine holds was captured from, if not the drawn region: every frame of fields sent
    /// to the overlay carries it, also one rebuilt without a new capture.
    pub(super) engine_capture: Option<NormRect>,
}

/// The farthest the capture of a region is extended upwards, a fraction of the window height.
pub(super) const MAX_REACH_UP: f64 = 0.12;

/// `rect` started `reach` higher (a fraction of the window), not above the window.
fn extended_up(rect: NormRect, reach: f64) -> NormRect {
    let top = (rect.y - reach).max(0.0);
    NormRect { x: rect.x, y: top, w: rect.w, h: rect.h + (rect.y - top) }
}

impl RegionState {
    /// The region as captured this tick: the drawn one, or started higher for text its top edge cut (in-place mode).
    pub(super) fn captured(&mut self, s: &Settings, region: &CaptureRegion, inplace: bool) -> Option<CaptureRegion> {
        if self.reach_for != region.rect { self.reach_up = 0.0; self.reach_for = region.rect; }
        if !inplace || !s.appearance.inplace.extend_up || self.reach_up <= 0.0 { return None; }
        let rect = extended_up(region.rect?, self.reach_up);
        Some(CaptureRegion { rect: Some(rect), ..region.clone() })
    }
}

impl RegionState {
    pub(super) fn enter(&mut self, region_id: &str, phase: Phase) {
        if self.phase == phase { return; }
        if self.phase.allows(phase) {
            tracing::debug!(target: "pipeline.phase", region = region_id, from = ?self.phase, to = ?phase, "region phase");
        } else {
            // A transition outside the table is a pipeline bug. Keep the last valid phase so the
            // state machine remains trustworthy, and expose the violation through logs/inspector.
            self.illegal += 1;
            tracing::error!(target: "pipeline.phase", region = region_id, from = ?self.phase, to = ?phase, "illegal region phase transition");
            return;
        }
        self.phase = phase;
    }
    pub(super) fn fail(&mut self, region: &CaptureRegion, reason: String, now: Instant, timeout: bool, out: &dyn Sink) {
        let first = *self.first_error.get_or_insert(now);
        self.failures += 1;
        self.reason = reason;
        self.halted = timeout || now.saturating_duration_since(first) >= ERROR_BUDGET;
        let delay = Duration::from_secs((1u64 << self.failures.min(5)).min(30));
        self.retry_at = (!self.halted).then_some((now + delay).min(first + ERROR_BUDGET));
        self.enter(&region.id, if self.halted { Phase::Halted } else { Phase::RetryWait });
        let message = if self.halted {
            format!("{} ({}). Автоповтор остановлен — нажмите «Повторить»", self.reason, region.name)
        } else { format!("{} ({}). Повтор через {} с", self.reason, region.name, delay.as_secs()) };
        // Log before the channel/Qt queue: the error survives a closed or unavailable GUI.
        tracing::error!(region = %region.id, terminal = self.halted, "{message}");
        out.send(Event::Error { region_id: region.id.clone(), message, terminal: self.halted });
    }
    pub(super) fn settle(&mut self, region: &CaptureRegion, out: &dyn Sink) {
        if self.failures > 0 { out.send(Event::Cleared { region_id: region.id.clone() }); }
        self.recovered();
    }
    pub(super) fn recovered(&mut self) { self.failures = 0; self.first_error = None; self.retry_at = None; self.halted = false; self.reason.clear(); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_machine_allows_exactly_the_documented_transitions() {
        use Phase::*;
        let allowed: &[(Phase, Phase)] = &[
            (WaitingFrame, Debouncing), (WaitingFrame, Recognizing), (WaitingFrame, Showing),
            (Debouncing, WaitingFrame), (Debouncing, Recognizing), (Debouncing, Showing),
            (Recognizing, Translating), (Recognizing, Showing), (Recognizing, WaitingFrame),
            (Translating, Showing), (Translating, WaitingFrame),
            (Showing, WaitingFrame), (Showing, Debouncing), (Showing, Recognizing),
            (RetryWait, WaitingFrame), (RetryWait, Debouncing), (RetryWait, Recognizing), (RetryWait, Showing),
            (Halted, WaitingFrame), (Halted, Recognizing), (Halted, Showing),
        ];
        for from in Phase::ALL {
            for to in Phase::ALL {
                let expected = from == to || matches!(to, RetryWait | Halted) || allowed.contains(&(from, to));
                assert_eq!(from.allows(to), expected, "{from:?} -> {to:?}");
            }
        }
        // The ones that matter most: nothing skips reading, and translating is never entered cold.
        assert!(!WaitingFrame.allows(Translating) && !Showing.allows(Translating) && !Halted.allows(Translating) && !RetryWait.allows(Translating));
        assert!(!Halted.allows(Debouncing), "a halted region leaves only through a manual retry");
    }

    #[test]
    fn an_illegal_transition_is_counted_and_rejected() {
        let mut state = RegionState::default();
        state.enter("r", Phase::Translating);
        assert_eq!((state.illegal, state.phase), (1, Phase::WaitingFrame));
        state.enter("r", Phase::Recognizing);
        state.enter("r", Phase::Showing);
        assert_eq!(state.illegal, 1);
        assert_eq!(state.phase, Phase::Showing);
    }

    #[test]
    fn the_extended_capture_never_leaves_the_window() {
        let r = extended_up(NormRect { x: 0.1, y: 0.05, w: 0.5, h: 0.3 }, 0.12);
        assert_eq!((r.x, r.y, r.w), (0.1, 0.0, 0.5));
        assert!((r.h - 0.35).abs() < 1e-12, "{r:?}");
    }
}
