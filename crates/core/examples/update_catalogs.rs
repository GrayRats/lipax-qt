//! Refreshes the bundled model catalogs. Developer tool, run by hand; the application never runs it.
//!
//!   cargo run -p lipa-core --example update_catalogs -- bergamot
//!   cargo run -p lipa-core --example update_catalogs -- ocr [--listing FILE]
//!
//! Only metadata is read (file lists with sizes and SHA-256); no model is downloaded. `--listing` reads a saved answer of
//! the ModelScope files API instead of asking the network.
use serde::{Serialize, Serializer, ser::SerializeMap};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Serialize)]
struct Entry {
    name: String,
    size: u64,
    sha256: String,
    url: String,
}

/// A JSON object that keeps the order of its entries (the order of the roles in the catalog).
struct Ordered(Vec<(&'static str, Entry)>);
impl Serialize for Ordered {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (role, entry) in &self.0 {
            map.serialize_entry(role, entry)?;
        }
        map.end()
    }
}

#[derive(Serialize)]
struct OcrModel {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine: Option<&'static str>,
    script: &'static str,
    variant: &'static str,
    label: &'static str,
    languages: Vec<&'static str>,
    version: String,
    license: &'static str,
    files: Ordered,
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder().user_agent(concat!("LipaX/", env!("CARGO_PKG_VERSION"), " catalog-update")).timeout(std::time::Duration::from_secs(60)).build()?)
}

fn write(relative: &str, json: &impl Serialize, count: usize) -> Result<()> {
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::write(&target, serde_json::to_string_pretty(json)? + "\n")?;
    println!("Wrote {count} model sets to {}", target.display());
    Ok(())
}

// --- Bergamot (Mozilla Remote Settings) ---

const BERGAMOT_ENDPOINT: &str = "https://firefox.settings.services.mozilla.com/v1/buckets/main/collections/translations-models/records";
const BERGAMOT_CDN: &str = "https://firefox-settings-attachments.cdn.mozilla.net/";

#[derive(Serialize)]
struct BergamotSet {
    pair: String,
    version: String,
    files: BTreeMap<String, Entry>,
}

fn version_key(version: &str) -> Vec<u64> {
    version.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

async fn bergamot() -> Result<()> {
    let records: Value = client()?.get(BERGAMOT_ENDPOINT).send().await?.error_for_status()?.json().await?;
    let mut groups: BTreeMap<(String, String, String), BTreeMap<String, Entry>> = BTreeMap::new();
    for record in records["data"].as_array().ok_or("no records")? {
        let text = |key: &str| record[key].as_str().map(str::to_owned).ok_or(format!("record without {key}"));
        let version = text("version")?;
        // Nightly / alpha models are not installed automatically.
        if !version.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit())) {
            continue;
        }
        let attachment = &record["attachment"];
        let entry = Entry {
            name: text("name")?,
            size: attachment["size"].as_u64().ok_or("record without size")?,
            sha256: attachment["hash"].as_str().ok_or("record without hash")?.to_owned(),
            url: format!("{BERGAMOT_CDN}{}", attachment["location"].as_str().ok_or("record without location")?),
        };
        groups.entry((text("fromLang")?, text("toLang")?, version)).or_default().insert(text("fileType")?, entry);
    }
    let mut sets: Vec<BergamotSet> = groups
        .into_iter()
        .filter(|(_, files)| files.contains_key("model") && files.contains_key("lex") && (files.contains_key("vocab") || (files.contains_key("srcvocab") && files.contains_key("trgvocab"))))
        .map(|((from, to, version), files)| BergamotSet { pair: format!("{from}-{to}"), version, files })
        .collect();
    if sets.is_empty() {
        return Err("Mozilla returned no complete stable model sets".into());
    }
    // Newest version first; the pairs keep a stable order within a version.
    sets.sort_by(|a, b| version_key(&b.version).cmp(&version_key(&a.version)).then_with(|| a.pair.cmp(&b.pair)));
    let count = sets.len();
    write("src/translate/bergamot_catalog.json", &sets, count)
}

// --- OCR models (RapidOCR on ModelScope, MeikiOCR on Hugging Face) ---

const RAPID_REPOSITORY: &str = "RapidAI/RapidOCR";
const RAPID_TAG: &str = "v3.10.0";

/// Script, variant, label, Tesseract codes. The recognizer of PP-OCRv5 `ch` reads simplified and traditional Chinese,
/// Japanese and English; the other scripts have a mobile recognizer only.
#[allow(clippy::type_complexity)]
fn rapid_sets() -> Vec<(&'static str, &'static str, &'static str, Vec<&'static str>)> {
    vec![
        ("en", "mobile", "английский", vec!["eng"]),
        ("latin", "mobile", "латиница (европейские языки)", vec![
            "deu", "fra", "spa", "ita", "por", "pol", "nld", "swe", "dan", "nor", "ces", "slk", "slv", "hrv", "hun", "tur",
            "ind", "msa", "est", "lit", "isl", "bos", "sqi", "afr", "lat", "cym", "gle"]),
        ("eslav", "mobile", "кириллица: русский, украинский, белорусский", vec!["rus", "ukr", "bel"]),
        ("cyrillic", "mobile", "кириллица: болгарский, сербский и др.", vec!["bul", "srp", "mkd", "kaz", "mon"]),
        ("ch", "mobile", "китайский и японский", vec!["chi_sim", "chi_tra", "jpn"]),
        ("ch", "server", "китайский и японский (server)", vec!["chi_sim", "chi_tra", "jpn"]),
        ("korean", "mobile", "корейский", vec!["kor"]),
        ("el", "mobile", "греческий", vec!["ell"]),
        ("th", "mobile", "тайский", vec!["tha"]),
        ("arabic", "mobile", "арабское письмо", vec!["ara", "fas", "urd"]),
        ("devanagari", "mobile", "деванагари", vec!["hin", "mar", "nep", "san"]),
        ("ta", "mobile", "тамильский", vec!["tam"]),
        ("te", "mobile", "телугу", vec!["tel"]),
    ]
}

/// MeikiOCR (rtr46): Japanese text of video games. The revisions pin the files: a later change of the repositories
/// cannot break the hashes. Update them, and the file names, when a newer set is adopted.
const MEIKI_DETECTOR: (&str, &str, &str) = ("rtr46/meiki.text.detect.v0", "a9cffa4f60cbf72ddb87edf19c6f98a01cd042e6", "meiki.text.detect.v0.1.960x544.onnx");
const MEIKI_RECOGNIZER: (&str, &str) = ("rtr46/meiki.txt.recognition.v0", "a28cf5874dc2438ebb1c86336be26bcec51e3375");
const MEIKI_RECOGNIZERS: [(&str, &str); 2] = [("rec", "meiki.text.rec.v0.960x32.onnx"), ("vrec", "meiki.text.rec.v0.vertical.32x480.onnx")];

fn modelscope_entry(files: &BTreeMap<String, Value>, path: &str) -> Result<Entry> {
    let file = files.get(path).ok_or(format!("{path}: not in the listing"))?;
    let (size, sha256) = (file["Size"].as_u64().unwrap_or(0), file["Sha256"].as_str().unwrap_or(""));
    if sha256.len() != 64 || size == 0 {
        return Err(format!("{path}: no size or SHA-256 in the listing").into());
    }
    let name = path.rsplit('/').next().unwrap_or(path).to_owned();
    Ok(Entry { name, size, sha256: sha256.into(), url: format!("https://www.modelscope.cn/models/{RAPID_REPOSITORY}/resolve/{RAPID_TAG}/{path}") })
}

/// The entry of a Hugging Face file from the tree API (LFS files carry their SHA-256 as `lfs.oid`).
fn hugging_face_entry(tree: &[Value], repository: &str, revision: &str, name: &str) -> Result<Entry> {
    let file = tree.iter().find(|f| f["path"] == name).ok_or(format!("{repository}: no {name}"))?;
    let (size, sha256) = (file["lfs"]["size"].as_u64().or(file["size"].as_u64()).unwrap_or(0), file["lfs"]["oid"].as_str().unwrap_or(""));
    if sha256.len() != 64 || size == 0 {
        return Err(format!("{repository}/{name}: no size or SHA-256 in the listing").into());
    }
    Ok(Entry { name: name.into(), size, sha256: sha256.into(), url: format!("https://huggingface.co/{repository}/resolve/{revision}/{name}") })
}

async fn hugging_face_tree(client: &reqwest::Client, repository: &str, revision: &str) -> Result<Vec<Value>> {
    let url = format!("https://huggingface.co/api/models/{repository}/tree/{revision}?recursive=1");
    Ok(client.get(url).send().await?.error_for_status()?.json().await?)
}

async fn ocr(listing: Option<PathBuf>) -> Result<()> {
    let client = client()?;
    let data: Value = match listing {
        Some(path) => serde_json::from_str(&std::fs::read_to_string(path)?)?,
        None => client.get(format!("https://www.modelscope.cn/api/v1/models/{RAPID_REPOSITORY}/repo/files?Revision={RAPID_TAG}&Recursive=true")).send().await?.error_for_status()?.json().await?,
    };
    let files: BTreeMap<String, Value> = data["Data"]["Files"].as_array().ok_or("no file list")?.iter()
        .filter(|f| f["Type"] == "blob").filter_map(|f| Some((f["Path"].as_str()?.to_owned(), f.clone()))).collect();
    let det = |variant: &str| format!("onnx/PP-OCRv5/det/ch_PP-OCRv5_det_{variant}.onnx");
    let cls = |variant: &str| if variant == "server" { "onnx/PP-OCRv5/cls/ch_PP-LCNet_x1_0_textline_ori_cls_server.onnx" } else { "onnx/PP-OCRv5/cls/ch_PP-LCNet_x0_25_textline_ori_cls_mobile.onnx" };
    let mut models = Vec::new();
    for (script, variant, label, languages) in rapid_sets() {
        let rec = format!("onnx/PP-OCRv5/rec/{script}_PP-OCRv5_rec_{variant}.onnx");
        let dictionary = format!("paddle/PP-OCRv5/rec/{script}_PP-OCRv5_rec_{variant}/{}", if script == "ch" { "ppocrv5_dict.txt".to_owned() } else { format!("ppocrv5_{script}_dict.txt") });
        models.push(OcrModel {
            id: format!("{script}-{variant}"), engine: None, script, variant, label, languages,
            version: format!("PP-OCRv5 ({RAPID_REPOSITORY} {RAPID_TAG})"), license: "Apache-2.0",
            files: Ordered(vec![
                ("det", modelscope_entry(&files, &det(variant))?),
                ("cls", modelscope_entry(&files, cls(variant))?),
                ("rec", modelscope_entry(&files, &rec)?),
                ("dict", modelscope_entry(&files, &dictionary)?),
            ]),
        });
    }
    let (det_repository, det_revision, det_name) = MEIKI_DETECTOR;
    let det_tree = hugging_face_tree(&client, det_repository, det_revision).await?;
    let (rec_repository, rec_revision) = MEIKI_RECOGNIZER;
    let rec_tree = hugging_face_tree(&client, rec_repository, rec_revision).await?;
    let mut meiki_files = vec![("det", hugging_face_entry(&det_tree, det_repository, det_revision, det_name)?)];
    for (role, name) in MEIKI_RECOGNIZERS {
        meiki_files.push((role, hugging_face_entry(&rec_tree, rec_repository, rec_revision, name)?));
    }
    models.push(OcrModel {
        id: "meiki-ja".into(), engine: Some("meikiocr"), script: "ja", variant: "v0.1", label: "японский: игры и визуальные новеллы",
        languages: vec!["jpn"],
        version: format!("detector v0.1 960×544 ({det_revision:.7}), recognizer v0 ({rec_revision:.7}), rtr46"),
        license: "LGPL-3.0", files: Ordered(meiki_files),
    });
    let count = models.len();
    write("src/ocr/ocr_models_catalog.json", &models, count)
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("bergamot") => bergamot().await,
        Some("ocr") => {
            let listing = match args.next().as_deref() {
                Some("--listing") => Some(PathBuf::from(args.next().ok_or("--listing needs a file")?)),
                Some(other) => return Err(format!("unknown argument {other}").into()),
                None => None,
            };
            ocr(listing).await
        }
        _ => Err("usage: update_catalogs bergamot | ocr [--listing FILE]".into()),
    }
}
