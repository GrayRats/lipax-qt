//! Look-alike flips of a short label.
//!
//! A HUD value or a button caption is read again every time its picture changes, and an engine may swap
//! one glyph for a similar one from one reading to the next (`15` → `is`, `0` → `O`). Each flip would be
//! translated and drawn again. The rule is Kizurium's (`live/reconcile.py`, `resolve_short_element`): a
//! reading that differs from the last one only at positions where the glyphs are easy to confuse does not
//! replace a solid earlier reading unless it is solid itself. The earlier reading never wins alone: any
//! other kind of difference (a letter that has nothing to do with the old one, a different length) is a real
//! change and passes at once.
//!
//! This is not a replacement table: `0` is never turned into `O` here, a reading is only kept or not.

/// Longest label the rule applies to: longer text has too many words for a single flip to matter.
const SHORT: usize = 14;
/// A reading at or above this is solid, as is the earlier reading that is protected from a weak one.
const SOLID: f32 = 80.0;

/// Glyphs that engines take for one another (either direction).
const PAIRS: &[(char, char)] = &[
    ('i', '1'), ('l', '1'), ('I', '1'), ('l', '|'), ('I', 'l'), ('I', '|'), ('o', '0'), ('O', '0'),
    ('s', '5'), ('S', '5'), ('B', '8'), ('b', '6'), ('G', '6'), ('g', '9'), ('q', '9'), ('z', '2'), ('Z', '2'),
];

fn confusable(a: char, b: char) -> bool {
    PAIRS.iter().any(|&(x, y)| (a == x && b == y) || (a == y && b == x))
}

/// The two texts have the same length, differ somewhere, and every difference is a pair of look-alikes.
pub fn differs_only_by_lookalikes(a: &str, b: &str) -> bool {
    let (mut a, mut b) = (a.chars(), b.chars());
    let mut differs = false;
    loop {
        match (a.next(), b.next()) {
            (None, None) => return differs,
            (Some(x), Some(y)) if x == y => {}
            (Some(x), Some(y)) if confusable(x, y) => differs = true,
            _ => return false,
        }
    }
}

/// `true`: keep the earlier reading and ignore the new one.
///
/// `before` is the text and the confidence the field was accepted with, `now` the new reading. Without a
/// confidence on either side there is nothing to weigh (an engine that does not report one is trusted), and
/// a reading that is solid itself replaces the earlier one.
pub fn keeps_earlier(before: &str, before_confidence: Option<f32>, now: &str, now_confidence: Option<f32>) -> bool {
    if before.chars().count() > SHORT || before.is_empty() || before == now { return false; }
    let (Some(earlier), Some(fresh)) = (before_confidence, now_confidence) else { return false };
    earlier >= SOLID && fresh < SOLID && differs_only_by_lookalikes(before, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_lookalike_positions_count() {
        assert!(differs_only_by_lookalikes("15", "l5"));
        assert!(differs_only_by_lookalikes("15", "I5"));
        assert!(differs_only_by_lookalikes("Lv. 05", "Lv. O5"));
        assert!(differs_only_by_lookalikes("is", "15"), "both letters are look-alikes of digits");
        assert!(!differs_only_by_lookalikes("15", "15"), "no difference is not a flip");
        assert!(!differs_only_by_lookalikes("15", "16"));
        assert!(!differs_only_by_lookalikes("15", "155"), "another length is a real change");
        assert!(!differs_only_by_lookalikes("Start", "Stop"));
        assert!(!differs_only_by_lookalikes("is", "15x"));
    }

    #[test]
    fn a_weak_flip_does_not_replace_a_solid_reading() {
        assert!(keeps_earlier("15", Some(92.0), "is", Some(61.0)));
        assert!(keeps_earlier("HP 100", Some(88.0), "HP 1O0", Some(70.0)));
    }

    #[test]
    fn the_earlier_reading_never_wins_alone() {
        // The new reading is solid: it replaces the old one.
        assert!(!keeps_earlier("15", Some(92.0), "is", Some(90.0)));
        // The old reading was not solid: it protects nothing.
        assert!(!keeps_earlier("l5", Some(60.0), "15", Some(70.0)));
        // A real change passes whatever the confidence.
        assert!(!keeps_earlier("15", Some(92.0), "16", Some(30.0)));
        assert!(!keeps_earlier("Start", Some(92.0), "Stop", Some(30.0)));
        // No confidence on either side: nothing to weigh.
        assert!(!keeps_earlier("15", None, "is", Some(30.0)));
        assert!(!keeps_earlier("15", Some(92.0), "is", None));
        // Long text, empty text, identical text.
        assert!(!keeps_earlier(&"1l".repeat(10), Some(92.0), &"ll".repeat(10), Some(30.0)));
        assert!(!keeps_earlier("", Some(92.0), "1", Some(30.0)));
        assert!(!keeps_earlier("15", Some(92.0), "15", Some(30.0)));
    }
}
