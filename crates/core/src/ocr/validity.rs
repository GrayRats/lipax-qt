//! Правдоподобие распознанного текста: число 0.0..=1.0 для данного языка.
//!
//! Аналог «ML-оценки правдоподобия» из Translumo (там — модель ML.NET, обученная отдельно под язык; её данных нет, а
//! формат закрыт). Здесь признаки те же по смыслу — язык, символы, длина — но модель прозрачная: взвешенная сумма
//! нескольких признаков без внешних данных. Число нужно только для сравнения чтений одного кадра между движками.

use std::collections::HashSet;

/// Письменности, которые ожидаются для языка распознавания.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Script { Latin, Cyrillic, Han, Kana, Hangul, Other }

fn script_of(c: char) -> Script {
    match c as u32 {
        0x41..=0x5A | 0x61..=0x7A | 0xC0..=0x24F => Script::Latin,
        0x400..=0x52F => Script::Cyrillic,
        0x4E00..=0x9FFF | 0x3400..=0x4DBF => Script::Han,
        0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9F => Script::Kana,
        0xAC00..=0xD7AF | 0x1100..=0x11FF | 0x3130..=0x318F => Script::Hangul,
        _ => Script::Other,
    }
}

/// Письменности языка (`recognition.language`: коды Tesseract, возможно `jpn+eng`).
fn expected_scripts(language: &str) -> HashSet<Script> {
    let mut set = HashSet::new();
    for part in language.split('+') {
        match part.trim() {
            "rus" | "ukr" | "bel" | "bul" | "srp" => { set.insert(Script::Cyrillic); }
            "jpn" => { set.extend([Script::Han, Script::Kana]); }
            "chi_sim" | "chi_tra" => { set.insert(Script::Han); }
            "kor" => { set.extend([Script::Hangul, Script::Han]); }
            _ => { set.insert(Script::Latin); }
        }
    }
    if set.is_empty() { set.insert(Script::Latin); }
    set
}

fn is_cjk(s: Script) -> bool { matches!(s, Script::Han | Script::Kana | Script::Hangul) }

fn is_vowel(c: char) -> bool {
    matches!(c.to_lowercase().next(), Some('a' | 'e' | 'i' | 'o' | 'u' | 'y' | 'а' | 'е' | 'ё' | 'и' | 'о' | 'у' | 'ы' | 'э' | 'ю' | 'я'))
}

/// Односимвольные слова, которые бывают настоящими.
fn is_real_single(c: char) -> bool {
    matches!(c.to_lowercase().next(), Some('a' | 'i' | 'o' | 'в' | 'к' | 'с' | 'о' | 'у' | 'я' | 'и' | 'а'))
}

/// Слово без обрамляющей пунктуации выглядит словом выбранного языка.
fn wordlike(token: &str, expected: &HashSet<Script>) -> bool {
    let core: Vec<char> = token.trim_matches(|c: char| !c.is_alphanumeric()).chars().collect();
    if core.is_empty() { return false; }
    let letters: Vec<char> = core.iter().copied().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() { return core.iter().all(|c| c.is_ascii_digit() || matches!(c, ',' | '.' | ':' | '/' | '%')); }
    // Цифры внутри слова (`l0ve`, `He11o`) — типичная подмена похожих знаков.
    if core.iter().any(|c| c.is_ascii_digit()) { return false; }
    let scripts: HashSet<Script> = letters.iter().map(|&c| script_of(c)).collect();
    // Смесь латиницы и кириллицы в одном слове — подмена `о`/`o`, `а`/`a`.
    if scripts.contains(&Script::Latin) && scripts.contains(&Script::Cyrillic) { return false; }
    if scripts.iter().any(|s| !expected.contains(s) && !matches!(s, Script::Latin)) { return false; }
    if letters.iter().all(|&c| is_cjk(script_of(c))) { return true; }
    if letters.len() == 1 { return is_real_single(letters[0]); }
    // Длинное слово без гласных и три одинаковые буквы подряд — не слово.
    if letters.len() >= 3 && !letters.iter().any(|&c| is_vowel(c)) { return false; }
    !letters.windows(3).any(|w| w[0] == w[1] && w[1] == w[2])
}

/// Оценка правдоподобия `text` для `language`: 0.0 — мусор, 1.0 — уверенная речь.
pub fn score(text: &str, language: &str) -> f32 {
    let expected = expected_scripts(language);
    let visible: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    if visible.is_empty() { return 0.0; }
    let letters: Vec<char> = visible.iter().copied().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() { return 0.0; }
    let total = visible.len() as f32;

    let letter_ratio = letters.len() as f32 / total;
    // Буквы письменности языка. Латиница допустима всегда: имена, названия, `jpn+eng`.
    let in_script = letters.iter().filter(|&&c| { let s = script_of(c); expected.contains(&s) || s == Script::Latin }).count() as f32 / letters.len() as f32;
    let native = letters.iter().filter(|&&c| expected.contains(&script_of(c))).count() as f32 / letters.len() as f32;
    let script = 0.5 * in_script + 0.5 * native.max(in_script * 0.5);

    let cjk = letters.iter().filter(|&&c| is_cjk(script_of(c))).count() * 2 > letters.len();
    let tokens: Vec<&str> = if cjk { text.split_whitespace().collect() } else { text.split_whitespace().filter(|t| t.chars().any(char::is_alphanumeric) || t.len() > 1).collect() };
    let words = if tokens.is_empty() { 0.0 } else { tokens.iter().filter(|t| wordlike(t, &expected)).count() as f32 / tokens.len() as f32 };

    // Одинокие знаки, которыми OCR «читает» рамки и края: | _ ~ ^ \ ` § ¦ и т. п.
    let stray = visible.iter().filter(|c| matches!(c, '|' | '_' | '~' | '^' | '\\' | '`' | '§' | '¦' | '¬' | '■' | '□' | '�')).count() as f32;
    let clean = (1.0 - 4.0 * stray / total).clamp(0.0, 1.0);

    // Совсем короткое чтение мало что говорит.
    let length = match letters.len() { 1 => 0.45, 2 => 0.7, 3 => 0.85, _ => 1.0 };

    ((0.3 * script + 0.4 * words + 0.15 * letter_ratio + 0.15 * clean) * length).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_speech_scores_high() {
        assert!(score("Where are you going, Captain?", "eng") > 0.85);
        assert!(score("Куда ты идёшь, капитан?", "rus") > 0.85);
        assert!(score("どこへ行くのですか、隊長?", "jpn") > 0.85);
    }

    #[test]
    fn garbage_scores_low() {
        assert!(score("|_ ~~ \\ |", "eng") < 0.1);
        assert!(score("", "eng") == 0.0);
        assert!(score("Wh3re 4re y0u g0ing", "eng") < score("Where are you going", "eng") - 0.3);
    }

    #[test]
    fn mixed_script_words_are_penalised() {
        // Латинские `o`, `a`, `p` внутри русских слов.
        assert!(score("Куда ты идёшь", "rus") > score("Кyдa тw идёшь", "rus") + 0.2);
    }

    #[test]
    fn wrong_language_scores_lower() {
        assert!(score("Куда ты идёшь, капитан?", "rus") > score("Куда ты идёшь, капитан?", "eng") + 0.2);
    }

    #[test]
    fn consonant_soup_is_not_a_word() {
        assert!(score("Where are you going", "eng") > score("Wxrk rtl ppqw gnng", "eng") + 0.3);
    }
}
