//! Maintainers only (ROADMAP R-01): how much do Tesseract and RapidOCR disagree on the real game frames, and would
//! choosing between them per text pay off? Both read the same full frame; each expected text of
//! `tests/inplace/fixtures/ground-truth.json` (entries with `"unsure"` skipped) is scored with the character error
//! rate against the closest part of what the engine read (case, spacing and punctuation ignored).
//! Reported per set (frames 0–9 working set, 10–15 control set): mean CER of each engine, the CER of an ORACLE that
//! picks the better engine per text (an upper bound for any selection rule), how many texts the engines read
//! differently by more than 0.10, and the time per frame.
//!
//!   LIPAX_RAPID_TEST_MODELS=~/.local/share/LipaX/ocr-models \
//!   cargo run --release -p lipa-core --example engine_disagreement
//! Frames come from `tmp/acc/frames/dialogue-en-N.png` (made by `ocr_accuracy`).
use lipa_core::{ocr::{Ocr, Tesseract, rapid::RapidOcr}, settings::Settings};
use std::time::Instant;

fn norm(s: &str) -> Vec<char> {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Edit distance of `needle` to the closest substring of `hay`, over the length of `needle` (Sellers' algorithm).
fn cer(needle: &[char], hay: &[char]) -> f64 {
    if needle.is_empty() { return 0.0; }
    let mut prev: Vec<usize> = vec![0; hay.len() + 1];
    for (i, &n) in needle.iter().enumerate() {
        let mut cur = vec![i + 1; hay.len() + 1];
        for (j, &h) in hay.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(n != h)).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    *prev.iter().min().unwrap() as f64 / needle.len() as f64
}

#[derive(Default)]
struct Sum { n: usize, tess: f64, rapid: f64, oracle: f64, differ: usize, tess_better: usize, rapid_better: usize, t_ms: f64, r_ms: f64, frames: usize }

#[tokio::main]
async fn main() {
    let truth: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/inplace/fixtures/ground-truth.json")).unwrap()).unwrap();
    let mut s = Settings::default();
    s.recognition.language = "eng".into();
    s.recognition.rapid_threads = 4;
    s.recognition.rapid_use_gpu = false;
    let rapid = RapidOcr::new(lipa_core::ocr::rapid_models::cache_root(), None);
    let mut sets = [Sum::default(), Sum::default()];
    for n in 0..16 {
        let key = format!("dialogue-en-{n}");
        let path = format!("tmp/acc/frames/{key}.png");
        let Ok(img) = image::open(&path) else { eprintln!("skip {key}: {path} missing (run ocr_accuracy once)"); continue };
        let t = Instant::now();
        let tess = Tesseract.recognize(&img, &s).await.unwrap_or_default();
        let t_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let rap = rapid.recognize(&img, &s).await.unwrap_or_else(|e| { eprintln!("rapid {key}: {e}"); String::new() });
        let r_ms = t.elapsed().as_secs_f64() * 1e3;
        let (tn, rn) = (norm(&tess), norm(&rap));
        let sum = &mut sets[usize::from(n >= 10)];
        sum.frames += 1; sum.t_ms += t_ms; sum.r_ms += r_ms;
        for e in truth[&key].as_array().into_iter().flatten().filter(|e| e["unsure"] != true) {
            let want = norm(e["text"].as_str().unwrap_or(""));
            if want.is_empty() { continue; }
            let (ct, cr) = (cer(&want, &tn), cer(&want, &rn));
            sum.n += 1; sum.tess += ct; sum.rapid += cr; sum.oracle += ct.min(cr);
            if (ct - cr).abs() > 0.10 { sum.differ += 1; if ct < cr { sum.tess_better += 1 } else { sum.rapid_better += 1 } }
        }
    }
    for (name, s) in ["working set 0-9", "control set 10-15"].iter().zip(&sets) {
        if s.n == 0 { continue; }
        let n = s.n as f64;
        println!("{name}: {} frames, {} texts | CER tesseract {:.3}  rapid {:.3}  oracle(best of two) {:.3} | differ>0.10: {} ({:.0}%) [tesseract better {}, rapid better {}] | ms/frame tesseract {:.0}  rapid {:.0}",
            s.frames, s.n, s.tess / n, s.rapid / n, s.oracle / n, s.differ, 100.0 * s.differ as f64 / n, s.tess_better, s.rapid_better, s.t_ms / s.frames as f64, s.r_ms / s.frames as f64);
    }
}
