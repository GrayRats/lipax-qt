//! Выбор лучшего чтения среди нескольких движков, прочитавших один и тот же кадр.
//!
//! По схеме Translumo (`GetBestDetectionResult` и `TextDetectionResult.CompareTo`): если текст совпал у `QUORUM`
//! движков, он побеждает; иначе выигрывает чтение с лучшей оценкой правдоподобия, при равных — с большей
//! уверенностью движка. Заметно более длинное чтение побеждает, если само по себе правдоподобно.

use super::{OcrResult, validity};

/// Столько движков с одним и тем же текстом решают спор без оценок.
pub const QUORUM: usize = 3;
/// Прибавка к оценке за каждое другое чтение с тем же текстом (при двух движках кворума не бывает).
const AGREEMENT_BONUS: f32 = 0.08;
/// Чтения ближе этого значения считаются равными по оценке.
const SCORE_TIE: f32 = 0.01;
/// Во сколько раз чтение должно быть длиннее, чтобы выиграть длиной.
const LONG_FACTOR: usize = 2;
/// Оценка, ниже которой длина не выручает.
const LONG_MIN_SCORE: f32 = 0.7;

/// Чтение одного движка вместе с оценкой.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub result: OcrResult,
    pub score: f32,
    /// Текст без регистра, пробелов и знаков: по нему сравнивают чтения на совпадение.
    key: String,
}

impl Candidate {
    pub fn new(result: OcrResult, language: &str) -> Self {
        let score = validity::score(&result.text, language);
        let key = result.text.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect();
        Self { result, score, key }
    }

    fn len(&self) -> usize { self.key.chars().count() }
}

/// Лучшее чтение. `None`, если все пусты.
pub fn select(results: Vec<OcrResult>, language: &str) -> Option<OcrResult> {
    let mut candidates: Vec<Candidate> = results.into_iter().filter(|r| !r.text.trim().is_empty()).map(|r| Candidate::new(r, language)).collect();
    if candidates.is_empty() { return None; }
    let agree = |c: &Candidate, all: &[Candidate]| all.iter().filter(|o| !o.key.is_empty() && o.key == c.key).count();
    if let Some(i) = (0..candidates.len()).find(|&i| agree(&candidates[i], &candidates) >= QUORUM) {
        tracing::debug!(target: "ocr.consensus", engine = candidates[i].result.engine, "consensus: the quorum agrees");
        return Some(candidates.swap_remove(i).result);
    }
    let votes: Vec<usize> = candidates.iter().map(|c| agree(c, &candidates)).collect();
    for (c, v) in candidates.iter_mut().zip(votes) { c.score += AGREEMENT_BONUS * (v - 1) as f32; }
    let mut best = 0;
    for i in 1..candidates.len() {
        if beats(&candidates[i], &candidates[best]) { best = i; }
    }
    tracing::debug!(target: "ocr.consensus", chosen = candidates[best].result.engine, scores = ?candidates.iter().map(|c| (c.result.engine, c.score)).collect::<Vec<_>>(), "consensus: the best score");
    Some(candidates.swap_remove(best).result)
}

/// Больше ли `a`, чем `b`.
fn beats(a: &Candidate, b: &Candidate) -> bool {
    if a.len() >= b.len() * LONG_FACTOR && a.score >= LONG_MIN_SCORE { return true; }
    if b.len() >= a.len() * LONG_FACTOR && b.score >= LONG_MIN_SCORE { return false; }
    if (a.score - b.score).abs() > SCORE_TIE { return a.score > b.score; }
    a.result.confidence.unwrap_or(0.0) > b.result.confidence.unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(text: &str, engine: &'static str, confidence: Option<f32>) -> OcrResult { OcrResult { text: text.into(), engine, confidence, ..OcrResult::default() } }

    #[test]
    fn nothing_when_all_are_empty() {
        assert!(select(vec![reading("  ", "a", None), reading("", "b", None)], "eng").is_none());
    }

    #[test]
    fn better_score_wins_over_confidence() {
        let picked = select(vec![reading("Wh3re 4re y0u g0ing", "tesseract", Some(95.0)), reading("Where are you going", "rapidocr", Some(70.0))], "eng").unwrap();
        assert_eq!(picked.engine, "rapidocr");
    }

    #[test]
    fn quorum_beats_a_higher_score() {
        let picked = select(vec![
            reading("Where are you going", "a", None), reading("where are you going!", "b", None), reading("Where are you going", "c", None),
            reading("Where are you going now friend", "d", None),
        ], "eng").unwrap();
        // Три совпали; четвёртое чтение, возможно, лучше, но кворум решает.
        assert_eq!(picked.engine, "a");
    }

    #[test]
    fn tie_goes_to_the_more_confident_engine() {
        let picked = select(vec![reading("Hello there friend", "a", Some(50.0)), reading("Hello there friend!", "b", Some(80.0))], "eng").unwrap();
        assert_eq!(picked.engine, "b");
    }

    #[test]
    fn longer_plausible_reading_wins() {
        // Один движок прочёл только начало строки.
        let picked = select(vec![reading("Where are", "a", Some(90.0)), reading("Where are you going, Captain?", "b", Some(60.0))], "eng").unwrap();
        assert_eq!(picked.engine, "b");
    }

    #[test]
    fn single_reading_is_returned_as_is() {
        assert_eq!(select(vec![reading("Hi", "a", None)], "eng").unwrap().engine, "a");
    }
}
