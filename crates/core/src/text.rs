//! Нормализация OCR-текста и нечёткое сравнение для отсечения шума распознавания.

pub fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Похожесть строк 0.0..=1.0 по Левенштейну (по символам).
pub fn similarity(a: &str, b: &str) -> f32 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let max = a.len().max(b.len());
    if max == 0 {
        return 1.0;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        prev = cur;
    }
    1.0 - prev[b.len()] as f32 / max as f32
}

/// Сходство Джаро 0.0..=1.0 (по символам).
pub fn jaro(a: &str, b: &str) -> f32 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() && b.is_empty() { return 1.0; }
    if a.is_empty() || b.is_empty() { return 0.0; }
    let window = (a.len().max(b.len()) / 2).saturating_sub(1);
    let mut taken = vec![false; b.len()];
    let mut from_a = Vec::new();
    for (i, &c) in a.iter().enumerate() {
        let lo = i.saturating_sub(window);
        let hi = (i + window + 1).min(b.len());
        if let Some(j) = (lo..hi).find(|&j| !taken[j] && b[j] == c) { taken[j] = true; from_a.push(c); }
    }
    if from_a.is_empty() { return 0.0; }
    let from_b = b.iter().zip(&taken).filter(|(_, t)| **t).map(|(c, _)| *c);
    let transposed = from_a.iter().zip(from_b).filter(|(x, y)| *x != y).count() as f32 / 2.0;
    let m = from_a.len() as f32;
    (m / a.len() as f32 + m / b.len() as f32 + (m - transposed) / m) / 3.0
}

/// Коэффициент Дайса по биграммам символов 0.0..=1.0.
pub fn dice(a: &str, b: &str) -> f32 {
    let bigrams = |s: &str| -> Vec<(char, char)> { let c: Vec<char> = s.chars().collect(); c.windows(2).map(|w| (w[0], w[1])).collect() };
    let (x, mut y) = (bigrams(a), bigrams(b));
    if x.is_empty() || y.is_empty() { return if a == b { 1.0 } else { 0.0 }; }
    let total = (x.len() + y.len()) as f32;
    let mut shared = 0;
    for g in &x { if let Some(i) = y.iter().position(|h| h == g) { y.swap_remove(i); shared += 1; } }
    2.0 * shared as f32 / total
}

/// Два чтения — одна и та же реплика: Джаро и Дайс оба не ниже порога (регистр и пробелы не в счёт).
/// Так небольшие расхождения OCR не переводят строку заново.
pub fn same_reading(a: &str, b: &str, threshold: f32) -> bool {
    let (a, b) = (normalize(a).to_lowercase(), normalize(b).to_lowercase());
    jaro(&a, &b) >= threshold && dice(&a, &b) >= threshold
}

/// Мусорный результат OCR: пусто или нет ни одной буквы/цифры.
pub fn is_meaningful(s: &str) -> bool {
    s.chars().any(|c| c.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_collapses_whitespace() {
        assert_eq!(normalize("  Where  are\nyou \t going? "), "Where are you going?");
    }

    #[test]
    fn similarity_basic() {
        assert_eq!(similarity("abc", "abc"), 1.0);
        assert!(similarity("Where are you going?", "Where are you goinq?") > 0.9);
        assert!(similarity("Hello", "Goodbye") < 0.5);
        assert_eq!(similarity("", ""), 1.0);
    }

    #[test]
    fn jaro_and_dice() {
        assert_eq!(jaro("abc", "abc"), 1.0);
        assert!((jaro("MARTHA", "MARHTA") - 0.944).abs() < 0.01);
        assert_eq!(jaro("abc", "xyz"), 0.0);
        assert_eq!(dice("night", "nacht"), 0.25);
        assert_eq!(dice("same", "same"), 1.0);
    }

    #[test]
    fn same_reading_tolerates_small_ocr_noise() {
        assert!(same_reading("Where are you going?", "Where are you goinq?", 0.85));
        assert!(same_reading("Where  are you going?", "where are you going?", 0.99));
        assert!(!same_reading("Where are you going?", "I will be back tomorrow.", 0.85));
    }

    #[test]
    fn meaningful() {
        assert!(!is_meaningful(" |-- "));
        assert!(is_meaningful("A1"));
    }
}
