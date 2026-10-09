//! MeikiOCR (rtr46/meikiocr): a detector and a recognizer of Japanese video-game text, ONNX models run in this process
//! by ONNX Runtime, the same library RapidOCR uses. A port of the reference pipeline: the frame is fitted into 960×544
//! for the detector (D-FINE, boxes of text lines), every line is cut out and read by the recognizer, which finds the
//! characters themselves (a label is the Unicode code point, there is no dictionary and no CTC); overlapping
//! candidates are thinned out by non-maximum suppression along the line. Tall boxes are read by the vertical
//! recognizer. Nothing is downloaded here and no other process is started.

use super::rapid::{self, Piece, effective_threads, gpu_providers, inference, init_runtime, resize, session, write_bgr};
use super::rapid_models::{self, OcrModel};
use super::{Ocr, OcrError, OcrResult};
use crate::settings::Settings;
use image::{DynamicImage, RgbImage};
use ort::{session::Session, value::Tensor};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex as StdMutex},
    time::Instant,
};

pub const ENGINE: &str = "meikiocr";

const DET_WIDTH: u32 = 960;
const DET_HEIGHT: u32 = 544;
const REC_HEIGHT: u32 = 32;
const REC_WIDTH: u32 = 960;
const VREC_WIDTH: u32 = 32;
const VREC_HEIGHT: u32 = 480;
/// A vertical line taller than the recognizer is cut into segments of this height (scaled), overlapping by `VREC_OVERLAP`.
const VREC_CONTENT: f64 = 420.0;
const VREC_OVERLAP: f64 = 64.0;
const DET_THRESHOLD: f32 = 0.5;
const REC_THRESHOLD: f32 = 0.1;
/// Candidates overlapping along the line by more than this share of the shorter one are the same character.
const OVERLAP: f32 = 0.3;
const EPSILON: f32 = 1e-6;
const BATCH: usize = 8;
const MAX_ENGINES: usize = 2;

/// The recognizer swaps these pairs of characters now and then (the reference pipeline corrects them the same way).
const SWAPPED_PAIRS: [(&str, &str); 8] = [("儡傀", "傀儡"), ("談冗", "冗談"), ("汰淘", "淘汰"), ("沱滂", "滂沱"), ("攣痙", "痙攣"), ("酊酩", "酩酊"), ("麭麺", "麺麭"), ("哭慟", "慟哭")];

/// One recognised character: its box in the frame and the interval along the line used to thin the candidates out.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub ch: char,
    pub rect: [i32; 4],
    pub confidence: f32,
    pub interval: (i32, i32),
}

/// A part of a line as the recognizer sees it: where it is in the frame, the pixels of the model's input and how much
/// of the input is the picture (the rest is black padding).
struct Crop {
    line: usize,
    rect: [i32; 4],
    content: u32,
    tensor: Vec<f32>,
}

/// Python's `round`: halves go to the even number, like in the reference pipeline.
fn round(value: f64) -> i64 {
    value.round_ties_even() as i64
}

/// Scale of the frame in the detector's input and the size it gets there.
pub fn detector_fit(width: u32, height: u32) -> (f64, u32, u32) {
    let scale = (DET_WIDTH as f64 / width as f64).min(DET_HEIGHT as f64 / height as f64);
    (scale, ((width as f64 * scale) as u32).clamp(1, DET_WIDTH), ((height as f64 * scale) as u32).clamp(1, DET_HEIGHT))
}

/// The boxes of lines the detector is sure of, clamped to the frame and ordered from top to bottom. `boxes` are
/// `x1, y1, x2, y2` in frame pixels, `scores` one per box.
pub fn lines_from_detection(boxes: &[f32], scores: &[f32], frame: (u32, u32), threshold: f32) -> Vec<[i32; 4]> {
    let mut lines: Vec<[i32; 4]> = boxes.as_chunks::<4>().0.iter().zip(scores).filter(|(_, s)| **s > threshold).filter_map(|(b, _)| {
        let clamped = [b[0].clamp(0.0, frame.0 as f32) as i32, b[1].clamp(0.0, frame.1 as f32) as i32, b[2].clamp(0.0, frame.0 as f32) as i32, b[3].clamp(0.0, frame.1 as f32) as i32];
        (clamped[2] > clamped[0] && clamped[3] > clamped[1]).then_some(clamped)
    }).collect();
    lines.sort_by_key(|l| l[1]);
    lines
}

/// Size of a horizontal line scaled for the recognizer: the height is 32, a longer line is squeezed into 960.
pub fn horizontal_size(width: u32, height: u32) -> (u32, u32) {
    let scale = REC_HEIGHT as f64 / height as f64;
    let (mut w, mut h) = (round(width as f64 * scale), REC_HEIGHT as i64);
    if w > REC_WIDTH as i64 {
        h = round(h as f64 * (REC_WIDTH as f64 / w as f64));
        w = REC_WIDTH as i64;
    }
    (w.max(1) as u32, h.max(1) as u32)
}

/// Segments `(top, bottom, scaled height)` of a vertical line `y1..y2` of width `width`: one segment when it fits the
/// recognizer (480 scaled pixels), otherwise overlapping ones of 420.
pub fn vertical_segments(y1: i32, y2: i32, width: i32) -> Vec<(i32, i32, u32)> {
    let scale = VREC_WIDTH as f64 / width as f64;
    let (limit, length, starts) = if (y2 - y1) as f64 * scale > VREC_HEIGHT as f64 {
        let length = VREC_CONTENT / scale;
        let stride = (VREC_CONTENT - VREC_OVERLAP) / scale;
        let mut starts = Vec::new();
        let mut at = y1 as f64;
        while at + length < y2 as f64 {
            starts.push(at);
            at += stride;
        }
        let last = y2 as f64 - length;
        if starts.last().is_none_or(|s| last > s + 1.0) {
            starts.push(last);
        }
        (VREC_CONTENT, length, starts)
    } else {
        (VREC_HEIGHT as f64, (y2 - y1) as f64, vec![y1 as f64])
    };
    starts.into_iter().filter_map(|start| {
        let top = (round(start) as i32).max(0);
        let bottom = (round(start + length) as i32).min(y2);
        (bottom > top).then(|| (top, bottom, (round((bottom - top) as f64 * scale) as f64).min(limit).max(1.0) as u32))
    }).collect()
}

fn horizontal_crop(img: &RgbImage, line: usize, rect: [i32; 4]) -> Option<Crop> {
    let (w, h) = ((rect[2] - rect[0]) as u32, (rect[3] - rect[1]) as u32);
    if w == 0 || h == 0 {
        return None;
    }
    let (new_w, new_h) = horizontal_size(w, h);
    let part = image::imageops::crop_imm(img, rect[0] as u32, rect[1] as u32, w, h).to_image();
    let mut tensor = vec![0.0; (3 * REC_WIDTH * REC_HEIGHT) as usize];
    write_bgr(&resize(&part, new_w, new_h), [0.0; 3], [1.0; 3], &mut tensor, REC_WIDTH as usize, (REC_WIDTH * REC_HEIGHT) as usize);
    Some(Crop { line, rect, content: new_w, tensor })
}

fn vertical_crops(img: &RgbImage, line: usize, rect: [i32; 4]) -> Vec<Crop> {
    let width = rect[2] - rect[0];
    vertical_segments(rect[1], rect[3], width).into_iter().map(|(top, bottom, content)| {
        let part = image::imageops::crop_imm(img, rect[0] as u32, top as u32, width as u32, (bottom - top) as u32).to_image();
        let mut tensor = vec![0.0; (3 * VREC_WIDTH * VREC_HEIGHT) as usize];
        write_bgr(&resize(&part, VREC_WIDTH, content), [0.0; 3], [1.0; 3], &mut tensor, VREC_WIDTH as usize, (VREC_WIDTH * VREC_HEIGHT) as usize);
        Crop { line, rect: [rect[0], top, rect[2], bottom], content, tensor }
    }).collect()
}

/// The characters the recognizer found in one crop, in frame pixels. `labels`, `boxes` (four per label) and `scores`
/// are the rows of the model's output for this crop.
fn decode_crop(crop: &Crop, vertical: bool, labels: &[i64], boxes: &[f32], scores: &[f32]) -> Vec<Candidate> {
    let (crop_w, crop_h) = ((crop.rect[2] - crop.rect[0]) as f32, (crop.rect[3] - crop.rect[1]) as f32);
    let content = crop.content as f32;
    labels.iter().zip(boxes.as_chunks::<4>().0.iter()).zip(scores).filter(|(_, s)| **s >= REC_THRESHOLD).filter_map(|((&label, b), &score)| {
        let ch = u32::try_from(label).ok().and_then(char::from_u32)?;
        let (x1, y1, x2, y2) = (b[0], b[1], b[2], b[3]);
        let (rect, interval) = if vertical {
            if y1 >= content {
                return None;
            }
            let (y1, y2) = (y1.min(content), y2.min(content));
            let rect = [crop.rect[0] + (x1 / VREC_WIDTH as f32 * crop_w) as i32, crop.rect[1] + (y1 / content * crop_h) as i32, crop.rect[0] + (x2 / VREC_WIDTH as f32 * crop_w) as i32, crop.rect[1] + (y2 / content * crop_h) as i32];
            if rect[3] <= rect[1] {
                return None;
            }
            (rect, (rect[1], rect[3]))
        } else {
            if x1 >= content {
                return None;
            }
            let (x1, x2) = (x1.min(content), x2.min(content));
            let rect = [crop.rect[0] + (x1 / content * crop_w) as i32, crop.rect[1] + (y1 / REC_HEIGHT as f32 * crop_h) as i32, crop.rect[0] + (x2 / content * crop_w) as i32, crop.rect[1] + (y2 / REC_HEIGHT as f32 * crop_h) as i32];
            (rect, (rect[0], rect[2]))
        };
        Some(Candidate { ch, rect, confidence: score, interval })
    }).collect()
}

/// Non-maximum suppression along the line: the surest candidate stays, one overlapping it by more than `OVERLAP` of the
/// shorter goes. The rest in reading order along the line.
pub fn suppress(mut candidates: Vec<Candidate>) -> Vec<Candidate> {
    candidates.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
    let mut accepted: Vec<Candidate> = Vec::new();
    for candidate in candidates {
        let (c1, c2) = candidate.interval;
        let length = (c2 - c1) as f32 + EPSILON;
        let overlaps = accepted.iter().any(|a| {
            let (a1, a2) = a.interval;
            if c1 >= a2 || a1 >= c2 {
                return false;
            }
            let inside = (c2.min(a2) - c1.max(a1)).max(0) as f32;
            inside / length.min((a2 - a1) as f32 + EPSILON) > OVERLAP
        });
        if !overlaps {
            accepted.push(candidate);
        }
    }
    accepted.sort_by_key(|c| c.interval.0);
    accepted
}

/// Puts back the pairs of characters the recognizer tends to swap (the first occurrence of each pair).
pub fn fix_swapped_pairs(chars: &mut [Candidate]) {
    for (wrong, _) in SWAPPED_PAIRS {
        let wrong: Vec<char> = wrong.chars().collect();
        if let Some(at) = chars.windows(2).position(|w| w[0].ch == wrong[0] && w[1].ch == wrong[1]) {
            let (a, b) = chars.split_at_mut(at + 1);
            std::mem::swap(&mut a[at].ch, &mut b[0].ch);
        }
    }
}

/// A line of characters as a piece of the frame: text, the box of the detector, the mean confidence.
pub fn piece(line: [i32; 4], chars: &[Candidate]) -> Piece {
    let text: String = chars.iter().map(|c| c.ch).collect();
    let confidence = if chars.is_empty() { 0.0 } else { chars.iter().map(|c| c.confidence).sum::<f32>() / chars.len() as f32 };
    let (x1, y1, x2, y2) = (line[0] as f32, line[1] as f32, line[2] as f32, line[3] as f32);
    Piece { quad: [[x1, y1], [x2, y1], [x2, y2], [x1, y2]], text, confidence, chars: chars.len() }
}

struct Engine {
    det: Session,
    rec: Session,
    /// The vertical recognizer is loaded when the first vertical line asks for it.
    vrec: Option<Session>,
    vrec_path: PathBuf,
    threads: usize,
    gpu: bool,
}

fn providers(gpu: bool) -> Vec<ort::ep::ExecutionProviderDispatch> {
    if !gpu {
        return Vec::new();
    }
    let available = gpu_providers();
    if available.is_empty() {
        tracing::info!(component = ENGINE, "GPU для MeikiOCR недоступен: ONNX Runtime собран без CUDA/MIGraphX/ROCm, используется CPU");
    }
    available.into_iter().map(|(_, p)| p).collect()
}

impl Engine {
    /// The files are there and verified: checked before ONNX Runtime is loaded, so a missing model is reported as such
    /// whether the library is installed or not.
    fn check(model: &OcrModel, root: &Path) -> Result<(), OcrError> {
        if !rapid_models::present(root, model) {
            return Err(OcrError::Setup(format!("MeikiOCR: модель «{}» ({}) не скачана. Скачайте её: «Настройки → Распознавание → MeikiOCR → Скачать».", model.label, model.id)));
        }
        if !rapid_models::verified(root, model) {
            return Err(OcrError::Setup(format!("MeikiOCR: файлы модели «{}» ({}) повреждены (SHA-256 не совпадает). Удалите модель в настройках и скачайте её заново.", model.label, model.id)));
        }
        Ok(())
    }

    fn load(model: &OcrModel, root: &Path, threads: usize, gpu: bool) -> Result<Self, OcrError> {
        let started = Instant::now();
        let providers = providers(gpu);
        let det = session(&model.file(root, "det"), threads, &providers)?;
        let rec = session(&model.file(root, "rec"), threads, &providers)?;
        tracing::info!(component = ENGINE, model = %model.id, threads, gpu = !providers.is_empty(), ms = started.elapsed().as_millis() as u64, "Модели MeikiOCR загружены");
        Ok(Self { det, rec, vrec: None, vrec_path: model.file(root, "vrec"), threads, gpu })
    }

    fn read(&mut self, img: &RgbImage) -> Result<Vec<Piece>, OcrError> {
        let (w, h) = img.dimensions();
        if w < 4 || h < 4 {
            return Ok(Vec::new());
        }
        let started = Instant::now();
        let lines = self.detect(img)?;
        let detected = started.elapsed();
        let mut horizontal = Vec::new();
        let mut vertical = Vec::new();
        for (i, &rect) in lines.iter().enumerate() {
            if rect[3] - rect[1] > rect[2] - rect[0] {
                vertical.extend(vertical_crops(img, i, rect));
            } else {
                horizontal.extend(horizontal_crop(img, i, rect));
            }
        }
        let mut found: Vec<Vec<Candidate>> = vec![Vec::new(); lines.len()];
        self.recognize(horizontal, false, &mut found)?;
        self.recognize(vertical, true, &mut found)?;
        let mut pieces: Vec<Piece> = lines.iter().zip(found).map(|(&rect, chars)| {
            let mut chars = suppress(chars);
            fix_swapped_pairs(&mut chars);
            piece(rect, &chars)
        }).collect();
        // Columns of vertical text run from the right to the left.
        if !lines.is_empty() && lines.iter().all(|l| l[3] - l[1] > l[2] - l[0]) {
            pieces.sort_by(|a, b| b.quad[0][0].total_cmp(&a.quad[0][0]));
        }
        tracing::debug!(component = ENGINE, width = w, height = h, lines = pieces.len(), det_ms = detected.as_millis() as u64, rec_ms = (started.elapsed() - detected).as_millis() as u64, "OCR завершён");
        Ok(pieces)
    }

    fn detect(&mut self, img: &RgbImage) -> Result<Vec<[i32; 4]>, OcrError> {
        let (scale, w, h) = detector_fit(img.width(), img.height());
        let plane = (DET_WIDTH * DET_HEIGHT) as usize;
        let mut input = vec![0.0; 3 * plane];
        write_bgr(&resize(img, w, h), [0.0; 3], [1.0; 3], &mut input, DET_WIDTH as usize, plane);
        let images = Tensor::from_array(([1usize, 3, DET_HEIGHT as usize, DET_WIDTH as usize], input)).map_err(inference)?;
        // The second input is the size of the frame in the units the boxes come back in.
        let sizes = Tensor::from_array(([1usize, 2], vec![(DET_WIDTH as f64 / scale) as i64, (DET_HEIGHT as f64 / scale) as i64])).map_err(inference)?;
        let outputs = self.det.run(ort::inputs![images, sizes]).map_err(inference)?;
        if outputs.len() < 3 {
            return Err(inference("the detector has fewer than three outputs"));
        }
        let (_, boxes) = outputs[1].try_extract_tensor::<f32>().map_err(inference)?;
        let (_, scores) = outputs[2].try_extract_tensor::<f32>().map_err(inference)?;
        if boxes.len() < scores.len() * 4 {
            return Err(inference("the detector's boxes do not match its scores"));
        }
        Ok(lines_from_detection(boxes, scores, img.dimensions(), DET_THRESHOLD))
    }

    fn recognize(&mut self, crops: Vec<Crop>, vertical: bool, found: &mut [Vec<Candidate>]) -> Result<(), OcrError> {
        if crops.is_empty() {
            return Ok(());
        }
        if vertical && self.vrec.is_none() {
            self.vrec = Some(session(&self.vrec_path, self.threads, &providers(self.gpu))?);
        }
        let (width, height) = if vertical { (VREC_WIDTH, VREC_HEIGHT) } else { (REC_WIDTH, REC_HEIGHT) };
        let plane = (3 * width * height) as usize;
        for batch in crops.chunks(BATCH) {
            let mut input = Vec::with_capacity(batch.len() * plane);
            for crop in batch {
                input.extend_from_slice(&crop.tensor);
            }
            let images = Tensor::from_array(([batch.len(), 3, height as usize, width as usize], input)).map_err(inference)?;
            let sizes = Tensor::from_array(([1usize, 2], vec![width as i64, height as i64])).map_err(inference)?;
            let session = if vertical { self.vrec.as_mut().expect("loaded above") } else { &mut self.rec };
            let outputs = session.run(ort::inputs!["images" => images, "orig_target_sizes" => sizes]).map_err(inference)?;
            if outputs.len() < 3 {
                return Err(inference("the recognizer has fewer than three outputs"));
            }
            let labels: Vec<i64> = match outputs[0].try_extract_tensor::<i64>() {
                Ok((_, labels)) => labels.to_vec(),
                Err(_) => outputs[0].try_extract_tensor::<i32>().map_err(inference)?.1.iter().map(|&l| l as i64).collect(),
            };
            let (_, boxes) = outputs[1].try_extract_tensor::<f32>().map_err(inference)?;
            let (_, scores) = outputs[2].try_extract_tensor::<f32>().map_err(inference)?;
            let per_crop = scores.len() / batch.len();
            if per_crop == 0 || labels.len() < batch.len() * per_crop || boxes.len() < batch.len() * per_crop * 4 {
                return Err(inference("the recognizer's outputs do not match the batch"));
            }
            for (k, crop) in batch.iter().enumerate() {
                let range = k * per_crop..(k + 1) * per_crop;
                found[crop.line].extend(decode_crop(crop, vertical, &labels[range.clone()], &boxes[range.start * 4..range.end * 4], &scores[range]));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Key {
    model: String,
    threads: usize,
    gpu: bool,
}

/// The engine: loaded models (at most `MAX_ENGINES`), each used by one frame at a time.
pub struct MeikiOcr {
    root: PathBuf,
    library: Option<PathBuf>,
    engines: tokio::sync::Mutex<Vec<(Key, Arc<StdMutex<Engine>>)>>,
}

impl Default for MeikiOcr {
    fn default() -> Self {
        Self::new(rapid_models::cache_root(), None)
    }
}

impl MeikiOcr {
    /// Models from `models_root/<id>/`, ONNX Runtime from `library` or the system. Both paths are made absolute here.
    pub fn new(models_root: PathBuf, library: Option<PathBuf>) -> Self {
        let absolute = |p: PathBuf| std::path::absolute(&p).unwrap_or(p);
        Self { root: absolute(models_root), library: library.map(absolute), engines: Default::default() }
    }

    /// The model for this language is in the cache (a cheap check by size): `auto` asks MeikiOCR only then.
    pub fn installed(&self, language_spec: &str) -> bool {
        rapid_models::select_meiki(language_spec).is_ok_and(|m| rapid_models::present(&self.root, m))
    }

    async fn engine(&self, model: &'static OcrModel, threads: usize, gpu: bool) -> Result<Arc<StdMutex<Engine>>, OcrError> {
        let key = Key { model: model.id.clone(), threads, gpu };
        let mut engines = self.engines.lock().await;
        if let Some((_, engine)) = engines.iter().find(|(k, _)| *k == key) {
            return Ok(engine.clone());
        }
        engines.retain(|(k, _)| k.model != key.model);
        let (root, library) = (self.root.clone(), self.library.clone());
        let engine = tokio::task::spawn_blocking(move || {
            Engine::check(model, &root)?;
            init_runtime(library.as_deref())?;
            Engine::load(model, &root, threads, gpu)
        }).await.map_err(|e| OcrError::Failed(format!("загрузка MeikiOCR прервана: {e}")))??;
        let engine = Arc::new(StdMutex::new(engine));
        if engines.len() >= MAX_ENGINES {
            engines.remove(0);
        }
        engines.push((key, engine.clone()));
        Ok(engine)
    }
}

impl Ocr for MeikiOcr {
    async fn recognize(&self, img: &DynamicImage, settings: &Settings) -> Result<String, OcrError> {
        Ok(self.recognize_detailed(img, settings).await?.text)
    }

    async fn recognize_detailed(&self, img: &DynamicImage, settings: &Settings) -> Result<OcrResult, OcrError> {
        let r = &settings.recognition;
        tracing::debug!(engine = ENGINE, width = img.width(), height = img.height(), language = %r.language, "Начало OCR");
        let model = rapid_models::select_meiki(&r.language).map_err(OcrError::Setup)?;
        let engine = self.engine(model, effective_threads(r.rapid_threads), r.rapid_use_gpu).await?;
        let img = img.clone();
        let pieces = tokio::task::spawn_blocking(move || engine.lock().unwrap_or_else(|e| e.into_inner()).read(&img.to_rgb8())).await
            .map_err(|e| OcrError::Failed(format!("распознавание MeikiOCR прервано: {e}")))??;
        Ok(OcrResult { engine: ENGINE, ..rapid::to_result(pieces) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(ch: char, start: i32, end: i32, confidence: f32) -> Candidate {
        Candidate { ch, rect: [start, 0, end, 30], confidence, interval: (start, end) }
    }

    #[test]
    fn the_frame_is_fitted_into_the_detector_without_distortion() {
        assert_eq!(detector_fit(1920, 1080), (0.5, 960, 540));
        assert_eq!(detector_fit(960, 544), (1.0, 960, 544));
        let (scale, w, h) = detector_fit(1280, 200);
        assert!((scale - 0.75).abs() < 1e-9 && (w, h) == (960, 150));
        // A tiny frame is enlarged to the width of the detector.
        assert_eq!(detector_fit(480, 100), (2.0, 960, 200));
    }

    #[test]
    fn detections_are_thresholded_clamped_dropped_when_empty_and_ordered_top_down() {
        let boxes = [10.0, 300.0, 200.0, 340.0, -5.0, 20.0, 5000.0, 60.0, 50.0, 90.0, 50.0, 120.0, 0.0, 150.0, 100.0, 180.0];
        let scores = [0.9, 0.8, 0.95, 0.3];
        let lines = lines_from_detection(&boxes, &scores, (1000, 400), 0.5);
        // The third box has no width, the fourth is below the threshold; the second is clamped to the frame.
        assert_eq!(lines, [[0, 20, 1000, 60], [10, 300, 200, 340]]);
    }

    #[test]
    fn horizontal_lines_are_scaled_to_height_32_and_squeezed_into_960() {
        assert_eq!(horizontal_size(320, 64), (160, 32));
        assert_eq!(horizontal_size(100, 32), (100, 32));
        // 4000×40 would be 3200 wide at height 32: it is squeezed, keeping the proportions.
        assert_eq!(horizontal_size(4000, 40), (960, 10));
        assert_eq!(horizontal_size(1, 500), (1, 32), "never an empty image");
    }

    #[test]
    fn a_vertical_line_that_fits_is_one_segment_and_a_long_one_overlaps() {
        // 64 wide → scale 0.5; 800 tall → 400 ≤ 480: one segment of the whole line.
        assert_eq!(vertical_segments(100, 900, 64), [(100, 900, 400)]);
        // 32 wide → scale 1; 1000 tall → cut into 420-tall segments every 356, the last one aligned to the end.
        let segments = vertical_segments(0, 1000, 32);
        assert_eq!(segments.first(), Some(&(0, 420, 420)));
        assert_eq!(segments.last(), Some(&(580, 1000, 420)));
        assert!(segments.windows(2).all(|w| w[1].0 < w[0].1 && w[1].0 > w[0].0), "overlapping, in order: {segments:?}");
        assert!(segments.iter().all(|&(top, bottom, content)| bottom - top == content as i32 && content <= 420));
    }

    #[test]
    fn characters_are_mapped_into_the_frame_and_weak_or_off_the_line_ones_dropped() {
        // A line at (100, 50) of 200×40, scaled to 160×32: the picture is 160 wide of the 960 of the input.
        let crop = Crop { line: 0, rect: [100, 50, 300, 90], content: 160, tensor: Vec::new() };
        let labels = [0x65E5, 0x672C, 0x8A9E, 0x3042, -1];
        let boxes = [0.0, 0.0, 80.0, 32.0, 80.0, 0.0, 160.0, 16.0, 200.0, 0.0, 300.0, 32.0, 0.0, 0.0, 10.0, 32.0, 0.0, 0.0, 10.0, 10.0];
        let scores = [0.9, 0.5, 0.9, 0.05, 0.9];
        let chars = decode_crop(&crop, false, &labels, &boxes, &scores);
        // 語 starts beyond the picture, あ is below the threshold, -1 is no character.
        assert_eq!(chars.iter().map(|c| c.ch).collect::<String>(), "日本");
        assert_eq!(chars[0].rect, [100, 50, 200, 90]);
        assert_eq!(chars[1].rect, [200, 50, 300, 70]);
        assert_eq!(chars[1].interval, (200, 300));
        // A box reaching beyond the picture is cut at its end.
        let wide = decode_crop(&crop, false, &[0x65E5], &[120.0, 0.0, 400.0, 32.0], &[0.9]);
        assert_eq!(wide[0].rect[2], 300);
    }

    #[test]
    fn vertical_characters_are_mapped_along_the_line() {
        let crop = Crop { line: 0, rect: [10, 200, 74, 600], content: 200, tensor: Vec::new() };
        let chars = decode_crop(&crop, true, &[0x7E26], &[0.0, 20.0, 32.0, 100.0], &[0.8]);
        // 64 px wide at 32 → ×2; 400 px tall at 200 → ×2.
        assert_eq!(chars[0].rect, [10, 240, 74, 400]);
        assert_eq!(chars[0].interval, (240, 400));
    }

    #[test]
    fn overlapping_candidates_keep_the_surest_and_the_rest_read_in_order() {
        let kept = suppress(vec![candidate('か', 0, 30, 0.6), candidate('日', 40, 70, 0.9), candidate('力', 2, 28, 0.95), candidate('本', 80, 110, 0.8)]);
        assert_eq!(kept.iter().map(|c| c.ch).collect::<String>(), "力日本");
        // A touch below the share is not a duplicate: neighbours may brush each other.
        let kept = suppress(vec![candidate('A', 0, 30, 0.9), candidate('B', 26, 56, 0.8)]);
        assert_eq!(kept.len(), 2);
        assert!(suppress(Vec::new()).is_empty());
    }

    #[test]
    fn a_swapped_pair_is_put_back() {
        let mut chars: Vec<Candidate> = "これは談冗です".chars().enumerate().map(|(i, c)| candidate(c, i as i32 * 30, i as i32 * 30 + 30, 0.9)).collect();
        fix_swapped_pairs(&mut chars);
        assert_eq!(chars.iter().map(|c| c.ch).collect::<String>(), "これは冗談です");
        let mut same: Vec<Candidate> = "冗談".chars().enumerate().map(|(i, c)| candidate(c, i as i32 * 30, i as i32 * 30 + 30, 0.9)).collect();
        fix_swapped_pairs(&mut same);
        assert_eq!(same.iter().map(|c| c.ch).collect::<String>(), "冗談", "the right order stays");
    }

    #[test]
    fn pieces_carry_the_text_the_box_and_the_mean_confidence() {
        let chars = [candidate('日', 0, 30, 0.9), candidate('本', 30, 60, 0.7)];
        let piece = piece([5, 10, 65, 40], &chars);
        assert_eq!((piece.text.as_str(), piece.chars), ("日本", 2));
        assert!((piece.confidence - 0.8).abs() < 1e-6);
        let result = OcrResult { engine: ENGINE, ..rapid::to_result(vec![piece]) };
        assert_eq!((result.text.as_str(), result.engine), ("日本", "meikiocr"));
        assert_eq!(result.lines[0].rect, crate::layout::CropRect::in_space(5.0, 10.0, 60.0, 30.0));
        assert!((result.confidence.unwrap() - 80.0).abs() < 1e-3);
    }

    #[tokio::test]
    async fn a_missing_model_or_a_language_other_than_japanese_is_a_setup_error_with_the_action() {
        let root = tempfile::tempdir().unwrap();
        let ocr = MeikiOcr::new(root.path().into(), Some(root.path().join("no-library.so")));
        let mut settings = Settings::default();
        settings.recognition.language = "jpn+eng".into();
        let error = ocr.recognize_detailed(&DynamicImage::new_rgb8(64, 32), &settings).await.unwrap_err();
        // The model is checked before the library: the user is told what to download, whatever else is missing.
        assert!(matches!(&error, OcrError::Setup(m) if m.contains("meiki-ja") && m.contains("Скачать")), "{error}");
        settings.recognition.language = "eng".into();
        let error = ocr.recognize_detailed(&DynamicImage::new_rgb8(64, 32), &settings).await.unwrap_err();
        assert!(matches!(&error, OcrError::Setup(m) if m.contains("японский")), "{error}");
    }
}
