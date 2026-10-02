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
    fn meaningful() {
        assert!(!is_meaningful(" |-- "));
        assert!(is_meaningful("A1"));
    }
}
