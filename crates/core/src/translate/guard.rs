//! Структурная проверка ответа переводчика: пустой ответ, эхо исходника, потерянный хвост, потерянные числа.
//! Только форма текста, без словарей. Все пороги эмпирические: подобраны на длинных репликах и абзацах
//! и проверяются таблицей тестов ниже; менять их нужно вместе с тестами.

use std::collections::HashMap;
use std::fmt;

/// Источник короче этого (в символах) не считается длинным: правила потери хвоста к нему не применяются.
/// Эмпирическое значение.
const LONG_SOURCE_CHARS: usize = 60;
/// Перевод короче этой доли исходника (в символах) для длинного текста считается обрезанным. Эмпирическое значение.
const MIN_LENGTH_RATIO: f64 = 0.42;
/// Missing sentence ends count as a lost tail only if the translation is also shorter than this share of the source.
/// Эмпирическое значение.
const TAIL_LENGTH_RATIO: f64 = 0.8;
/// Для источников не длиннее этого эхо не проверяется: имена, междометия, метки совпадают с переводом законно.
/// Эмпирическое значение.
const SHORT_SOURCE_CHARS: usize = 3;
/// Доля CJK-букв, начиная с которой текст считается «CJK» (там нет пробелов и отношение длин иное). Эмпирическое значение.
const CJK_SHARE: f64 = 0.5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    Empty,
    Echo,
    LostTail,
    LostNumber(String),
}

impl fmt::Display for Rejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("пустой перевод"),
            Self::Echo => f.write_str("перевод совпадает с исходным текстом"),
            Self::LostTail => f.write_str("потеряна часть текста"),
            Self::LostNumber(n) => write!(f, "потеряно число {n}"),
        }
    }
}

impl std::error::Error for Rejection {}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF   // хирагана, катакана
        | 0x31F0..=0x31FF
        | 0x3400..=0x4DBF // иероглифы
        | 0x4E00..=0x9FFF
        | 0xF900..=0xFAFF
        | 0xAC00..=0xD7AF // хангыль
        | 0x1100..=0x11FF
        | 0x20000..=0x2FFFF)
}

fn mostly_cjk(text: &str) -> bool {
    let (mut letters, mut cjk) = (0usize, 0usize);
    for c in text.chars().filter(|c| c.is_alphabetic()) {
        letters += 1;
        cjk += usize::from(is_cjk(c));
    }
    letters > 0 && cjk as f64 / letters as f64 >= CJK_SHARE
}

/// Нижний регистр, только буквы и цифры: пробелы, пунктуация и регистр не учитываются.
fn normalised(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Концы предложений: `.!?…` считаются, если за ними пробел, закрывающая кавычка или конец текста
/// (чтобы `3.14` и `a.b` не считались), а CJK-терминаторы считаются всегда.
fn terminators(text: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut count = 0;
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '。' | '！' | '？' | '…' | '‥' => count += 1,
            '.' | '!' | '?' if !matches!(chars.get(i + 1), Some('.' | '!' | '?')) => {
                let next = chars.get(i + 1);
                // Серия «...» или «?!» — один конец: считается только её последний знак.
                if next.is_none_or(|n| n.is_whitespace() || matches!(n, '"' | '\'' | '»' | '”' | '’' | ')' | '」' | '』')) {
                    count += 1;
                }
            }
            _ => {}
        }
    }
    count
}

fn digit_runs(text: &str) -> HashMap<String, usize> {
    let mut runs = HashMap::new();
    let mut current = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() {
            current.push(c);
        } else if !current.is_empty() {
            *runs.entry(std::mem::take(&mut current)).or_insert(0) += 1;
        }
    }
    runs
}

/// Принимает ли пайплайн `translated` как перевод `source`.
pub fn check(source: &str, translated: &str, src_lang: &str, dst_lang: &str) -> Result<(), Rejection> {
    let translated_trim = translated.trim();
    if translated_trim.is_empty() {
        return Err(Rejection::Empty);
    }
    let source_trim = source.trim();
    let source_chars = source_trim.chars().count();

    let has_letters = source_trim.chars().any(char::is_alphabetic);
    if has_letters
        && source_chars > SHORT_SOURCE_CHARS
        && !src_lang.eq_ignore_ascii_case(dst_lang)
        && normalised(source_trim) == normalised(translated_trim)
    {
        return Err(Rejection::Echo);
    }

    if source_chars >= LONG_SOURCE_CHARS {
        let (src_ends, dst_ends) = (terminators(source_trim), terminators(translated_trim));
        // Fewer sentence ends alone is not a lost tail: OCR noise adds stray `!` and `.`, and the translator may join
        // sentences. A shorter text on top of it is.
        let shorter = (translated_trim.chars().count() as f64) < TAIL_LENGTH_RATIO * source_chars as f64;
        if src_ends >= 2 && dst_ends < src_ends && shorter {
            return Err(Rejection::LostTail);
        }
        if !mostly_cjk(source_trim) && !mostly_cjk(translated_trim)
            && (translated_trim.chars().count() as f64) < MIN_LENGTH_RATIO * source_chars as f64
        {
            return Err(Rejection::LostTail);
        }
    }

    let mut have = digit_runs(translated_trim);
    let mut wanted: Vec<_> = digit_runs(source_trim).into_iter().collect();
    wanted.sort();
    for (run, need) in wanted {
        let got = have.entry(run.clone()).or_insert(0);
        if *got < need {
            return Err(Rejection::LostNumber(run));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LONG: &str = "The old road leads to the village. Nobody has walked it for years. Bring a lantern with you.";

    #[test]
    fn table() {
        use Rejection::*;
        type Case = (&'static str, &'static str, &'static str, &'static str, Result<(), Rejection>);
        let long_ru_ok = "Старая дорога ведёт в деревню. По ней никто не ходил годами. Возьми с собой фонарь.";
        let cases: Vec<Case> = vec![
            // хорошие
            ("Hello", "Привет", "en", "ru", Ok(())),
            ("Hello", "RU:Hello", "en", "ru", Ok(())),
            (LONG, long_ru_ok, "en", "ru", Ok(())),
            ("Lv. 12", "Ур. 12", "en", "ru", Ok(())),
            ("12/24", "12/24", "en", "ru", Ok(())),
            ("1,000 coins", "1 000 монет", "en", "ru", Ok(())),
            ("Take 3 apples", "Возьми 3 яблока", "en", "ru", Ok(())),
            ("Take 3 apples", "Возьми 3 яблока и ещё 5", "en", "ru", Ok(())),
            // пусто
            ("Hello", "", "en", "ru", Err(Empty)),
            ("Hello", " \n\t ", "en", "ru", Err(Empty)),
            // эхо
            ("Where are you going?", "where are you going", "en", "ru", Err(Echo)),
            ("Where are you going?", "  Where  are you going? ", "en", "ru", Err(Echo)),
            ("Where are you going?", "Where are you going?", "en", "en", Ok(())),
            ("Hi", "Hi", "en", "ru", Ok(())),
            ("Bob", "Bob", "en", "ru", Ok(())),
            ("100%", "100%", "en", "ru", Ok(())),
            ("...", "...", "en", "ru", Ok(())),
            ("12:30", "12:30", "en", "ru", Ok(())),
            // потерянный хвост
            (LONG, "Старая дорога ведёт в деревню.", "en", "ru", Err(LostTail)),
            (LONG, "Старая дорога ведёт в деревню. Никто не ходил по ней годами", "en", "ru", Err(LostTail)),
            // OCR-мусор с лишними знаками и перевод покороче по числу знаков конца, но не короче по смыслу: не отказ
            ("looking for!'Sure it might be dangerous, but I will go anyway. Wait!", "Ищу! Конечно, это может быть опасно, но я всё равно пойду. Подожди", "en", "ru", Ok(())),
            (LONG, "Старая дорога", "en", "ru", Err(LostTail)),
            // числа
            ("Lv. 12", "Ур.", "en", "ru", Err(LostNumber("12".into()))),
            ("Take 3 apples", "Возьми яблоки", "en", "ru", Err(LostNumber("3".into()))),
            ("Take 3 and 3 apples", "Возьми 3 яблока", "en", "ru", Err(LostNumber("3".into()))),
            ("HP 120/340", "ОЗ 120/34", "en", "ru", Err(LostNumber("340".into()))),
            // CJK
            ("こんにちは", "Здравствуйте", "ja", "ru", Ok(())),
            ("こんにちは", "こんにちは", "ja", "ru", Err(Echo)),
            ("你好", "你好", "zh", "ru", Ok(())),
            ("今日はとても良い天気ですね。散歩に行きましょう。公園で花を見て、お茶を飲んで、それから家に帰りましょう。そして夜には星を眺めながら、ゆっくりと眠りにつきましょう。", "Сегодня хорошая погода.", "ja", "ru", Err(LostTail)),
            ("今日はとても良い天気ですね。散歩に行きましょう。公園で花を見て、お茶を飲んで、それから家に帰りましょう。そして夜には星を眺めながら、ゆっくりと眠りにつきましょう。", "Сегодня очень хорошая погода. Давай погуляем. Посмотрим на цветы в парке, выпьем чаю и вернёмся домой. Вечером заснём под звёздами.", "ja", "ru", Ok(())),
            (LONG, "老路通向村庄。多年无人走过。带上灯笼。", "en", "zh", Ok(())),
        ];
        for (source, translated, src, dst, want) in cases {
            assert_eq!(check(source, translated, src, dst), want, "{source:?} -> {translated:?}");
        }
    }

    #[test]
    fn dense_cjk_target_is_not_judged_by_length() {
        let source = "This is a fairly long sentence that has no terminator inside it at all so only length counts";
        assert_eq!(check(source, "这是一个没有句号的很长的句子", "en", "zh"), Ok(()));
        assert_eq!(check(source, "Это", "en", "ru"), Err(Rejection::LostTail));
    }

    #[test]
    fn decimal_point_is_not_a_sentence_end() {
        assert_eq!(terminators("Pi is 3.14 roughly."), 1);
        assert_eq!(terminators("Wait... what?!"), 2);
    }

    #[test]
    fn messages_are_russian() {
        assert_eq!(Rejection::Empty.to_string(), "пустой перевод");
        assert_eq!(Rejection::Echo.to_string(), "перевод совпадает с исходным текстом");
        assert_eq!(Rejection::LostTail.to_string(), "потеряна часть текста");
    }
}
