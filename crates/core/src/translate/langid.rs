//! A cheap guess of the language of a text from its script, for a source language set to `auto`.
//!
//! Only languages that a script identifies are answered: Japanese (kana), Korean (hangul), Chinese (han without kana),
//! Ukrainian and Russian (Cyrillic). Latin text, which is shared by dozens of languages, gives `None`: guessing between
//! English, German or French without a statistical detector would be wrong too often, and a wrong source language
//! gives a worse translation than a clear "select the language" message.

#[derive(Default, Debug, PartialEq, Eq)]
struct Counts {
    kana: usize,
    han: usize,
    hangul: usize,
    cyrillic: usize,
    ukrainian: usize,
    latin: usize,
    other: usize,
}

fn count(text: &str) -> Counts {
    let mut c = Counts::default();
    for ch in text.chars().filter(|ch| ch.is_alphabetic()) {
        match ch {
            '\u{3040}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}' | '\u{FF66}'..='\u{FF9F}' => c.kana += 1,
            '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{F900}'..='\u{FAFF}' | '\u{20000}'..='\u{2FA1F}' => c.han += 1,
            '\u{AC00}'..='\u{D7AF}' | '\u{1100}'..='\u{11FF}' | '\u{3130}'..='\u{318F}' => c.hangul += 1,
            // Letters that Russian does not have.
            'і' | 'ї' | 'є' | 'ґ' | 'І' | 'Ї' | 'Є' | 'Ґ' => { c.cyrillic += 1; c.ukrainian += 1 }
            '\u{0400}'..='\u{04FF}' => c.cyrillic += 1,
            ch if ch.is_ascii_alphabetic() || ('\u{00C0}'..='\u{024F}').contains(&ch) => c.latin += 1,
            _ => c.other += 1,
        }
    }
    c
}

/// The ISO code of the language the script of `text` points to, or `None` when the script is Latin, mixed without a
/// clear majority, or the text has no letters.
pub fn detect(text: &str) -> Option<&'static str> {
    let c = count(text);
    let total = c.kana + c.han + c.hangul + c.cyrillic + c.latin + c.other;
    if total == 0 { return None; }
    let majority = |n: usize| n * 2 > total;
    // Kana together with han is Japanese; han alone is Chinese. A game line may carry a few Latin letters or digits.
    if c.kana > 0 && majority(c.kana + c.han) { return Some("ja"); }
    if majority(c.han) { return Some("zh"); }
    if majority(c.hangul) { return Some("ko"); }
    if majority(c.cyrillic) { return Some(if c.ukrainian > 0 { "uk" } else { "ru" }); }
    None
}

#[cfg(test)]
mod tests {
    use super::detect;

    #[test]
    fn scripts_that_identify_a_language() {
        assert_eq!(detect("こんにちは、世界"), Some("ja"));
        assert_eq!(detect("勇者は剣を取った"), Some("ja"));
        assert_eq!(detect("你好，世界"), Some("zh"));
        assert_eq!(detect("안녕하세요"), Some("ko"));
        assert_eq!(detect("Привет, мир"), Some("ru"));
        assert_eq!(detect("Привіт, світе"), Some("uk"));
        assert_eq!(detect("Їжак"), Some("uk"));
    }

    #[test]
    fn latin_text_is_not_guessed() {
        assert_eq!(detect("Hello, world"), None);
        assert_eq!(detect("Schönes Wetter heute"), None);
    }

    #[test]
    fn digits_punctuation_and_empty_text_say_nothing() {
        assert_eq!(detect(""), None);
        assert_eq!(detect("1234 !?…"), None);
    }

    #[test]
    fn a_few_foreign_letters_do_not_change_the_answer() {
        assert_eq!(detect("HP: 100 勇者は剣を取った"), Some("ja"));
        assert_eq!(detect("Нажмите OK"), Some("ru"));
    }

    #[test]
    fn mixed_text_without_a_majority_is_not_guessed() {
        assert_eq!(detect("Press ВВОД to continue to the next level"), None);
    }
}
