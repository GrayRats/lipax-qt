//! Verified PP-OCRv5 models for RapidOCR: the bundled catalog (`ocr_models_catalog.json`, sizes and SHA-256 pinned to a
//! tag of the RapidAI repository), the choice of a model for a Tesseract language code, and the installation in
//! `~/.local/share/LipaX/ocr-models/<id>/`. Downloading happens only when the user asks for it in the settings; reading
//! text never touches the network. The same rules as for the Bergamot models (`translate::models`): files are written
//! as `.part` in a staging directory, checked against the catalog and only then moved into place, an old installation
//! is kept until the new one is committed.
use crate::translate::models::verify;
pub use crate::translate::models::ModelFile;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

/// The files of a model set: detector, text line orientation classifier, recognizer and its dictionary.
pub const ROLES: [&str; 4] = ["det", "cls", "rec", "dict"];

#[derive(Debug, Clone, Deserialize)]
pub struct OcrModel {
    pub id: String,
    /// `en`, `latin`, `eslav`, `ch`, …: the recognizer of PP-OCRv5.
    pub script: String,
    /// `mobile` (default) or `server`.
    pub variant: String,
    /// What the model reads, for the settings.
    pub label: String,
    /// Tesseract codes read by the recognizer.
    pub languages: Vec<String>,
    pub version: String,
    pub license: String,
    pub files: BTreeMap<String, ModelFile>,
}

impl OcrModel {
    pub fn size(&self) -> u64 {
        self.files.values().map(|f| f.size).sum()
    }

    pub fn dir(&self, root: &Path) -> PathBuf {
        root.join(&self.id)
    }

    pub fn file(&self, root: &Path, role: &str) -> PathBuf {
        self.dir(root).join(&self.files[role].name)
    }
}

pub fn catalog() -> &'static [OcrModel] {
    static CATALOG: std::sync::OnceLock<Vec<OcrModel>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| parse_catalog(include_str!("ocr_models_catalog.json")).expect("bundled OCR model catalog"))
}

pub fn parse_catalog(json: &str) -> Result<Vec<OcrModel>, String> {
    let models: Vec<OcrModel> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    for model in &models {
        let safe = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
        if !safe(&model.id) {
            return Err(format!("unsafe model id «{}»", model.id));
        }
        for role in ROLES {
            let file = model.files.get(role).ok_or_else(|| format!("{}: no «{role}» file", model.id))?;
            if Path::new(&file.name).components().count() != 1 || file.name.starts_with('.') || file.name.ends_with(".part") {
                return Err(format!("{}: unsafe file name «{}»", model.id, file.name));
            }
            if file.size == 0 || file.sha256.len() != 64 || !file.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(format!("{}: «{role}» has no size or SHA-256", model.id));
            }
        }
    }
    Ok(models)
}

pub fn cache_root() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("LipaX/ocr-models")
}

/// The script (recognizer) of a Tesseract language code, `None` if no model reads it.
pub fn script(language: &str) -> Option<&'static str> {
    catalog().iter().find(|m| m.languages.iter().any(|l| l == language)).map(|m| m.script.as_str())
}

/// The model for the settings: RapidOCR reads the first language of `jpn+eng` only (like PaddleOCR). The `server`
/// variant exists for some scripts only; the others use `mobile`.
pub fn select(language_spec: &str, variant: &str) -> Result<&'static OcrModel, String> {
    let language = crate::tesseract::primary_lang(language_spec);
    let script = script(language).ok_or_else(|| format!("RapidOCR: язык «{language}» не поддерживается моделями PP-OCRv5. Выберите другой основной язык или движок Tesseract."))?;
    let of_script = || catalog().iter().filter(|m| m.script == script);
    of_script().find(|m| m.variant == variant).or_else(|| of_script().find(|m| m.variant == "mobile")).ok_or_else(|| format!("RapidOCR: нет модели для «{language}»."))
}

/// The languages of `jpn+eng` RapidOCR does not read (all but the first).
pub fn ignored_languages(language_spec: &str) -> Vec<String> {
    let primary = crate::tesseract::primary_lang(language_spec);
    language_spec.split('+').map(str::trim).filter(|l| !l.is_empty() && *l != primary).map(str::to_owned).collect()
}

/// Every file is in place with its size: cheap, for the status in the settings.
pub fn present(root: &Path, model: &OcrModel) -> bool {
    ROLES.iter().all(|role| fs::metadata(model.file(root, role)).is_ok_and(|m| m.is_file() && m.len() == model.files[*role].size))
}

/// Every file is in place and has the size and SHA-256 of the catalog.
pub fn verified(root: &Path, model: &OcrModel) -> bool {
    valid_in(&model.dir(root), model)
}

fn valid_in(dir: &Path, model: &OcrModel) -> bool {
    model.files.values().all(|file| verify(&dir.join(&file.name), file))
}

pub fn remove(root: &Path, model: &OcrModel) -> Result<(), String> {
    let dir = model.dir(root);
    match fs::remove_dir_all(&dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Не удалось удалить {}: {e}", dir.display())),
    }
}

fn offline(model: &OcrModel, error: impl std::fmt::Display) -> String {
    format!("Не удалось скачать модель RapidOCR «{}» ({}): {error}. Проверьте подключение к сети и повторите загрузку в настройках.", model.label, model.id)
}

/// Download the model set into `root/<id>`. `progress` gets 0..100. An installation that already verifies is kept.
pub async fn download(model: &OcrModel, root: &Path, mut progress: impl FnMut(i32)) -> Result<PathBuf, String> {
    let dest = model.dir(root);
    if verified(root, model) {
        progress(100);
        return Ok(dest);
    }
    fs::create_dir_all(root).map_err(|e| format!("{}: {e}", root.display()))?;
    let stage = tempfile::Builder::new().prefix(".download-").tempdir_in(root).map_err(|e| e.to_string())?;
    // The ModelScope CDN answers 403 to a request without a User-Agent (reqwest sends none by default).
    let client = reqwest::Client::builder()
        .user_agent(concat!("LipaX/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(1800))
        .build()
        .map_err(|e| e.to_string())?;
    let total = model.size().max(1);
    let (mut done, mut percent) = (0u64, 0);
    progress(0);
    for role in ROLES {
        let file = &model.files[role];
        let part = stage.path().join(format!("{}.part", file.name));
        let mut response = client.get(&file.url).send().await.and_then(reqwest::Response::error_for_status).map_err(|e| offline(model, e))?;
        let mut out = fs::File::create(&part).map_err(|e| e.to_string())?;
        let mut size = 0;
        while let Some(chunk) = response.chunk().await.map_err(|e| offline(model, e))? {
            size += chunk.len() as u64;
            if size > file.size {
                return Err(format!("Модель RapidOCR «{}»: файл {} больше ожидаемого; загрузка отменена.", model.label, file.name));
            }
            out.write_all(&chunk).map_err(|e| e.to_string())?;
            done += chunk.len() as u64;
            let next = ((done * 100 / total) as i32).min(99);
            if next != percent {
                percent = next;
                progress(percent);
            }
        }
        out.sync_all().map_err(|e| e.to_string())?;
        drop(out);
        if !verify(&part, file) {
            return Err(format!("Модель RapidOCR «{}»: размер или SHA-256 файла {} не совпадает с каталогом; файл удалён. Повторите загрузку.", model.label, file.name));
        }
        fs::rename(&part, stage.path().join(&file.name)).map_err(|e| e.to_string())?;
    }
    commit(root, model, stage)?;
    progress(100);
    Ok(dest)
}

fn commit(root: &Path, model: &OcrModel, stage: tempfile::TempDir) -> Result<(), String> {
    if !valid_in(stage.path(), model) {
        return Err("Model size or SHA-256 verification failed.".into());
    }
    let dest = model.dir(root);
    // Keep an old installation recoverable until the replacement is in place.
    let backup = tempfile::Builder::new().prefix(".backup-").tempdir_in(root).map_err(|e| e.to_string())?;
    let old = backup.path().join("old");
    if dest.exists() {
        fs::rename(&dest, &old).map_err(|e| e.to_string())?;
    }
    if let Err(e) = fs::rename(stage.path(), &dest) {
        if old.exists() && fs::rename(&old, &dest).is_err() {
            let retained = backup.keep();
            return Err(format!("{e}; previous model retained at {}", retained.join("old").display()));
        }
        return Err(e.to_string());
    }
    Ok(())
}

/// One model as the settings show it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ModelState {
    pub id: String,
    pub label: String,
    pub variant: String,
    pub size: u64,
    pub installed: bool,
}

/// What the settings show for RapidOCR: the model the recognition language needs, the installed ones, the library.
#[derive(Debug, Clone, Serialize)]
pub struct RapidStatus {
    pub language: String,
    /// Languages of the setting RapidOCR does not read.
    pub ignored: Vec<String>,
    pub supported: bool,
    pub selected: Option<ModelState>,
    /// The script of the language has a `server` model.
    pub server_available: bool,
    /// Every catalog model with its state, for «Delete».
    pub models: Vec<ModelState>,
    pub library: Option<String>,
    pub threads: usize,
    pub ready: bool,
    pub summary: String,
    pub problems: Vec<String>,
}

/// The status for the settings. Reads the disk (no hashing of every model: the selected one is verified, the others are
/// checked by size); run it off the GUI thread.
pub fn status(language_spec: &str, variant: &str, threads: u32, root: &Path, library: Result<PathBuf, String>) -> RapidStatus {
    let language = crate::tesseract::primary_lang(language_spec).to_owned();
    let selected = select(language_spec, variant);
    let state = |m: &OcrModel, installed| ModelState { id: m.id.clone(), label: m.label.clone(), variant: m.variant.clone(), size: m.size(), installed };
    let selected_state = selected.as_ref().ok().map(|m| state(m, verified(root, m)));
    let models = catalog().iter().map(|m| {
        let installed = match &selected_state { Some(s) if s.id == m.id => s.installed, _ => present(root, m) };
        state(m, installed)
    }).collect();
    let mut problems = Vec::new();
    if let Err(e) = &selected { problems.push(e.clone()); }
    if let Some(s) = selected_state.as_ref().filter(|s| !s.installed) {
        problems.push(format!("Модель «{}» ({}, {:.0} МБ) не скачана. Нажмите «Скачать».", s.label, s.id, s.size as f64 / 1e6));
    }
    if let Err(e) = &library { problems.push(e.clone()); }
    let ready = problems.is_empty();
    let summary = match (&selected_state, ready) {
        (Some(s), true) => format!("Готово: {} · {}", s.label, s.variant),
        _ => problems.first().cloned().unwrap_or_default(),
    };
    RapidStatus {
        ignored: ignored_languages(language_spec),
        supported: selected.is_ok(),
        server_available: selected.as_ref().is_ok_and(|m| catalog().iter().any(|o| o.script == m.script && o.variant == "server")),
        selected: selected_state,
        models,
        library: library.ok().map(|p| p.display().to_string()),
        threads: super::rapid::effective_threads(threads),
        ready,
        summary,
        problems,
        language,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::io::Read;

    fn fixture() -> (tempfile::TempDir, OcrModel) {
        let dir = tempfile::tempdir().unwrap();
        let files = ROLES.iter().map(|role| {
            let name = format!("{role}.bin");
            let data = format!("valid fixture bytes for {role}");
            fs::write(dir.path().join(&name), &data).unwrap();
            (role.to_string(), ModelFile { name, size: data.len() as u64, sha256: format!("{:x}", Sha256::digest(data.as_bytes())), url: String::new() })
        }).collect();
        let model = OcrModel { id: "test-mobile".into(), script: "test".into(), variant: "mobile".into(), label: "тест".into(),
            languages: vec!["eng".into()], version: "test".into(), license: "Apache-2.0".into(), files };
        (dir, model)
    }

    #[test]
    fn catalog_is_complete_pinned_and_safe() {
        let models = catalog();
        assert!(models.len() >= 6);
        for model in models {
            assert!(["mobile", "server"].contains(&model.variant.as_str()), "{}", model.id);
            assert_eq!(model.id, format!("{}-{}", model.script, model.variant));
            assert_eq!(model.license, "Apache-2.0");
            for file in model.files.values() {
                assert!(file.url.starts_with("https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.10.0/"), "{}", file.url);
                assert!(file.url.ends_with(&format!("/{}", file.name)));
            }
            assert!(model.files["rec"].name.starts_with(&model.script) && model.files["dict"].name.ends_with(".txt"));
        }
        let ids: std::collections::BTreeSet<_> = models.iter().map(|m| &m.id).collect();
        assert_eq!(ids.len(), models.len(), "unique ids");
        // A language belongs to one script: the choice of a model never depends on the order of the catalog.
        for model in models {
            for language in &model.languages {
                assert!(models.iter().filter(|m| m.languages.contains(language)).all(|m| m.script == model.script), "{language}");
            }
        }
    }

    #[test]
    fn a_broken_catalog_is_refused() {
        let good = include_str!("ocr_models_catalog.json");
        assert!(parse_catalog(good).is_ok());
        assert!(parse_catalog(&good.replacen("\"id\": \"en-mobile\"", "\"id\": \"../en\"", 1)).is_err());
        assert!(parse_catalog(&good.replacen("\"name\": \"ppocrv5_en_dict.txt\"", "\"name\": \"../dict.txt\"", 1)).is_err());
        assert!(parse_catalog(&good.replacen("\"sha256\": \"4d97c44a", "\"sha256\": \"4d97", 1)).is_err());
        assert!(parse_catalog("[{\"id\": \"x\"}]").is_err());
    }

    #[test]
    fn tesseract_codes_map_to_scripts_and_the_first_language_wins() {
        assert_eq!(script("eng"), Some("en"));
        assert_eq!(script("rus"), Some("eslav"));
        assert_eq!(script("ukr"), Some("eslav"));
        assert_eq!(script("bul"), Some("cyrillic"));
        assert_eq!(script("deu"), Some("latin"));
        assert_eq!(script("jpn"), Some("ch"));
        assert_eq!(script("chi_sim"), Some("ch"));
        assert_eq!(script("kor"), Some("korean"));
        assert_eq!(script("klingon"), None);
        assert_eq!(select("jpn+eng", "mobile").unwrap().id, "ch-mobile");
        assert_eq!(select("jpn+eng", "server").unwrap().id, "ch-server");
        assert_eq!(select("rus+eng", "server").unwrap().id, "eslav-mobile", "no server model for Cyrillic: mobile is used");
        assert_eq!(select("", "mobile").unwrap().id, "en-mobile");
        assert!(select("klingon+eng", "mobile").unwrap_err().contains("klingon"));
        assert_eq!(ignored_languages("jpn+eng+rus"), ["eng", "rus"]);
        assert!(ignored_languages("eng").is_empty());
    }

    #[test]
    fn truncated_and_same_size_corrupt_files_are_rejected() {
        let (dir, model) = fixture();
        let root = dir.path().parent().unwrap();
        let model = OcrModel { id: dir.path().file_name().unwrap().to_string_lossy().into_owned(), ..model };
        assert!(verified(root, &model) && present(root, &model));
        let rec = &model.files["rec"];
        fs::write(dir.path().join(&rec.name), vec![b'x'; rec.size as usize]).unwrap();
        assert!(present(root, &model), "same size passes the cheap check");
        assert!(!verified(root, &model), "but not the hash");
        fs::write(dir.path().join(&rec.name), b"short").unwrap();
        assert!(!present(root, &model) && !verified(root, &model));
    }

    /// Serves the files of `model` from `fixture` (optionally with the first byte of each flipped).
    fn server(model: &mut OcrModel, fixture: &Path, corrupt: bool) -> std::thread::JoinHandle<()> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let mut bodies = Vec::new();
        for role in ROLES {
            let file = model.files.get_mut(role).unwrap();
            file.url = format!("http://{addr}/{}", file.name);
            let mut bytes = fs::read(fixture.join(&file.name)).unwrap();
            if corrupt { bytes[0] ^= 1; }
            bodies.push(bytes);
        }
        std::thread::spawn(move || {
            for bytes in bodies.into_iter().take(if corrupt { 1 } else { ROLES.len() }) {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    assert!(request.len() < 4096);
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                // Like the ModelScope CDN: no User-Agent, no file.
                if !String::from_utf8_lossy(&request).to_ascii_lowercase().contains("\r\nuser-agent: lipax/") {
                    write!(stream, "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                    continue;
                }
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bytes.len()).unwrap();
                stream.write_all(&bytes).unwrap();
            }
        })
    }

    #[tokio::test]
    async fn download_reports_progress_verifies_and_commits() {
        let (fixture, mut model) = fixture();
        let root = tempfile::tempdir().unwrap();
        let server = server(&mut model, fixture.path(), false);
        let mut progress = Vec::new();
        let dir = download(&model, root.path(), |p| progress.push(p)).await.unwrap();
        server.join().unwrap();
        assert_eq!(dir, root.path().join("test-mobile"));
        assert_eq!((progress.first(), progress.last()), (Some(&0), Some(&100)));
        assert!(progress.windows(2).all(|w| w[0] <= w[1]));
        assert!(verified(root.path(), &model));
        // Only the model directory is left: no staging, no `.part`.
        let entries: Vec<_> = fs::read_dir(root.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(entries, ["test-mobile"]);
        assert!(fs::read_dir(&dir).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".part")));
        // An installed model is not downloaded again (the server is gone).
        assert_eq!(download(&model, root.path(), |_| {}).await.unwrap(), dir);
        remove(root.path(), &model).unwrap();
        assert!(!dir.exists());
        remove(root.path(), &model).unwrap();
    }

    #[tokio::test]
    async fn checksum_failure_cleans_staging_and_keeps_the_old_installation() {
        let (fixture, mut model) = fixture();
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("test-mobile");
        fs::create_dir(&old).unwrap();
        fs::write(old.join("keep"), b"existing file").unwrap();
        let server = server(&mut model, fixture.path(), true);
        let error = download(&model, root.path(), |_| {}).await.unwrap_err();
        server.join().unwrap();
        assert!(error.contains("SHA-256"), "{error}");
        assert!(old.join("keep").is_file());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn offline_error_is_actionable_and_leaves_nothing_behind() {
        let (_, mut model) = fixture();
        let root = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        for f in model.files.values_mut() { f.url = format!("http://{addr}/missing"); }
        let error = download(&model, root.path(), |_| {}).await.unwrap_err();
        assert!(error.contains("Проверьте подключение к сети"), "{error}");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn status_names_the_missing_model_and_the_ignored_languages() {
        let root = tempfile::tempdir().unwrap();
        let s = status("rus+eng", "mobile", 0, root.path(), Ok(PathBuf::from("/usr/lib/libonnxruntime.so.1")));
        assert!(!s.ready && s.supported && !s.server_available);
        assert_eq!(s.selected.as_ref().unwrap().id, "eslav-mobile");
        assert_eq!(s.ignored, ["eng"]);
        assert!(s.summary.contains("не скачана"), "{}", s.summary);
        assert!(s.models.iter().all(|m| !m.installed));
        let s = status("jpn", "server", 2, root.path(), Err("нет библиотеки".into()));
        assert!(s.server_available && s.problems.iter().any(|p| p == "нет библиотеки"));
        assert_eq!(s.threads, 2);
        let s = status("klingon", "mobile", 0, root.path(), Ok(PathBuf::new()));
        assert!(!s.supported && s.selected.is_none());
    }
}
