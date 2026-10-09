//! RapidOCR: PP-OCRv5 (text detection, text line orientation, recognition) as ONNX models run in this process by ONNX
//! Runtime. The runtime is the system library (`libonnxruntime.so`, Arch package `onnxruntime-cpu`), loaded once when the
//! engine is first used; the models are the verified files of `rapid_models`. Nothing is downloaded here and no other
//! process is started.
//!
//! The pipeline is the one of PaddleOCR: the frame is resized for the DBNet detector, its probability map becomes
//! rotated boxes, every box is cut out and the recognizer reads it (a line the orientation classifier calls upside
//! down is also read turned, and the surer reading kept); its per-step probabilities are decoded by CTC against the
//! dictionary of the model.

use super::rapid_models::{self, OcrModel};
use super::{Ocr, OcrError, OcrLine, OcrResult};
use crate::layout::CropRect;
use crate::settings::Settings;
use image::{DynamicImage, RgbImage, imageops::FilterType};
use ort::{ep::ExecutionProviderDispatch, session::Session, value::Tensor};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex as StdMutex},
    time::Instant,
};

pub const ENGINE: &str = "rapidocr";

/// The detector sees the frame with the shorter side at least this long…
const DET_MIN_SIDE: f32 = 64.0;
/// …and the longer side at most this long: a 1920×1080 frame is read at 1600×896, a 1280×200 subtitle strip as it is.
const DET_MAX_SIDE: f32 = 1600.0;
const DET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const DET_STD: [f32; 3] = [0.229, 0.224, 0.225];
const REC_HEIGHT: u32 = 48;
/// The narrowest batch the recognizer gets (PaddleOCR: `rec_image_shape` 3×48×320).
const REC_MIN_WIDTH: u32 = 320;
/// A longer line is squeezed: the recognizer's memory grows with the width of the batch.
const REC_MAX_WIDTH: u32 = 3200;
const REC_BATCH: usize = 6;
/// Input of the PP-LCNet text line orientation classifier (width, height) when the model does not fix it.
const CLS_SIZE: (u32, u32) = (160, 80);
/// A line is also read turned when the classifier leans this much to «upside down». The recognizer decides, so the
/// threshold can be low: PaddleOCR's 0.9 missed half of an upside-down line split in two boxes (0.79 for one of them).
const CLS_THRESH: f32 = 0.5;
/// The turned reading wins only by this much: a nearly symmetric line («NOW» / «MON», digits) can read both ways,
/// while real upside-down text reads at about 0.45 one way and 0.97 the other.
const TURN_MARGIN: f32 = 0.1;
const CLS_BATCH: usize = 6;
/// Models kept loaded at once: one per capture region at most.
const MAX_ENGINES: usize = 3;
const MAX_THREADS: usize = 16;

/// Threads of ONNX Runtime for a setting: `0` is half of the cores, at most 4, so the game keeps its CPU.
pub fn effective_threads(setting: u32) -> usize {
    if setting > 0 {
        return (setting as usize).min(MAX_THREADS);
    }
    let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
    (cores / 2).clamp(1, 4)
}

/// Where the system installs ONNX Runtime (Arch: `/usr/lib/libonnxruntime.so*` from `onnxruntime-cpu`).
const LIBRARY_CANDIDATES: [&str; 5] = [
    "/usr/lib/libonnxruntime.so.1",
    "/usr/lib/libonnxruntime.so",
    "/usr/lib64/libonnxruntime.so.1",
    "/usr/local/lib/libonnxruntime.so.1",
    "/usr/local/lib/libonnxruntime.so",
];
pub const INSTALL_COMMAND: &str = "sudo pacman -S onnxruntime-cpu";

/// The ONNX Runtime library: `explicit`, `LIPAX_ORT_LIBRARY`, `ORT_DYLIB_PATH`, then the system locations. The path is
/// made absolute now (a relative one against the current directory), never at the moment the library is opened.
pub fn library_path(explicit: Option<&Path>) -> Result<PathBuf, String> {
    let from_env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(path) = explicit.map(Path::to_path_buf).or_else(|| from_env("LIPAX_ORT_LIBRARY")).or_else(|| from_env("ORT_DYLIB_PATH")) {
        return path.canonicalize().ok().filter(|p| p.is_file())
            .ok_or_else(|| format!("RapidOCR: библиотека ONNX Runtime {} не найдена. Проверьте путь или установите библиотеку: {INSTALL_COMMAND}.", path.display()));
    }
    LIBRARY_CANDIDATES.iter().find_map(|p| Path::new(p).canonicalize().ok().filter(|p| p.is_file()))
        .ok_or_else(|| format!("RapidOCR: не найдена библиотека ONNX Runtime (libonnxruntime.so). Установите её: {INSTALL_COMMAND} — и нажмите «Проверить снова». Другой путь можно задать переменной окружения LIPAX_ORT_LIBRARY."))
}

static RUNTIME: StdMutex<Option<PathBuf>> = StdMutex::new(None);

/// Load ONNX Runtime once for the process. Nothing of `ort` may be used before this succeeded: without a loaded library
/// `ort` would look for one by itself. A failure is not remembered, so installing the library helps without a restart.
pub fn init_runtime(explicit: Option<&Path>) -> Result<PathBuf, OcrError> {
    let mut loaded = RUNTIME.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(path) = loaded.as_ref() {
        return Ok(path.clone());
    }
    let path = library_path(explicit).map_err(OcrError::Setup)?;
    let environment = ort::init_from(&path).map_err(|e| OcrError::Setup(format!(
        "RapidOCR: не удалось загрузить ONNX Runtime из {}: {e}. Переустановите библиотеку: {INSTALL_COMMAND}.", path.display())))?;
    let _ = environment.with_name("lipax").commit();
    tracing::info!(component = ENGINE, library = %path.display(), "ONNX Runtime загружен");
    *loaded = Some(path.clone());
    Ok(path)
}

fn inference(error: impl std::fmt::Display) -> OcrError {
    OcrError::Inference(error.to_string())
}

/// The GPU execution providers the loaded ONNX Runtime was built with (CUDA, MIGraphX, ROCm), best first.
fn gpu_providers() -> Vec<(&'static str, ExecutionProviderDispatch)> {
    use ort::ep::{CUDA, ExecutionProvider, MIGraphX, ROCm};
    let mut providers = Vec::new();
    if CUDA::default().is_available().unwrap_or(false) { providers.push(("CUDA", CUDA::default().build())); }
    if MIGraphX::default().is_available().unwrap_or(false) { providers.push(("MIGraphX", MIGraphX::default().build())); }
    if ROCm::default().is_available().unwrap_or(false) { providers.push(("ROCm", ROCm::default().build())); }
    providers
}

fn session(path: &Path, threads: usize, providers: &[ExecutionProviderDispatch]) -> Result<Session, OcrError> {
    let fail = |e: String| OcrError::Setup(format!("RapidOCR: не удалось открыть модель {}: {e}. Удалите модель в настройках и скачайте её заново.", path.display()));
    // No spinning: idle threads of ONNX Runtime would otherwise keep cores busy between frames, next to the game.
    let mut builder = Session::builder().map_err(|e| fail(e.to_string()))?
        .with_intra_threads(threads).map_err(|e| fail(e.to_string()))?
        .with_inter_threads(1).map_err(|e| fail(e.to_string()))?
        .with_intra_op_spinning(false).map_err(|e| fail(e.to_string()))?
        .with_inter_op_spinning(false).map_err(|e| fail(e.to_string()))?;
    if !providers.is_empty() {
        builder = match builder.with_execution_providers(providers) {
            Ok(builder) => builder,
            Err(e) => {
                tracing::warn!(component = ENGINE, error = %e, "GPU недоступен, RapidOCR работает на CPU");
                e.recover()
            }
        };
    }
    builder.commit_from_file(path).map_err(|e| fail(e.to_string()))
}

/// The fixed height and width of a model's image input, if it has them.
fn fixed_size(session: &Session) -> Option<(u32, u32)> {
    let shape = session.inputs().first()?.dtype().tensor_shape()?;
    let (h, w) = (*shape.get(2)?, *shape.get(3)?);
    (h > 0 && w > 0).then_some((w as u32, h as u32))
}

/// The dictionary of a recognizer: one character per line, in the order of the model's classes. Empty lines inside are
/// characters too (the indices must not move); only the end of the last line is not one.
pub fn parse_dictionary(text: &str) -> Vec<String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines: Vec<String> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l).to_owned()).collect();
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

/// Rows of model output as probabilities: PaddleOCR exports them after softmax; logits are converted, just in case.
fn looks_like_probabilities(row: &[f32]) -> bool {
    row.iter().all(|p| (0.0..=1.0001).contains(p)) && (row.iter().sum::<f32>() - 1.0).abs() < 0.01
}

fn softmax(row: &[f32]) -> Vec<f32> {
    let max = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exp: Vec<f32> = row.iter().map(|v| (v - max).exp()).collect();
    let sum: f32 = exp.iter().sum();
    exp.into_iter().map(|e| e / sum).collect()
}

/// Greedy CTC decoding of one line: `probs` is `steps × classes`, class 0 is the blank, `1..=dict.len()` the dictionary,
/// `dict.len() + 1` the space. Repeated classes collapse unless a blank separates them. Returns the text, the mean
/// probability of its characters (0–1) and their count.
pub fn ctc_decode(probs: &[f32], steps: usize, classes: usize, dict: &[String]) -> (String, f32, usize) {
    let (mut text, mut sum, mut count, mut previous) = (String::new(), 0.0, 0, 0);
    let convert = steps > 0 && !looks_like_probabilities(&probs[..classes.min(probs.len())]);
    for t in 0..steps {
        let Some(row) = probs.get(t * classes..(t + 1) * classes) else { break };
        let row = if convert { std::borrow::Cow::Owned(softmax(row)) } else { std::borrow::Cow::Borrowed(row) };
        let (index, p) = row.iter().enumerate().fold((0, f32::NEG_INFINITY), |best, (i, &p)| if p > best.1 { (i, p) } else { best });
        if index != 0 && index != previous {
            let character = match index - 1 {
                i if i < dict.len() => Some(dict[i].as_str()),
                i if i == dict.len() => Some(" "),
                _ => None,
            };
            if let Some(character) = character {
                text.push_str(character);
                sum += p;
                count += 1;
            }
        }
        previous = index;
    }
    (text, if count > 0 { sum / count as f32 } else { 0.0 }, count)
}

/// The detector's input size for a frame: within `DET_MIN_SIDE`…`DET_MAX_SIDE`, both sides multiples of 32.
pub fn det_size(width: u32, height: u32) -> (u32, u32) {
    let (w, h) = (width as f32, height as f32);
    let mut ratio = if w.min(h) < DET_MIN_SIDE { DET_MIN_SIDE / w.min(h) } else { 1.0 };
    if w.max(h) * ratio > DET_MAX_SIDE {
        ratio = DET_MAX_SIDE / w.max(h);
    }
    let round = |v: f32| (((v * ratio) / 32.0).round() as u32 * 32).max(32);
    (round(w), round(h))
}

/// A rotated rectangle: top left, top right, bottom right, bottom left.
pub type Quad = [[f32; 2]; 4];

/// Parameters of the DB post-processing (RapidOCR's defaults for PP-OCRv5).
#[derive(Debug, Clone, Copy)]
pub struct DbParams {
    pub thresh: f32,
    pub box_thresh: f32,
    pub unclip_ratio: f32,
    pub min_size: f32,
    pub max_candidates: usize,
}

impl Default for DbParams {
    fn default() -> Self {
        Self { thresh: 0.3, box_thresh: 0.5, unclip_ratio: 1.6, min_size: 3.0, max_candidates: 1000 }
    }
}

fn cross(o: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Convex hull (monotone chain), counter-clockwise without collinear points.
fn convex_hull(mut points: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let mut hull: Vec<[f32; 2]> = Vec::with_capacity(points.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &[f32; 2]>> = if pass == 0 { Box::new(points.iter()) } else { Box::new(points.iter().rev()) };
        for &p in iter {
            while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

/// The order of PaddleOCR's `get_mini_boxes`: of the two leftmost corners the upper one is the first, of the two
/// rightmost the upper one the second.
fn order_box(mut corners: [[f32; 2]; 4]) -> Quad {
    corners.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let (tl, bl) = if corners[1][1] > corners[0][1] { (corners[0], corners[1]) } else { (corners[1], corners[0]) };
    let (tr, br) = if corners[3][1] > corners[2][1] { (corners[2], corners[3]) } else { (corners[3], corners[2]) };
    [tl, tr, br, bl]
}

/// The smallest rectangle around `points` (rotating calipers on the hull) and its shorter side, as `cv2.minAreaRect`.
pub fn min_area_rect(points: Vec<[f32; 2]>) -> Option<(Quad, f32)> {
    let hull = convex_hull(points);
    if hull.is_empty() {
        return None;
    }
    if hull.len() < 3 {
        // A point or a segment: a rectangle without height.
        let (a, b) = (hull[0], *hull.last().unwrap());
        return Some((order_box([a, b, b, a]), 0.0));
    }
    let mut best: Option<(f32, [[f32; 2]; 4], f32)> = None;
    for i in 0..hull.len() {
        let (a, b) = (hull[i], hull[(i + 1) % hull.len()]);
        let length = distance(a, b);
        if length == 0.0 {
            continue;
        }
        let u = [(b[0] - a[0]) / length, (b[1] - a[1]) / length];
        let v = [-u[1], u[0]];
        let (mut u0, mut u1, mut v0, mut v1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for p in &hull {
            let (pu, pv) = (p[0] * u[0] + p[1] * u[1], p[0] * v[0] + p[1] * v[1]);
            (u0, u1, v0, v1) = (u0.min(pu), u1.max(pu), v0.min(pv), v1.max(pv));
        }
        let area = (u1 - u0) * (v1 - v0);
        if best.as_ref().is_none_or(|(a, _, _)| area < *a) {
            let at = |s: f32, t: f32| [u[0] * s + v[0] * t, u[1] * s + v[1] * t];
            best = Some((area, [at(u0, v0), at(u1, v0), at(u1, v1), at(u0, v1)], (u1 - u0).min(v1 - v0)));
        }
    }
    best.map(|(_, corners, side)| (order_box(corners), side))
}

fn inside(quad: &Quad, x: f32, y: f32) -> bool {
    let signs: [f32; 4] = std::array::from_fn(|i| cross(quad[i], quad[(i + 1) % 4], [x, y]));
    signs.iter().all(|&s| s >= -1e-3) || signs.iter().all(|&s| s <= 1e-3)
}

/// The mean probability inside the box («fast» score of PaddleOCR).
fn box_score(map: &[f32], width: usize, height: usize, quad: &Quad) -> f32 {
    let clamp = |v: f32, max: usize| (v.max(0.0) as usize).min(max - 1);
    let x0 = clamp(quad.iter().map(|p| p[0]).fold(f32::MAX, f32::min).floor(), width);
    let x1 = clamp(quad.iter().map(|p| p[0]).fold(f32::MIN, f32::max).ceil(), width);
    let y0 = clamp(quad.iter().map(|p| p[1]).fold(f32::MAX, f32::min).floor(), height);
    let y1 = clamp(quad.iter().map(|p| p[1]).fold(f32::MIN, f32::max).ceil(), height);
    let (mut sum, mut count) = (0.0, 0u32);
    for y in y0..=y1 {
        for x in x0..=x1 {
            if inside(quad, x as f32, y as f32) {
                sum += map[y * width + x];
                count += 1;
            }
        }
    }
    if count == 0 { 0.0 } else { sum / count as f32 }
}

/// The box grown on every side by `area × ratio / perimeter` (PaddleOCR's `unclip`; the rounded corners of the
/// polygon offset do not change the rectangle around it).
fn unclip(quad: &Quad, ratio: f32) -> Quad {
    let (w, h) = (distance(quad[0], quad[1]), distance(quad[0], quad[3]));
    if w == 0.0 {
        return *quad;
    }
    let d = w * h * ratio / (2.0 * (w + h));
    let u = [(quad[1][0] - quad[0][0]) / w, (quad[1][1] - quad[0][1]) / w];
    let v = if h > 0.0 { [(quad[3][0] - quad[0][0]) / h, (quad[3][1] - quad[0][1]) / h] } else { [-u[1], u[0]] };
    let c = [(quad[0][0] + quad[2][0]) / 2.0, (quad[0][1] + quad[2][1]) / 2.0];
    let (hw, hh) = (w / 2.0 + d, h / 2.0 + d);
    let at = |s: f32, t: f32| [c[0] + u[0] * s + v[0] * t, c[1] + u[1] * s + v[1] * t];
    order_box([at(-hw, -hh), at(hw, -hh), at(hw, hh), at(-hw, hh)])
}

/// Text boxes in a DBNet probability map of `width × height`, in the map's pixels.
pub fn boxes_from_map(map: &[f32], width: usize, height: usize, p: &DbParams) -> Vec<Quad> {
    let mask: Vec<bool> = map.iter().map(|&v| v > p.thresh).collect();
    let mut seen = vec![false; mask.len()];
    let mut boxes = Vec::new();
    let mut candidates = 0;
    let mut stack = Vec::new();
    for start in 0..mask.len() {
        if !mask[start] || seen[start] {
            continue;
        }
        if candidates >= p.max_candidates {
            break;
        }
        candidates += 1;
        // One 8-connected region, as an outer contour of `cv2.findContours`.
        let mut points = Vec::new();
        seen[start] = true;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % width, i / width);
            points.push([x as f32, y as f32]);
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if (dx, dy) == (0, 0) || nx < 0 || ny < 0 || nx >= width as i64 || ny >= height as i64 {
                        continue;
                    }
                    let j = ny as usize * width + nx as usize;
                    if mask[j] && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        let Some((quad, side)) = min_area_rect(points) else { continue };
        if side < p.min_size || box_score(map, width, height, &quad) < p.box_thresh {
            continue;
        }
        let grown = unclip(&quad, p.unclip_ratio);
        let Some((grown, side)) = min_area_rect(grown.to_vec()) else { continue };
        if side < p.min_size + 2.0 {
            continue;
        }
        boxes.push(grown);
    }
    boxes
}

/// Boxes of the map in pixels of the frame, without slivers, top to bottom and left to right on a row (PaddleOCR's
/// `filter_tag_det_res` and `sorted_boxes`).
pub fn frame_boxes(boxes: Vec<Quad>, map: (usize, usize), frame: (u32, u32)) -> Vec<Quad> {
    let (sx, sy) = (frame.0 as f32 / map.0 as f32, frame.1 as f32 / map.1 as f32);
    let (mx, my) = ((frame.0 - 1) as f32, (frame.1 - 1) as f32);
    let mut out: Vec<Quad> = boxes.into_iter().map(|q| q.map(|p| [(p[0] * sx).round().clamp(0.0, mx), (p[1] * sy).round().clamp(0.0, my)]))
        .filter(|q| distance(q[0], q[1]) as i32 > 3 && distance(q[0], q[3]) as i32 > 3)
        .collect();
    out.sort_by(|a, b| a[0][1].total_cmp(&b[0][1]).then(a[0][0].total_cmp(&b[0][0])));
    for i in 0..out.len().saturating_sub(1) {
        for j in (0..=i).rev() {
            if (out[j + 1][0][1] - out[j][0][1]).abs() < 10.0 && out[j + 1][0][0] < out[j][0][0] {
                out.swap(j, j + 1);
            } else {
                break;
            }
        }
    }
    out
}

fn sample(img: &RgbImage, x: f32, y: f32) -> [f32; 3] {
    let (w, h) = (img.width() as i64, img.height() as i64);
    let (x, y) = (x.clamp(0.0, (w - 1) as f32), y.clamp(0.0, (h - 1) as f32));
    let (x0, y0) = (x.floor() as i64, y.floor() as i64);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let px = |x: i64, y: i64| img.get_pixel(x.min(w - 1) as u32, y.min(h - 1) as u32).0;
    let (a, b, c, d) = (px(x0, y0), px(x0 + 1, y0), px(x0, y0 + 1), px(x0 + 1, y0 + 1));
    std::array::from_fn(|i| {
        let top = a[i] as f32 * (1.0 - fx) + b[i] as f32 * fx;
        let bottom = c[i] as f32 * (1.0 - fx) + d[i] as f32 * fx;
        top * (1.0 - fy) + bottom * fy
    })
}

/// The box cut out as an upright image (PaddleOCR's `get_rotate_crop_image`): a tall box is a vertical line and is
/// turned a quarter counter-clockwise.
pub fn crop(img: &RgbImage, q: &Quad) -> Option<RgbImage> {
    let w = distance(q[0], q[1]).max(distance(q[2], q[3])) as u32;
    let h = distance(q[0], q[3]).max(distance(q[1], q[2])) as u32;
    if w == 0 || h == 0 {
        return None;
    }
    let out = RgbImage::from_fn(w, h, |u, v| {
        let (s, t) = (u as f32 / w as f32, v as f32 / h as f32);
        let point = |i: usize| (1.0 - s) * (1.0 - t) * q[0][i] + s * (1.0 - t) * q[1][i] + s * t * q[2][i] + (1.0 - s) * t * q[3][i];
        image::Rgb(sample(img, point(0), point(1)).map(|c| c.round().clamp(0.0, 255.0) as u8))
    });
    Some(if h as f32 / w as f32 >= 1.5 { image::imageops::rotate270(&out) } else { out })
}

/// Writes `img` into `out` (planes of `stride × height`) in the BGR order the PaddleOCR models were trained on.
fn write_bgr(img: &RgbImage, mean: [f32; 3], std: [f32; 3], out: &mut [f32], stride: usize, plane: usize) {
    for (x, y, pixel) in img.enumerate_pixels() {
        for c in 0..3 {
            let value = pixel.0[2 - c] as f32 / 255.0;
            out[c * plane + y as usize * stride + x as usize] = (value - mean[c]) / std[c];
        }
    }
}

fn resize(img: &RgbImage, width: u32, height: u32) -> RgbImage {
    if img.dimensions() == (width, height) { img.clone() } else { image::imageops::resize(img, width, height, FilterType::Triangle) }
}

/// One recognised box.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub quad: Quad,
    pub text: String,
    /// 0–1, mean over the characters.
    pub confidence: f32,
    pub chars: usize,
}

struct Engine {
    det: Session,
    cls: Session,
    cls_size: (u32, u32),
    rec: Session,
    dict: Vec<String>,
}

impl Engine {
    /// The files of the model are there and verified: checked before ONNX Runtime is loaded, so a missing model is
    /// reported as such whether the library is installed or not.
    fn check(model: &OcrModel, root: &Path) -> Result<(), OcrError> {
        if !rapid_models::present(root, model) {
            return Err(OcrError::Setup(format!(
                "RapidOCR: модель «{}» ({}) не скачана. Скачайте её: «Настройки → Распознавание → RapidOCR → Скачать».", model.label, model.id)));
        }
        if !rapid_models::verified(root, model) {
            return Err(OcrError::Setup(format!(
                "RapidOCR: файлы модели «{}» ({}) повреждены (SHA-256 не совпадает). Удалите модель в настройках и скачайте её заново.", model.label, model.id)));
        }
        Ok(())
    }

    fn load(model: &OcrModel, root: &Path, threads: usize, gpu: bool) -> Result<Self, OcrError> {
        let dict = std::fs::read_to_string(model.file(root, "dict")).map(|t| parse_dictionary(&t))
            .map_err(|e| OcrError::Setup(format!("RapidOCR: словарь модели {} не читается: {e}", model.id)))?;
        let providers = if gpu {
            let available = gpu_providers();
            if available.is_empty() {
                tracing::info!(component = ENGINE, "GPU для RapidOCR недоступен: ONNX Runtime собран без CUDA/MIGraphX/ROCm, используется CPU");
            } else {
                tracing::info!(component = ENGINE, providers = ?available.iter().map(|(n, _)| *n).collect::<Vec<_>>(), "RapidOCR пробует GPU");
            }
            available.into_iter().map(|(_, p)| p).collect()
        } else {
            Vec::new()
        };
        let started = Instant::now();
        let det = session(&model.file(root, "det"), threads, &providers)?;
        // The classifier is tiny: more threads only cost their start.
        let cls = session(&model.file(root, "cls"), threads.min(2), &providers)?;
        let rec = session(&model.file(root, "rec"), threads, &providers)?;
        let cls_size = fixed_size(&cls).unwrap_or(CLS_SIZE);
        tracing::info!(component = ENGINE, model = %model.id, threads, gpu = !providers.is_empty(), ms = started.elapsed().as_millis() as u64, "Модели RapidOCR загружены");
        Ok(Self { det, cls, cls_size, rec, dict })
    }

    fn read(&mut self, img: &RgbImage) -> Result<Vec<Piece>, OcrError> {
        let (w, h) = img.dimensions();
        if w < 4 || h < 4 {
            return Ok(Vec::new());
        }
        let started = Instant::now();
        let quads = self.detect(img)?;
        let detected = started.elapsed();
        let crops: Vec<(Quad, RgbImage)> = quads.into_iter().filter_map(|q| crop(img, &q).map(|c| (q, c))).collect();
        let turned: Vec<(usize, (Quad, RgbImage))> = self.upside_down(&crops)?.into_iter()
            .map(|i| (i, (crops[i].0, image::imageops::rotate180(&crops[i].1)))).collect();
        let oriented = started.elapsed();
        let mut pieces = self.recognize(crops)?;
        // The classifier only suggests: a line it calls upside down is read both ways and the surer reading is kept.
        // On long thin crops over a textured background it calls upright lines upside down with 0.92 (measured on a
        // 1920×1080 frame), and turning them would lose the line.
        if !turned.is_empty() {
            let (indices, turned): (Vec<usize>, Vec<(Quad, RgbImage)>) = turned.into_iter().unzip();
            let mut kept = 0;
            for (i, other) in indices.into_iter().zip(self.recognize(turned)?) {
                if !other.text.trim().is_empty() && other.confidence > pieces[i].confidence + TURN_MARGIN {
                    pieces[i] = other;
                    kept += 1;
                }
            }
            // Every line upside down: the whole frame is, and it reads from the last box to the first.
            if kept == pieces.len() {
                pieces.reverse();
            }
        }
        tracing::debug!(component = ENGINE, width = w, height = h, lines = pieces.len(), det_ms = detected.as_millis() as u64,
            cls_ms = (oriented - detected).as_millis() as u64, rec_ms = (started.elapsed() - oriented).as_millis() as u64, "OCR завершён");
        Ok(pieces)
    }

    fn detect(&mut self, img: &RgbImage) -> Result<Vec<Quad>, OcrError> {
        let (rw, rh) = det_size(img.width(), img.height());
        let resized = resize(img, rw, rh);
        let plane = (rw * rh) as usize;
        let mut input = vec![0.0; 3 * plane];
        write_bgr(&resized, DET_MEAN, DET_STD, &mut input, rw as usize, plane);
        let tensor = Tensor::from_array(([1usize, 3, rh as usize, rw as usize], input)).map_err(inference)?;
        let outputs = self.det.run(ort::inputs![tensor]).map_err(inference)?;
        let (shape, map) = outputs[0].try_extract_tensor::<f32>().map_err(inference)?;
        let [.., mh, mw] = shape[..] else { return Err(inference("detector output has no map")) };
        let (mh, mw) = (mh as usize, mw as usize);
        if mh * mw == 0 || map.len() < mh * mw {
            return Err(inference("detector output has an unexpected shape"));
        }
        let boxes = boxes_from_map(&map[..mh * mw], mw, mh, &DbParams::default());
        Ok(frame_boxes(boxes, (mw, mh), img.dimensions()))
    }

    /// The lines the classifier finds upside down (indices into `crops`).
    fn upside_down(&mut self, crops: &[(Quad, RgbImage)]) -> Result<Vec<usize>, OcrError> {
        let mut found = Vec::new();
        let (cw, ch) = self.cls_size;
        let plane = (cw * ch) as usize;
        for (b, batch) in crops.chunks(CLS_BATCH).enumerate() {
            let mut input = vec![0.0; batch.len() * 3 * plane];
            for (i, (_, c)) in batch.iter().enumerate() {
                write_bgr(&resize(c, cw, ch), DET_MEAN, DET_STD, &mut input[i * 3 * plane..(i + 1) * 3 * plane], cw as usize, plane);
            }
            let tensor = Tensor::from_array(([batch.len(), 3, ch as usize, cw as usize], input)).map_err(inference)?;
            let outputs = self.cls.run(ort::inputs![tensor]).map_err(inference)?;
            let (shape, scores) = outputs[0].try_extract_tensor::<f32>().map_err(inference)?;
            let classes = shape.last().copied().unwrap_or(0) as usize;
            if classes < 2 || scores.len() < batch.len() * classes {
                return Err(inference("orientation classifier output has an unexpected shape"));
            }
            for i in 0..batch.len() {
                let row = &scores[i * classes..(i + 1) * classes];
                let row = if looks_like_probabilities(row) { row.to_vec() } else { softmax(row) };
                if row[1] > row[0] && row[1] > CLS_THRESH {
                    found.push(b * CLS_BATCH + i);
                }
            }
        }
        Ok(found)
    }

    fn recognize(&mut self, crops: Vec<(Quad, RgbImage)>) -> Result<Vec<Piece>, OcrError> {
        let ratio = |c: &RgbImage| c.width() as f32 / c.height() as f32;
        let mut order: Vec<usize> = (0..crops.len()).collect();
        order.sort_by(|&a, &b| ratio(&crops[a].1).total_cmp(&ratio(&crops[b].1)));
        let mut pieces: Vec<Option<Piece>> = vec![None; crops.len()];
        for batch in order.chunks(REC_BATCH) {
            let widest = batch.iter().map(|&i| ratio(&crops[i].1)).fold(REC_MIN_WIDTH as f32 / REC_HEIGHT as f32, f32::max);
            let width = ((REC_HEIGHT as f32 * widest).ceil() as u32).min(REC_MAX_WIDTH);
            let plane = (width * REC_HEIGHT) as usize;
            let mut input = vec![0.0; batch.len() * 3 * plane];
            for (k, &i) in batch.iter().enumerate() {
                let resized_w = ((REC_HEIGHT as f32 * ratio(&crops[i].1)).ceil() as u32).clamp(1, width);
                let line = resize(&crops[i].1, resized_w, REC_HEIGHT);
                write_bgr(&line, [0.5; 3], [0.5; 3], &mut input[k * 3 * plane..(k + 1) * 3 * plane], width as usize, plane);
            }
            let tensor = Tensor::from_array(([batch.len(), 3, REC_HEIGHT as usize, width as usize], input)).map_err(inference)?;
            let outputs = self.rec.run(ort::inputs![tensor]).map_err(inference)?;
            let (shape, probs) = outputs[0].try_extract_tensor::<f32>().map_err(inference)?;
            let [_, steps, classes] = shape[..] else { return Err(inference("recognizer output is not batch × steps × classes")) };
            let (steps, classes) = (steps as usize, classes as usize);
            if classes != self.dict.len() + 2 && classes != self.dict.len() + 1 {
                return Err(OcrError::Setup(format!("RapidOCR: словарь ({} символов) не подходит к распознавателю ({classes} классов). Удалите модель и скачайте её заново.", self.dict.len())));
            }
            for (k, &i) in batch.iter().enumerate() {
                let (text, confidence, chars) = ctc_decode(&probs[k * steps * classes..(k + 1) * steps * classes], steps, classes, &self.dict);
                pieces[i] = Some(Piece { quad: crops[i].0, text, confidence, chars });
            }
        }
        Ok(pieces.into_iter().flatten().collect())
    }
}

/// The recognised boxes as lines of the frame: text, rectangle around the box, confidence 0–100 (the mean over the
/// characters of the line; the result's is the mean over all characters).
pub fn to_result(pieces: Vec<Piece>) -> OcrResult {
    let pieces: Vec<Piece> = pieces.into_iter().filter(|p| !p.text.trim().is_empty()).collect();
    let chars: usize = pieces.iter().map(|p| p.chars).sum();
    let confidence = (chars > 0).then(|| pieces.iter().map(|p| p.confidence * p.chars as f32).sum::<f32>() / chars as f32 * 100.0);
    let lines: Vec<OcrLine> = pieces.into_iter().map(|p| {
        let (x0, x1) = (p.quad.iter().map(|c| c[0]).fold(f32::MAX, f32::min), p.quad.iter().map(|c| c[0]).fold(f32::MIN, f32::max));
        let (y0, y1) = (p.quad.iter().map(|c| c[1]).fold(f32::MAX, f32::min), p.quad.iter().map(|c| c[1]).fold(f32::MIN, f32::max));
        OcrLine { rect: CropRect::in_space(x0, y0, x1 - x0, y1 - y0), text: p.text.trim().to_owned(), confidence: (p.confidence * 100.0).clamp(0.0, 100.0) }
    }).collect();
    let text = lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
    OcrResult { text, lines, confidence, engine: ENGINE, ..OcrResult::default() }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Key {
    model: String,
    threads: usize,
    gpu: bool,
}

/// The engine: loaded models per language (at most `MAX_ENGINES`), each used by one frame at a time.
pub struct RapidOcr {
    root: PathBuf,
    library: Option<PathBuf>,
    engines: tokio::sync::Mutex<Vec<(Key, Arc<StdMutex<Engine>>)>>,
}

impl Default for RapidOcr {
    fn default() -> Self {
        Self::new(rapid_models::cache_root(), None)
    }
}

impl RapidOcr {
    /// Models from `models_root/<id>/`, ONNX Runtime from `library` or the system. Both paths are made absolute here.
    pub fn new(models_root: PathBuf, library: Option<PathBuf>) -> Self {
        let absolute = |p: PathBuf| std::path::absolute(&p).unwrap_or(p);
        Self { root: absolute(models_root), library: library.map(absolute), engines: Default::default() }
    }

    async fn engine(&self, model: &'static OcrModel, threads: usize, gpu: bool) -> Result<Arc<StdMutex<Engine>>, OcrError> {
        let key = Key { model: model.id.clone(), threads, gpu };
        let mut engines = self.engines.lock().await;
        if let Some((_, engine)) = engines.iter().find(|(k, _)| *k == key) {
            return Ok(engine.clone());
        }
        // The same model with other threads or GPU settings is replaced.
        engines.retain(|(k, _)| k.model != key.model);
        let (root, library) = (self.root.clone(), self.library.clone());
        let engine = tokio::task::spawn_blocking(move || {
            Engine::check(model, &root)?;
            init_runtime(library.as_deref())?;
            Engine::load(model, &root, threads, gpu)
        }).await.map_err(|e| OcrError::Failed(format!("загрузка RapidOCR прервана: {e}")))??;
        let engine = Arc::new(StdMutex::new(engine));
        if engines.len() >= MAX_ENGINES {
            engines.remove(0);
        }
        engines.push((key, engine.clone()));
        Ok(engine)
    }
}

impl Ocr for RapidOcr {
    async fn recognize(&self, img: &DynamicImage, settings: &Settings) -> Result<String, OcrError> {
        Ok(self.recognize_detailed(img, settings).await?.text)
    }

    async fn recognize_detailed(&self, img: &DynamicImage, settings: &Settings) -> Result<OcrResult, OcrError> {
        let r = &settings.recognition;
        tracing::debug!(engine = ENGINE, width = img.width(), height = img.height(), language = %r.language, "Начало OCR");
        let model = rapid_models::select(&r.language, &r.rapid_variant).map_err(OcrError::Setup)?;
        let engine = self.engine(model, effective_threads(r.rapid_threads), r.rapid_use_gpu).await?;
        let img = img.clone();
        let pieces = tokio::task::spawn_blocking(move || engine.lock().unwrap_or_else(|e| e.into_inner()).read(&img.to_rgb8())).await
            .map_err(|e| OcrError::Failed(format!("распознавание RapidOCR прервано: {e}")))??;
        Ok(to_result(pieces))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_keeps_indices_and_only_drops_the_final_newline() {
        assert_eq!(parse_dictionary("a\nb\r\nc\n"), ["a", "b", "c"]);
        assert_eq!(parse_dictionary("\u{feff}x\ny"), ["x", "y"]);
        // A space and an empty line inside are characters: the classes after them must keep their index.
        assert_eq!(parse_dictionary("a\n \n\nб\n"), ["a", " ", "", "б"]);
        assert!(parse_dictionary("").is_empty());
    }

    fn one_hot(steps: &[usize], classes: usize, p: f32) -> Vec<f32> {
        steps.iter().flat_map(|&i| (0..classes).map(move |c| if c == i { p } else { (1.0 - p) / (classes - 1) as f32 })).collect()
    }

    #[test]
    fn ctc_collapses_repeats_skips_blanks_and_averages_per_character() {
        let dict: Vec<String> = ["H", "e", "l", "o"].map(String::from).to_vec();
        let classes = dict.len() + 2;
        // H H e blank l l blank l o space blank
        let probs = one_hot(&[1, 1, 2, 0, 3, 3, 0, 3, 4, 5, 0], classes, 0.9);
        let (text, confidence, chars) = ctc_decode(&probs, 11, classes, &dict);
        assert_eq!(text, "Hello ");
        assert_eq!(chars, 6);
        assert!((confidence - 0.9).abs() < 1e-5);
        let (empty, confidence, chars) = ctc_decode(&one_hot(&[0, 0], classes, 0.99), 2, classes, &dict);
        assert_eq!((empty.as_str(), confidence, chars), ("", 0.0, 0));
        // Logits instead of probabilities give the same text.
        let logits: Vec<f32> = probs.iter().map(|p| p.ln() * 3.0).collect();
        assert_eq!(ctc_decode(&logits, 11, classes, &dict).0, "Hello ");
    }

    #[test]
    fn ctc_reads_cyrillic_and_cjk_dictionaries() {
        let dict = parse_dictionary("П\nр\nи\nв\nе\nт\n日\n本\n");
        let classes = dict.len() + 2;
        let (text, _, chars) = ctc_decode(&one_hot(&[1, 2, 3, 4, 5, 6, 0, 7, 8], classes, 0.8), 9, classes, &dict);
        assert_eq!((text.as_str(), chars), ("Привет日本", 8));
    }

    #[test]
    fn detector_input_is_bounded_and_aligned() {
        assert_eq!(det_size(1280, 200), (1280, 192));
        assert_eq!(det_size(1920, 1080), (1600, 896));
        assert_eq!(det_size(300, 40), (480, 64));
        assert_eq!(det_size(3840, 2160), (1600, 896));
        for (w, h) in [(5, 5), (17, 4000), (640, 480)] {
            let (rw, rh) = det_size(w, h);
            assert!(rw % 32 == 0 && rh % 32 == 0 && rw >= 32 && rh >= 32 && rw.max(rh) as f32 <= DET_MAX_SIDE + 16.0, "{w}×{h} → {rw}×{rh}");
        }
    }

    #[test]
    fn rectangles_are_found_minimal_and_ordered() {
        let (quad, side) = min_area_rect(vec![[10.0, 5.0], [20.0, 5.0], [20.0, 9.0], [10.0, 9.0], [15.0, 7.0]]).unwrap();
        assert_eq!(quad, [[10.0, 5.0], [20.0, 5.0], [20.0, 9.0], [10.0, 9.0]]);
        assert_eq!(side, 4.0);
        // A rotated square: the rectangle follows it instead of the axis-aligned box around it.
        let (quad, side) = min_area_rect(vec![[0.0, 5.0], [5.0, 0.0], [10.0, 5.0], [5.0, 10.0]]).unwrap();
        assert!((side - 50f32.sqrt()).abs() < 1e-3, "{side}");
        assert!(quad.iter().all(|p| [[0.0, 5.0], [5.0, 0.0], [10.0, 5.0], [5.0, 10.0]].iter().any(|c| distance(*c, *p) < 1e-3)), "{quad:?}");
        assert_eq!(min_area_rect(vec![[1.0, 1.0], [4.0, 1.0]]).unwrap().1, 0.0);
        assert!(min_area_rect(Vec::new()).is_none());
    }

    fn map_with(rects: &[(usize, usize, usize, usize)], width: usize, height: usize) -> Vec<f32> {
        let mut map = vec![0.02; width * height];
        for &(x0, y0, x1, y1) in rects {
            for y in y0..y1 {
                for x in x0..x1 {
                    map[y * width + x] = 0.9;
                }
            }
        }
        map
    }

    #[test]
    fn db_boxes_are_found_grown_and_specks_dropped() {
        // Two text lines and a speck.
        let map = map_with(&[(10, 10, 110, 22), (20, 40, 80, 50), (150, 5, 152, 7)], 200, 60);
        let boxes = boxes_from_map(&map, 200, 60, &DbParams::default());
        assert_eq!(boxes.len(), 2, "{boxes:?}");
        let first = boxes.iter().find(|b| b[0][1] < 20.0).unwrap();
        // Grown by area·1.6/perimeter ≈ 7.9 px around the 99×11 core (pixel centres).
        assert!(first[0][0] < 10.0 && first[0][0] > 1.0 && first[2][0] > 109.0 && first[2][0] < 118.0, "{first:?}");
        assert!(first[0][1] < 10.0 && first[2][1] > 21.0, "{first:?}");
        // A box with a weak mean score is not text.
        let mut weak = vec![0.02; 200 * 60];
        for v in weak.iter_mut().take(200 * 20).skip(200 * 8) { *v = 0.35; }
        assert!(boxes_from_map(&weak, 200, 60, &DbParams::default()).is_empty());
    }

    #[test]
    fn frame_boxes_scale_clip_and_read_rows_left_to_right() {
        let q = |x: f32, y: f32| [[x, y], [x + 40.0, y], [x + 40.0, y + 10.0], [x, y + 10.0]];
        let boxes = vec![q(100.0, 21.0), q(10.0, 20.0), q(10.0, 60.0), q(150.0, 25.0), [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]];
        let out = frame_boxes(boxes, (200, 100), (400, 200));
        assert_eq!(out.len(), 4, "the 2×2 sliver is dropped");
        let xs: Vec<f32> = out.iter().map(|b| b[0][0]).collect();
        assert_eq!(xs, [20.0, 200.0, 300.0, 20.0], "one row (y within 10 px) is read left to right, then the next row");
        assert!(out.iter().flatten().all(|p| p[0] <= 399.0 && p[1] <= 199.0));
    }

    #[test]
    fn crops_follow_the_box_and_tall_boxes_are_turned() {
        let img = RgbImage::from_fn(100, 60, |x, y| image::Rgb([x as u8, y as u8, 0]));
        let c = crop(&img, &[[10.0, 20.0], [50.0, 20.0], [50.0, 30.0], [10.0, 30.0]]).unwrap();
        assert_eq!(c.dimensions(), (40, 10));
        assert_eq!(c.get_pixel(0, 0).0, [10, 20, 0]);
        assert_eq!(c.get_pixel(39, 9).0, [49, 29, 0]);
        let tall = crop(&img, &[[10.0, 5.0], [20.0, 5.0], [20.0, 45.0], [10.0, 45.0]]).unwrap();
        assert_eq!(tall.dimensions(), (40, 10), "a vertical line is read turned");
        assert!(crop(&img, &[[1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0]]).is_none());
    }

    #[test]
    fn pieces_become_lines_with_rectangles_and_per_character_confidence() {
        let piece = |y: f32, text: &str, confidence: f32| Piece { quad: [[10.0, y], [110.0, y], [110.0, y + 20.0], [10.0, y + 20.0]], text: text.into(), confidence, chars: text.chars().count() };
        let r = to_result(vec![piece(0.0, "Hello", 0.9), piece(30.0, "  ", 0.99), piece(40.0, "Hi", 0.6)]);
        assert_eq!(r.text, "Hello\nHi");
        assert_eq!(r.engine, ENGINE);
        assert_eq!(r.lines[0].rect, CropRect::in_space(10.0, 0.0, 100.0, 20.0));
        assert!((r.lines[0].confidence - 90.0).abs() < 1e-3 && (r.lines[1].confidence - 60.0).abs() < 1e-3);
        // (5 × 0.9 + 2 × 0.6) / 7: a long sure line weighs more than a short unsure one.
        assert!((r.confidence.unwrap() - 570.0 / 7.0).abs() < 1e-3, "{:?}", r.confidence);
        let empty = to_result(Vec::new());
        assert!(empty.text.is_empty() && empty.confidence.is_none());
    }

    #[test]
    fn threads_leave_the_game_its_cores() {
        let auto = effective_threads(0);
        assert!((1..=4).contains(&auto));
        assert_eq!(effective_threads(3), 3);
        assert_eq!(effective_threads(500), MAX_THREADS);
    }

    #[test]
    fn library_paths_are_absolute_and_missing_ones_explained() {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("libonnxruntime.so.1");
        std::fs::write(&lib, b"not really").unwrap();
        assert_eq!(library_path(Some(&lib)).unwrap(), lib.canonicalize().unwrap());
        let error = library_path(Some(&dir.path().join("missing.so"))).unwrap_err();
        assert!(error.contains("missing.so") && error.contains(INSTALL_COMMAND), "{error}");
    }

    /// A minimal ONNX writer (protobuf by hand) for models that stand in for PP-OCRv5 in tests: nothing is downloaded.
    mod onnx {
        fn varint(mut v: u64, out: &mut Vec<u8>) {
            loop {
                let byte = (v & 0x7f) as u8;
                v >>= 7;
                if v == 0 { out.push(byte); break; }
                out.push(byte | 0x80);
            }
        }
        fn int(field: u64, v: i64, out: &mut Vec<u8>) {
            varint(field << 3, out);
            varint(v as u64, out);
        }
        fn bytes(field: u64, data: &[u8], out: &mut Vec<u8>) {
            varint((field << 3) | 2, out);
            varint(data.len() as u64, out);
            out.extend_from_slice(data);
        }
        fn message(field: u64, out: &mut Vec<u8>, build: impl FnOnce(&mut Vec<u8>)) {
            let mut m = Vec::new();
            build(&mut m);
            bytes(field, &m, out);
        }
        pub enum Dim { Value(i64), Param(&'static str) }
        pub struct Node { pub op: &'static str, pub inputs: Vec<&'static str>, pub output: &'static str, pub ints: Vec<(&'static str, Vec<i64>)>, pub int: Vec<(&'static str, i64)> }

        /// One float input `x` of `dims`, one float output `y`, opset 13; `constants` are int64 initializers.
        pub fn model(nodes: &[Node], constants: &[(&'static str, Vec<i64>)], dims: &[Dim]) -> Vec<u8> {
            let mut out = Vec::new();
            int(1, 7, &mut out); // ir_version
            message(8, &mut out, |o| int(2, 13, o)); // opset_import { version: 13 }
            message(7, &mut out, |g| {
                for node in nodes {
                    message(1, g, |n| {
                        for input in &node.inputs { bytes(1, input.as_bytes(), n); }
                        bytes(2, node.output.as_bytes(), n);
                        bytes(4, node.op.as_bytes(), n);
                        for (name, values) in &node.ints {
                            message(5, n, |a| { bytes(1, name.as_bytes(), a); for v in values { int(8, *v, a); } int(20, 7, a); });
                        }
                        for (name, value) in &node.int {
                            message(5, n, |a| { bytes(1, name.as_bytes(), a); int(3, *value, a); int(20, 2, a); });
                        }
                    });
                }
                bytes(2, b"test", g);
                for (name, values) in constants {
                    message(5, g, |t| { int(1, values.len() as i64, t); int(2, 7, t); for v in values { int(7, *v, t); } bytes(8, name.as_bytes(), t); });
                }
                message(11, g, |v| {
                    bytes(1, b"x", v);
                    message(2, v, |t| message(1, t, |tt| {
                        int(1, 1, tt);
                        message(2, tt, |s| for d in dims {
                            message(1, s, |dd| match d { Dim::Value(n) => int(1, *n, dd), Dim::Param(p) => bytes(2, p.as_bytes(), dd) });
                        });
                    }));
                });
                message(12, g, |v| { bytes(1, b"y", v); message(2, v, |t| message(1, t, |tt| int(1, 1, tt))); });
            });
            out
        }
    }

    /// Detector: max over the channels through a sigmoid (green text on black → a map of ~0.9 on ~0.14). Classifier:
    /// the green and red channel means (in BGR order), so a green line is «upright». Recognizer: per column the three channel means, so a green column is class 1.
    fn fake_model(root: &Path) -> OcrModel {
        use onnx::{Dim::*, Node, model};
        let node = |op, inputs: Vec<&'static str>, output, ints: Vec<(&'static str, Vec<i64>)>, int: Vec<(&'static str, i64)>| Node { op, inputs, output, ints, int };
        let det = model(&[node("ReduceMax", vec!["x"], "m", vec![("axes", vec![1])], vec![("keepdims", 1)]), node("Sigmoid", vec!["m"], "y", vec![], vec![])],
            &[], &[Value(1), Value(3), Param("h"), Param("w")]);
        let cls = model(&[node("ReduceMean", vec!["x"], "m", vec![("axes", vec![2, 3])], vec![("keepdims", 0)]), node("Slice", vec!["m", "starts", "ends", "axes"], "y", vec![], vec![])],
            &[("starts", vec![1]), ("ends", vec![3]), ("axes", vec![1])], &[Param("n"), Value(3), Value(80), Value(160)]);
        let rec = model(&[node("ReduceMean", vec!["x"], "m", vec![("axes", vec![2])], vec![("keepdims", 0)]), node("Transpose", vec!["m"], "y", vec![("perm", vec![0, 2, 1])], vec![])],
            &[], &[Param("n"), Value(3), Value(48), Param("w")]);
        use sha2::Digest;
        let dir = root.join("fake-mobile");
        std::fs::create_dir_all(&dir).unwrap();
        let files = [("det", det), ("cls", cls), ("rec", rec), ("dict", b"A\n".to_vec())].into_iter().map(|(role, data)| {
            let name = format!("{role}.bin");
            std::fs::write(dir.join(&name), &data).unwrap();
            (role.to_owned(), rapid_models::ModelFile { name, size: data.len() as u64, sha256: format!("{:x}", sha2::Sha256::digest(&data)), url: String::new() })
        }).collect();
        OcrModel { id: "fake-mobile".into(), script: "fake".into(), variant: "mobile".into(), label: "fake".into(), languages: vec![], version: "test".into(), license: "test".into(), files }
    }

    /// The whole pipeline through the system ONNX Runtime with stand-in models: library loading, sessions with the
    /// thread settings, tensors in and out, boxes, crops, orientation, batching and CTC. Skipped without the library.
    #[test]
    fn the_pipeline_runs_through_onnx_runtime_with_stand_in_models() {
        if library_path(None).is_err() {
            eprintln!("skipped: no ONNX Runtime library");
            return;
        }
        init_runtime(None).unwrap();
        let root = tempfile::tempdir().unwrap();
        let model = fake_model(root.path());
        Engine::check(&model, root.path()).unwrap();
        let mut engine = Engine::load(&model, root.path(), 2, false).unwrap();
        assert_eq!(engine.cls_size, (160, 80), "the classifier's fixed input is read from the model");
        // Two «lines» of green on black, and one long one (a batch wider than 320 px).
        let mut img = RgbImage::from_pixel(1280, 200, image::Rgb([0, 0, 0]));
        for (x0, y0, x1, y1) in [(40, 20, 240, 50), (300, 24, 420, 52), (40, 120, 1200, 160)] {
            for y in y0..y1 { for x in x0..x1 { img.put_pixel(x, y, image::Rgb([0, 255, 0])); } }
        }
        let pieces = engine.read(&img).unwrap();
        assert_eq!(pieces.len(), 3, "{pieces:?}");
        assert!(pieces.iter().all(|p| p.text == "A" && p.chars == 1), "{pieces:?}");
        // Reading order: the first row left to right, then the second.
        let xs: Vec<f32> = pieces.iter().map(|p| p.quad[0][0]).collect();
        assert!(xs[0] < xs[1] && pieces[2].quad[0][1] > pieces[1].quad[3][1], "{pieces:?}");
        // Boxes are in pixels of the frame and cover the green areas.
        let first = pieces[0].quad;
        assert!(first[0][0] <= 40.0 && first[0][1] <= 20.0 && first[2][0] >= 239.0 && first[2][1] >= 49.0, "{first:?}");
        // The grown box adds black above and below the green, so class 1 wins by less than softmax(−1, 1, −1) ≈ 0.79,
        // but more than chance.
        assert!(pieces.iter().all(|p| p.confidence > 1.0 / 3.0 && p.confidence < 0.79), "{pieces:?}");
        let result = to_result(pieces);
        assert_eq!(result.text, "A\nA\nA");
        // An empty frame reads as nothing, and a tiny one too.
        assert!(engine.read(&RgbImage::new(640, 120)).unwrap().is_empty());
        assert!(engine.read(&RgbImage::new(3, 3)).unwrap().is_empty());
        // GPU asked for: with a CPU build of ONNX Runtime (or a GPU it cannot use) the models load on the CPU.
        let mut on_gpu = Engine::load(&model, root.path(), 1, true).unwrap();
        assert_eq!(on_gpu.read(&img).unwrap().len(), 3);
    }

    #[tokio::test]
    async fn a_missing_model_is_a_setup_error_with_the_action() {
        let root = tempfile::tempdir().unwrap();
        let ocr = RapidOcr::new(root.path().into(), Some(root.path().join("no-library.so")));
        let mut settings = Settings::default();
        settings.recognition.language = "rus+eng".into();
        let error = ocr.recognize_detailed(&DynamicImage::new_rgb8(64, 32), &settings).await.unwrap_err();
        // The model is checked before the library: the user is told what to download, whatever else is missing.
        assert!(matches!(&error, OcrError::Setup(m) if m.contains("eslav-mobile") && m.contains("Скачать")), "{error}");
        settings.recognition.language = "klingon".into();
        let error = ocr.recognize_detailed(&DynamicImage::new_rgb8(64, 32), &settings).await.unwrap_err();
        assert!(matches!(&error, OcrError::Setup(m) if m.contains("klingon")), "{error}");
    }
}
