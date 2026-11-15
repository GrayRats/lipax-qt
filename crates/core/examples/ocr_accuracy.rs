//! Recognition and overlay on real game frames, measured against a hand-made transcription and the hand-made
//! reference translations. A measuring tool for the maintainer: nothing here is part of the application.
//!
//! Every frame goes through the path of the application: block detector → RapidOCR per field (`read_field`, with its
//! second chances) → `observe_ocr_result` / `complete` → rescans until the fields settle → placement
//! (`place_regions` with approximate text metrics instead of Qt).
//!
//! Text (`tests/inplace/fixtures/ground-truth.json`, `{"dialogue-en-3": [{"class": "name", "text": "…"}, …]}`, classes
//! `dialogue`, `name`, `choice`, `ui`; an entry with `"unsure": true` is not measured):
//! - `cer`: the character error rate of an expected text against the closest part of what was read (edit distance to
//!   the closest substring over the length of the expected text; case, spacing and quote style ignored). Both a single
//!   field and all the fields in reading order are tried: the second forgives a text cut into two fields, which is then
//!   counted in `split`;
//! - `missed`: expected texts with `cer > 0.3` (not found, or read too badly to translate);
//! - `split`: expected texts that only read well across several fields;
//! - `merged`: fields that hold expected texts of different classes (a name glued to its dialogue);
//! - `spurious`: fields whose reading is in no expected text (scenery taken for text, garbage).
//!
//! Overlay:
//! - `cover`: the share of a field's source rectangle under its translation plate;
//! - `recall`: the share of the pixels the reference translation changed that lie under our plates;
//! - `excess`: the share of our plates' area farther than `NEAR` px from anything the reference changed.
//!   Plates are rectangles here (no Qt rendering): the measure is of geometry, not of pixels drawn.
//!
//! Frames 0–9 are the working set, 10–15 the control set (never used for tuning); the summary is per set.
//!
//!   LIPAX_RAPID_TEST_MODELS=~/.local/share/LipaX/ocr-models \
//!   cargo run --release -p lipa-core --example ocr_accuracy -- --out=tmp/acc/<run> [--frames=0,3,10-12] [--lang=eng]
//!
//! Sources are `docs/screenshots/test-game/dialogue-en-N.*` and `Эталоны/dialogue-ru-N.*`; ImageMagick (`magick`)
//! converts them to opaque PNG under `tmp/acc/frames` and `tmp/acc/references` once (this build of the `image` crate
//! reads no JPEG). `<out>/report.json` holds everything but the timings (`<out>/timing.json`), so two runs can be
//! compared line by line; `<out>/<frame>.png` shows the fields (green: shown, red: rejected) and the plates (blue).
#[path = "../tests/common/reference.rs"]
mod reference;

use lipa_core::layout::{
    engine::{InplaceBlock, InplaceEngine, LinesVerdict},
    fit::ApproxMeasure,
    font_database::InstalledFontDatabase,
    place::{PlacementCache, RegionInput, Viewport, place_regions},
};
use lipa_core::{
    capture::kwin::WindowGeometry,
    ocr::{rapid::RapidOcr, rapid_models},
    settings::{NormRect, Settings},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

const CLASSES: [&str; 4] = ["dialogue", "name", "choice", "ui"];
/// An expected text read worse than this is missed.
const MISSED: f32 = 0.3;
/// A field whose reading is farther than this from every part of the expected texts is spurious.
const SPURIOUS: f32 = 0.5;
/// The first frame of the control set.
const CONTROL_FROM: u32 = 10;

#[derive(serde::Deserialize)]
struct Expected {
    class: String,
    text: String,
    #[serde(default)]
    unsure: bool,
}

fn normal(text: &str) -> Vec<char> {
    text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase().chars()
        .map(|c| match c { '’' | '‘' | '`' => '\'', '“' | '”' => '"', '…' => '.', '—' | '–' => '-', other => other }).collect()
}

/// Edit distance of `needle` to the closest substring of `hay` (semi-global alignment), over the length of `needle`.
fn substring_cer(needle: &[char], hay: &[char]) -> f32 {
    if needle.is_empty() { return 0.0; }
    let mut previous: Vec<usize> = vec![0; hay.len() + 1];
    for (i, &a) in needle.iter().enumerate() {
        let mut row = vec![i + 1; hay.len() + 1];
        for (j, &b) in hay.iter().enumerate() {
            row[j + 1] = (previous[j] + usize::from(a != b)).min(previous[j + 1] + 1).min(row[j] + 1);
        }
        previous = row;
    }
    (*previous.iter().min().unwrap() as f32 / needle.len() as f32).min(1.0)
}

/// `source` as an opaque PNG in `dir` (made by ImageMagick once, transparency over black as the capture gives it).
fn as_png(source: &Path, dir: &Path, stem: &str) -> PathBuf {
    std::fs::create_dir_all(dir).expect("tmp directory");
    let target = dir.join(format!("{stem}.png"));
    let fresh = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    if fresh(&target).is_none() || fresh(&target) < fresh(source) {
        let status = std::process::Command::new("magick").arg(source).args(["-background", "black", "-alpha", "remove", "-alpha", "off"]).arg(&target)
            .status().expect("ImageMagick (magick) converts the frames");
        assert!(status.success(), "magick failed on {}", source.display());
    }
    target
}

/// The file `<prefix><n>.*` of `dir`, whatever its extension.
fn numbered(dir: &Path, prefix: &str, n: u32) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok()).map(|e| e.path())
        .find(|p| p.is_file() && p.file_stem().is_some_and(|s| s.to_string_lossy() == format!("{prefix}{n}")))
}

fn frames_wanted(arg: Option<&str>) -> Vec<u32> {
    let Some(arg) = arg else { return (0..=15).collect() };
    arg.split(',').flat_map(|part| match part.split_once('-') {
        Some((a, b)) => (a.parse::<u32>().expect("frame number")..=b.parse().expect("frame number")).collect::<Vec<_>>(),
        None => vec![part.parse().expect("frame number")],
    }).collect()
}

fn round(v: f32) -> Value { json!((v as f64 * 1000.0).round() / 1000.0) }

struct Rejected { rect: [f32; 4], text: String }

/// One frame through the engine with real OCR: the final frame of fields, what was rejected, and the timings (ms).
async fn run_frame(image: &image::RgbaImage, rapid: &RapidOcr, settings: &Settings) -> (lipa_core::layout::engine::InplaceFrame, Vec<Rejected>, Value) {
    let mut engine = InplaceEngine::with_fonts(InstalledFontDatabase::bundled());
    let now = Instant::now();
    let started = Instant::now();
    let mut rejected: HashMap<u64, Rejected> = HashMap::new();
    let mut frame = None;
    let (mut begin_ms, mut ocr_ms, mut scans) = (Vec::new(), 0.0f64, 0);
    let lap = Instant::now();
    let mut jobs = engine.begin(image::DynamicImage::ImageRgba8(image.clone()), settings, now, false);
    begin_ms.push(lap.elapsed().as_secs_f64() * 1000.0);
    for scan in 1..=4u64 {
        scans += 1;
        for job in jobs {
            let lap = Instant::now();
            let result = lipa_core::pipeline::read_field(rapid, &job.image, settings).await.expect("RapidOCR");
            ocr_ms += lap.elapsed().as_secs_f64() * 1000.0;
            let result = lipa_core::ocr::filter::clean(result, settings.effective_minimum_confidence(), settings.recognition.filter_noise);
            let raw = lipa_core::text::normalize(&result.text);
            let confident = result.confidence.is_none_or(|c| c >= settings.effective_minimum_confidence() as f32) && lipa_core::text::is_meaningful(&raw);
            let observation = confident.then(|| engine.observe_ocr_result(job.id, &result, job.origin, settings));
            let text = observation.as_ref().map_or(raw, |o| o.text.clone());
            let split = observation.is_some_and(|o| o.verdict == LinesVerdict::Split);
            let accepted = confident && lipa_core::text::is_meaningful(&text);
            if !accepted {
                engine.reject_ocr(job.id, settings);
                rejected.insert(job.id, Rejected { rect: [job.rect.x, job.rect.y, job.rect.w, job.rect.h], text });
            } else if !split {
                engine.complete(job.id, Some((text.clone(), text)), settings);
                rejected.remove(&job.id);
            }
        }
        if let Some(f) = engine.finish(settings) { frame = Some(f); }
        if !engine.needs_rescan() { break; }
        let lap = Instant::now();
        jobs = engine.begin(image::DynamicImage::ImageRgba8(image.clone()), settings, now + std::time::Duration::from_millis(100 * scan), false);
        begin_ms.push(lap.elapsed().as_secs_f64() * 1000.0);
    }
    let frame = frame.unwrap_or(lipa_core::layout::engine::InplaceFrame { frame: image.dimensions(), capture: None, blocks: Vec::new(), undrawable: Vec::new(), new_translations: Vec::new() });
    let mut rejected: Vec<Rejected> = rejected.into_iter().filter(|(id, _)| !frame.blocks.iter().any(|b| b.id == *id)).map(|(_, r)| r).collect();
    rejected.sort_by(|a, b| (a.rect[1], a.rect[0]).partial_cmp(&(b.rect[1], b.rect[0])).unwrap());
    let timing = json!({"total_ms": started.elapsed().as_secs_f64() * 1000.0, "begin_ms": begin_ms, "ocr_ms": ocr_ms, "scans": scans});
    (frame, rejected, timing)
}

#[derive(Default, Clone)]
struct Tally { cer: f32, expected: usize, missed: usize, split: usize }

#[derive(Default)]
struct SetTotals {
    frames: usize,
    classes: BTreeMap<String, Tally>,
    merged: usize,
    spurious: usize,
    fields: usize,
    cover: f32,
    covered_fields: usize,
    recall: f32,
    excess: f32,
    referenced: usize,
    millis: f64,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let option = |name: &str| args.iter().find_map(|a| a.strip_prefix(name));
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = option("--source=").map(PathBuf::from).unwrap_or_else(|| root.join("docs/screenshots/test-game"));
    let truth_path = option("--truth=").map(PathBuf::from).unwrap_or_else(|| root.join("tests/inplace/fixtures/ground-truth.json"));
    let truth: HashMap<String, Vec<Expected>> = std::fs::read(&truth_path).ok()
        .map(|bytes| serde_json::from_slice(&bytes).expect("ground truth JSON")).unwrap_or_default();
    let out = PathBuf::from(option("--out=").expect("--out=tmp/acc/<run>"));
    std::fs::create_dir_all(&out).expect("output directory");
    let acc = root.join("tmp/acc");
    let mut settings = Settings::default();
    settings.recognition.engine = "rapidocr".into();
    settings.recognition.language = option("--lang=").unwrap_or("eng").into();
    settings.recognition.rapid_threads = 4;
    settings.recognition.rapid_use_gpu = false;
    let rapid = RapidOcr::new(
        std::env::var_os("LIPAX_RAPID_TEST_MODELS").map(PathBuf::from).unwrap_or_else(rapid_models::cache_root),
        std::env::var_os("LIPAX_RAPID_TEST_ORT").map(PathBuf::from),
    );
    let mut report = BTreeMap::new();
    let mut timings = BTreeMap::new();
    let mut sets: BTreeMap<&str, SetTotals> = BTreeMap::new();
    for n in frames_wanted(option("--frames=")) {
        let Some(original) = numbered(&source, "dialogue-en-", n) else { eprintln!("frame {n}: no source"); continue };
        let stem = format!("dialogue-en-{n}");
        let image = image::open(as_png(&original, &acc.join("frames"), &stem)).expect("frame").into_rgba8();
        let (fw, fh) = (image.width() as usize, image.height() as usize);
        let (frame, rejected, timing) = run_frame(&image, &rapid, &settings).await;
        let mut shown: Vec<&InplaceBlock> = frame.blocks.iter().collect();
        shown.sort_by(|a, b| (a.text_rect.y, a.text_rect.x).partial_cmp(&(b.text_rect.y, b.text_rect.x)).unwrap());
        // Placement over the whole frame, as the application places it.
        let region = RegionInput { id: "frame", rect: NormRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }, frame: &frame };
        let view = Viewport::new(WindowGeometry::from([0.0, 0.0, fw as f64, fh as f64]), Vec::new());
        let placed = place_regions(&[region], &view, &settings.appearance.inplace, &HashMap::new(), &ApproxMeasure, &mut PlacementCache::default());
        let plate_of = |b: &InplaceBlock| placed.placed.iter().find(|p| p.block_id == b.id)
            // `box_rect` is the plate in fractions of the region frame; the region is the whole frame here.
            .map(|p| [p.box_rect[0] * fw as f64, p.box_rect[1] * fh as f64, p.box_rect[2] * fw as f64, p.box_rect[3] * fh as f64].map(|v| v as f32));
        let set = if n >= CONTROL_FROM { "control" } else { "work" };
        let totals = sets.entry(set).or_default();
        totals.frames += 1;
        totals.millis += timing["total_ms"].as_f64().unwrap_or(0.0);
        let mut fields_json = Vec::new();
        let mut plates: Vec<[f32; 4]> = Vec::new();
        for b in &shown {
            let plate = plate_of(b);
            let cover = plate.map(|[px, py, pw, ph]| {
                let r = b.text_rect;
                let (x0, y0, x1, y1) = (r.x.max(px), r.y.max(py), r.right().min(px + pw), r.bottom().min(py + ph));
                (x1 - x0).max(0.0) * (y1 - y0).max(0.0) / (r.w * r.h).max(1.0)
            });
            if let Some(c) = cover { totals.cover += c; totals.covered_fields += 1; }
            plates.extend(plate);
            let r = b.text_rect;
            fields_json.push(json!({"rect": [r.x.round(), r.y.round(), r.w.round(), r.h.round()], "type": format!("{:?}", b.block_type), "text": b.original,
                "plate": plate.map(|p| p.map(|v| v.round())), "cover": cover.map(round), "background": format!("{:?}", b.background.mode)}));
        }
        totals.fields += shown.len();
        // Text against the transcription.
        let mut truth_json = Vec::new();
        let (mut spurious, mut merged) = (Vec::new(), Vec::new());
        if let Some(expected) = truth.get(&stem) {
            let readings: Vec<Vec<char>> = shown.iter().map(|b| normal(&b.original)).collect();
            let joined: Vec<char> = normal(&shown.iter().map(|b| b.original.as_str()).collect::<Vec<_>>().join(" "));
            for e in expected {
                let want = normal(&e.text);
                let single = readings.iter().map(|r| substring_cer(&want, r)).fold(1.0f32, f32::min);
                let across = substring_cer(&want, &joined);
                let cer = single.min(across);
                let split = single > MISSED && across <= MISSED;
                if !e.unsure {
                    let tally = totals.classes.entry(e.class.clone()).or_default();
                    tally.cer += cer;
                    tally.expected += 1;
                    tally.missed += usize::from(cer > MISSED);
                    tally.split += usize::from(split);
                }
                truth_json.push(json!({"class": e.class, "text": e.text, "unsure": e.unsure, "cer": round(cer), "missed": cer > MISSED, "split": split}));
            }
            let everything: Vec<char> = normal(&expected.iter().map(|e| e.text.as_str()).collect::<Vec<_>>().join(" | "));
            // The field an expected text belongs to: the best reading of it, the shortest among equals (a name that also
            // occurs inside the dialogue belongs to the name field).
            let mut held: Vec<Vec<&str>> = vec![Vec::new(); shown.len()];
            for e in expected.iter().filter(|e| !e.unsure) {
                let want = normal(&e.text);
                let best = readings.iter().enumerate().map(|(i, r)| (substring_cer(&want, r), r.len(), i))
                    .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                if let Some((cer, _, i)) = best && cer <= MISSED { held[i].push(e.class.as_str()); }
            }
            for ((b, reading), mut classes) in shown.iter().zip(&readings).zip(held) {
                if substring_cer(reading, &everything) > SPURIOUS { spurious.push(b.original.clone()); }
                classes.sort_unstable();
                classes.dedup();
                if classes.len() > 1 { merged.push(json!({"text": b.original, "classes": classes})); }
            }
            totals.spurious += spurious.len();
            totals.merged += merged.len();
        }
        // Plates against the reference translation.
        let mut overlay = Value::Null;
        if let Some(reference) = numbered(&source.join("Эталоны"), "dialogue-ru-", n) {
            let reference = image::open(as_png(&reference, &acc.join("references"), &format!("dialogue-ru-{n}"))).expect("reference").into_rgb8();
            let original_rgb = image::DynamicImage::ImageRgba8(image.clone()).into_rgb8();
            if reference.dimensions() == original_rgb.dimensions() {
                let changed = reference::changed_mask(&original_rgb, &reference);
                // Specks of JPEG noise are not text: only the blocks the measure itself would count.
                let mut text = vec![false; fw * fh];
                for block in reference::blocks(&changed, fw, fh) { for &i in &block.pixels { text[i as usize] = true; } }
                let near = reference::dilate(&text, fw, fh, reference::NEAR);
                let mut plate_mask = vec![false; fw * fh];
                for [px, py, pw, ph] in &plates {
                    let (x0, y0) = (px.max(0.0) as usize, py.max(0.0) as usize);
                    let (x1, y1) = (((px + pw).ceil() as usize).min(fw), ((py + ph).ceil() as usize).min(fh));
                    for y in y0..y1 { plate_mask[y * fw + x0..y * fw + x1.max(x0)].fill(true); }
                }
                let reference_pixels = text.iter().filter(|&&t| t).count();
                let covered = text.iter().zip(&plate_mask).filter(|(t, p)| **t && **p).count();
                let plate_pixels = plate_mask.iter().filter(|&&p| p).count();
                let beyond = plate_mask.iter().zip(&near).filter(|(p, n)| **p && !**n).count();
                let recall = covered as f32 / reference_pixels.max(1) as f32;
                let excess = beyond as f32 / plate_pixels.max(1) as f32;
                totals.recall += recall;
                totals.excess += excess;
                totals.referenced += 1;
                overlay = json!({"recall": round(recall), "excess": round(excess), "reference_pixels": reference_pixels});
            } else {
                overlay = json!({"skipped": "the reference has another size"});
            }
        }
        // The picture: fields, rejected fields and plates.
        let mut canvas = image.clone();
        let mut outline = |r: [f32; 4], colour: [u8; 4]| {
            let (x0, y0) = (r[0].max(0.0) as u32, r[1].max(0.0) as u32);
            let (x1, y1) = (((r[0] + r[2]) as u32).min(canvas.width() - 1), ((r[1] + r[3]) as u32).min(canvas.height() - 1));
            if x1 <= x0 || y1 <= y0 { return; }
            for t in 0..2 {
                for x in x0..=x1 { for y in [y0 + t, y1 - t] { canvas.put_pixel(x, y, image::Rgba(colour)); } }
                for y in y0..=y1 { for x in [x0 + t, x1 - t] { canvas.put_pixel(x, y, image::Rgba(colour)); } }
            }
        };
        for p in &plates { outline(*p, [60, 120, 255, 255]); }
        for r in &rejected { outline(r.rect, [255, 40, 40, 255]); }
        for b in &shown { let r = b.text_rect; outline([r.x, r.y, r.w, r.h], [40, 255, 40, 255]); }
        canvas.save(out.join(format!("{stem}.png"))).expect("picture");
        let missed = truth_json.iter().filter(|t| t["missed"] == true && t["unsure"] == false).count();
        println!("{stem} [{set}]: {} fields, {} rejected, missed {missed}, spurious {}, merged {}, overlay {overlay}, {:.0} ms",
            shown.len(), rejected.len(), spurious.len(), merged.len(), timing["total_ms"].as_f64().unwrap_or(0.0));
        report.insert(stem.clone(), json!({"set": set, "size": [fw, fh], "fields": fields_json,
            "rejected": rejected.iter().map(|r| json!({"rect": r.rect.map(|v| v.round()), "text": r.text})).collect::<Vec<_>>(),
            "truth": truth_json, "spurious": spurious, "merged": merged, "overlay": overlay,
            "dropped": placed.dropped.iter().map(|d| json!({"text": d.original, "reason": d.reason})).collect::<Vec<_>>()}));
        timings.insert(stem, timing);
    }
    let mut summary = BTreeMap::new();
    for (set, t) in &sets {
        let classes: BTreeMap<&str, Value> = CLASSES.iter().filter_map(|c| t.classes.get(*c).map(|k| (*c, json!({
            "expected": k.expected, "cer": round(k.cer / k.expected.max(1) as f32), "missed": k.missed, "split": k.split})))).collect();
        println!("\n{set}: {} frames, {} fields, spurious {}, merged {}, cover {:.1}%, recall {:.1}%, excess {:.1}% ({} with a reference), {:.1} s",
            t.frames, t.fields, t.spurious, t.merged, 100.0 * t.cover / t.covered_fields.max(1) as f32,
            100.0 * t.recall / t.referenced.max(1) as f32, 100.0 * t.excess / t.referenced.max(1) as f32, t.referenced, t.millis / 1000.0);
        for (class, v) in &classes { println!("  {class:9} expected {:>3}  CER {:.3}  missed {:>2}  split {:>2}", v["expected"], v["cer"].as_f64().unwrap(), v["missed"], v["split"]); }
        summary.insert(*set, json!({"frames": t.frames, "fields": t.fields, "classes": classes, "spurious": t.spurious, "merged": t.merged,
            "cover": round(t.cover / t.covered_fields.max(1) as f32), "recall": round(t.recall / t.referenced.max(1) as f32),
            "excess": round(t.excess / t.referenced.max(1) as f32), "referenced": t.referenced}));
    }
    let write = |name: &str, value: Value| std::fs::write(out.join(name), serde_json::to_string_pretty(&value).unwrap() + "\n").expect("report");
    write("report.json", json!({"frames": report, "summary": summary}));
    write("timing.json", json!(timings));
}
