//! Verified Mozilla Bergamot models. All filesystem work runs on the model QThread
//! (or spawn_blocking for translation-time validation), never on the GUI thread.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug, Clone, Deserialize)]
pub struct ModelFile {
    pub name: String,
    pub size: u64,
    pub sha256: String,
    pub url: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Model {
    pub pair: String,
    pub version: String,
    pub files: BTreeMap<String, ModelFile>,
}

pub fn catalog() -> &'static [Model] {
    static CATALOG: std::sync::OnceLock<Vec<Model>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("bergamot_catalog.json"))
            .expect("bundled Mozilla catalog")
    })
}
pub fn pair(source: &str, target: &str) -> Result<String, String> {
    if [source, target].iter().any(|s| {
        s.is_empty()
            || s.len() > 16
            || !s.bytes().all(|b| b.is_ascii_alphabetic() || b == b'-')
            || *s == "auto"
    }) {
        return Err("Select explicit source and target languages for Bergamot.".into());
    }
    Ok(format!(
        "{}-{}",
        source.to_ascii_lowercase(),
        target.to_ascii_lowercase()
    ))
}
pub fn cache_root() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LipaX/bergamot-models")
}
pub fn missing(pair: &str) -> String {
    format!("Translation model for {pair} is missing. Download it in Settings.")
}
pub(crate) fn verify(path: &Path, file: &ModelFile) -> bool {
    let Ok(mut input) = fs::File::open(path) else {
        return false;
    };
    if !input
        .metadata()
        .is_ok_and(|m| m.is_file() && m.len() == file.size)
    {
        return false;
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        match input.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => hash.update(&buffer[..n]),
            Err(_) => return false,
        }
    }
    format!("{:x}", hash.finalize()) == file.sha256
}

// Explicit paths may be a directory or a legacy Bergamot YAML configuration.
fn files_at(path: &Path, model: &Model) -> Option<BTreeMap<String, PathBuf>> {
    let config = if path.is_file() {
        Some(path.to_path_buf())
    } else {
        let p = path.join(format!("{}.yml", model.pair));
        p.is_file().then_some(p)
    };
    if let Some(config) = config {
        let config = config.canonicalize().ok()?;
        if fs::metadata(&config).ok()?.len() > 1024 * 1024 {
            return None;
        }
        let yaml: serde_yaml::Value =
            serde_yaml::from_str(&fs::read_to_string(&config).ok()?).ok()?;
        let parent = config.parent()?;
        let mut files = BTreeMap::new();
        for role in model.files.keys() {
            let (key, index) = match role.as_str() {
                "model" => ("models", 0),
                "lex" => ("shortlist", 0),
                "vocab" | "srcvocab" => ("vocabs", 0),
                "trgvocab" => ("vocabs", 1),
                _ => return None,
            };
            let name = yaml.get(key)?.get(index)?.as_str()?;
            files.insert(role.clone(), parent.join(name));
        }
        // A shared vocabulary must actually be used for both sides.
        if model.files.contains_key("vocab")
            && yaml.get("vocabs")?.get(0)? != yaml.get("vocabs")?.get(1)?
        {
            return None;
        }
        Some(files)
    } else {
        Some(
            model
                .files
                .iter()
                .map(|(role, f)| (role.clone(), path.join(&f.name)))
                .collect(),
        )
    }
}
fn valid_at(path: &Path, model: &Model) -> Option<BTreeMap<String, PathBuf>> {
    let paths = files_at(path, model)?;
    model
        .files
        .iter()
        .all(|(role, f)| verify(&paths[role], f))
        .then_some(paths)
}

/// No symlink traversal of Firefox directories; bounded scan also tolerates inaccessible profiles.
fn firefox_dirs(
    root: &Path,
    depth: usize,
    in_models: bool,
    out: &mut Vec<PathBuf>,
    remaining: &mut usize,
) {
    if depth > 12 || *remaining == 0 {
        return;
    }
    *remaining -= 1;
    let in_models = in_models || root.file_name().is_some_and(|n| n == "bergamot-models");
    if in_models {
        out.push(root.to_path_buf());
    }
    if let Ok(entries) = fs::read_dir(root) {
        for e in entries.flatten() {
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                firefox_dirs(&e.path(), depth + 1, in_models, out, remaining);
            }
        }
    }
}

fn write_config(dir: &Path, model: &Model) -> Result<(), String> {
    let file = |role: &str| serde_json::to_string(&model.files[role].name).expect("filename");
    let (src, dst) = if model.files.contains_key("vocab") {
        ("vocab", "vocab")
    } else {
        ("srcvocab", "trgvocab")
    };
    let config = format!(
        "bergamot-mode: wasm\nmodels: [{}]\nvocabs: [{}, {}]\nshortlist: [{}, false]\nbeam-size: 1\nnormalize: 1.0\nword-penalty: 0\nmax-length-break: 128\nmini-batch-words: 1024\nworkspace: 128\nmax-length-factor: 2.0\nskip-cost: true\ngemm-precision: int8shiftAll\nalignment: soft\nquiet: true\nquiet-translation: false\n",
        file("model"),
        file(src),
        file(dst),
        file("lex")
    );
    fs::write(dir.join(format!("{}.yml", model.pair)), config).map_err(|e| e.to_string())
}
fn install(
    root: &Path,
    model: &Model,
    paths: &BTreeMap<String, PathBuf>,
) -> Result<PathBuf, String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let stage = tempfile::Builder::new()
        .prefix(".install-")
        .tempdir_in(root)
        .map_err(|e| e.to_string())?;
    for (role, file) in &model.files {
        fs::copy(&paths[role], stage.path().join(&file.name)).map_err(|e| e.to_string())?;
    }
    commit(root, model, stage)
}
fn commit(root: &Path, model: &Model, stage: tempfile::TempDir) -> Result<PathBuf, String> {
    if valid_at(stage.path(), model).is_none() {
        return Err("Model size or SHA-256 verification failed.".into());
    }
    write_config(stage.path(), model)?;
    let dest = root.join(&model.pair);
    // Keep an old/invalid installation recoverable until the replacement is committed.
    let backup = tempfile::Builder::new()
        .prefix(".backup-")
        .tempdir_in(root)
        .map_err(|e| e.to_string())?;
    let old = backup.path().join("old");
    if dest.exists() {
        fs::rename(&dest, &old).map_err(|e| e.to_string())?;
    }
    if let Err(e) = fs::rename(stage.path(), &dest) {
        if old.exists() && fs::rename(&old, &dest).is_err() {
            let retained = backup.keep();
            return Err(format!(
                "{e}; previous model retained at {}",
                retained.join("old").display()
            ));
        }
        return Err(e.to_string());
    }
    Ok(dest)
}

pub fn installed_config(path: &Path, pair: &str) -> Option<PathBuf> {
    for model in catalog().iter().filter(|m| m.pair == pair) {
        if valid_at(path, model).is_some() {
            let config = if path.is_file() {
                path.to_path_buf()
            } else {
                path.join(format!("{pair}.yml"))
            };
            if config.is_file() {
                return Some(config);
            }
        }
    }
    None
}

/// Search order: explicit settings, LipaX cache, Firefox; install only complete verified sets.
/// `progress` uses -1 for discovery and 0..100 for download/verification.
pub async fn ensure(
    pair: &str,
    explicit: &[PathBuf],
    root: &Path,
    firefox: &Path,
    mut progress: impl FnMut(i32),
) -> Result<PathBuf, String> {
    ensure_with_catalog(pair, explicit, root, firefox, catalog(), &mut progress).await
}
async fn ensure_with_catalog(
    pair: &str,
    explicit: &[PathBuf],
    root: &Path,
    firefox: &Path,
    catalog: &[Model],
    progress: &mut impl FnMut(i32),
) -> Result<PathBuf, String> {
    let models: Vec<_> = catalog.iter().filter(|m| m.pair == pair).collect();
    let Some(preferred) = models.first() else {
        return Err(format!(
            "{} No official model is available for this pair.",
            missing(pair)
        ));
    };
    progress(-1);
    let cache = root.join(pair);
    for path in explicit
        .iter()
        .cloned()
        .chain(std::iter::once(cache.clone()))
    {
        for candidate in [path.clone(), path.join(pair)] {
            for model in &models {
                if let Some(paths) = valid_at(&candidate, model) {
                    if candidate == cache {
                        write_config(&cache, model)?;
                        return Ok(cache);
                    }
                    return install(root, model, &paths);
                }
            }
        }
    }
    let mut dirs = Vec::new();
    firefox_dirs(firefox, 0, false, &mut dirs, &mut 10000);
    for dir in dirs {
        for model in &models {
            if let Some(paths) = valid_at(&dir, model) {
                return install(root, model, &paths);
            }
        }
    }
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let stage = tempfile::Builder::new()
        .prefix(".download-")
        .tempdir_in(root)
        .map_err(|e| e.to_string())?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(1800))
        .build()
        .map_err(|e| e.to_string())?;
    let total: u64 = preferred.files.values().map(|f| f.size).sum();
    let mut done = 0;
    let mut percent = 0;
    progress(0);
    for file in preferred.files.values() {
        let mut response = client
            .get(&file.url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| format!("{} {e}", missing(pair)))?;
        let mut out = fs::File::create(stage.path().join(&file.name)).map_err(|e| e.to_string())?;
        let mut size = 0;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| format!("{} {e}", missing(pair)))?
        {
            size += chunk.len() as u64;
            if size > file.size {
                return Err("Model download exceeds its expected size.".into());
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
        if !verify(&stage.path().join(&file.name), file) {
            return Err("Model size or SHA-256 verification failed.".into());
        }
    }
    let installed = commit(root, preferred, stage)?;
    progress(100);
    Ok(installed)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Model) {
        let dir = tempfile::tempdir().unwrap();
        let files = [
            ("model", "model.enru.intgemm.alphas.bin"),
            ("lex", "lex.enru.bin"),
            ("vocab", "vocab.enru.spm"),
        ]
        .into_iter()
        .map(|(role, name)| {
            let data = format!("valid fixture bytes for {role}");
            fs::write(dir.path().join(name), &data).unwrap();
            (
                role.into(),
                ModelFile {
                    name: name.into(),
                    size: data.len() as u64,
                    sha256: format!("{:x}", Sha256::digest(data.as_bytes())),
                    url: String::new(),
                },
            )
        })
        .collect();
        (
            dir,
            Model {
                pair: "en-ru".into(),
                version: "test".into(),
                files,
            },
        )
    }
    #[test]
    fn catalog_has_complete_safe_verified_entries() {
        assert!(catalog().iter().any(|m| m.pair == "en-ru"));
        for model in catalog() {
            assert!(!model.version.is_empty());
            assert!(model.files.contains_key("model") && model.files.contains_key("lex"));
            assert!(
                model.files.contains_key("vocab")
                    || (model.files.contains_key("srcvocab")
                        && model.files.contains_key("trgvocab"))
            );
            for file in model.files.values() {
                assert_eq!(Path::new(&file.name).components().count(), 1);
                assert!(file.size > 0 && file.sha256.len() == 64);
                assert!(
                    file.url
                        .starts_with("https://firefox-settings-attachments.cdn.mozilla.net/")
                );
            }
        }
        assert!(pair("../../en", "ru").is_err());
        assert!(pair("auto", "ru").is_err());
        assert_eq!(pair("EN", "RU").unwrap(), "en-ru");
    }
    #[test]
    fn truncated_and_same_size_corrupt_files_are_rejected() {
        let (dir, model) = fixture();
        assert!(valid_at(dir.path(), &model).is_some());
        let f = &model.files["model"];
        fs::write(dir.path().join(&f.name), vec![b'x'; f.size as usize]).unwrap();
        assert!(valid_at(dir.path(), &model).is_none());
        fs::write(dir.path().join(&f.name), b"short").unwrap();
        assert!(valid_at(dir.path(), &model).is_none());
    }
    #[test]
    fn generated_config_and_legacy_explicit_yaml_validate() {
        let (dir, model) = fixture();
        write_config(dir.path(), &model).unwrap();
        assert!(valid_at(dir.path(), &model).is_some());
        assert!(valid_at(&dir.path().join("en-ru.yml"), &model).is_some());
        fs::remove_file(dir.path().join(&model.files["vocab"].name)).unwrap();
        assert!(valid_at(dir.path(), &model).is_none());
    }
    #[tokio::test]
    async fn explicit_path_wins_over_cache_and_firefox() {
        let (explicit, model) = fixture();
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("en-ru")).unwrap();
        fs::write(root.path().join("en-ru/marker"), "old").unwrap();
        let result = ensure_with_catalog(
            "en-ru",
            &[explicit.path().into()],
            root.path(),
            Path::new("/missing-firefox"),
            std::slice::from_ref(&model),
            &mut |_| {},
        )
        .await
        .unwrap();
        assert!(!result.join("marker").exists());
        assert!(valid_at(&result, &model).is_some());
        assert!(explicit.path().join(&model.files["model"].name).exists());
        // An invalid explicit path falls back to the installed cache, even offline.
        assert_eq!(
            ensure_with_catalog(
                "en-ru",
                &[PathBuf::from("/missing")],
                root.path(),
                Path::new("/missing"),
                &[model],
                &mut |_| {}
            )
            .await
            .unwrap(),
            result
        );
    }
    #[tokio::test]
    async fn firefox_import_is_copied_and_survives_profile_removal() {
        let (fixture, model) = fixture();
        let home = tempfile::tempdir().unwrap();
        let firefox = home.path().join("firefox");
        let profile = firefox.join("abc.default/storage/bergamot-models/en-ru");
        fs::create_dir_all(&profile).unwrap();
        for f in model.files.values() {
            fs::copy(fixture.path().join(&f.name), profile.join(&f.name)).unwrap();
        }
        let root = home.path().join("cache");
        let result = ensure_with_catalog(
            "en-ru",
            &[],
            &root,
            &firefox,
            std::slice::from_ref(&model),
            &mut |_| {},
        )
        .await
        .unwrap();
        fs::remove_dir_all(firefox).unwrap();
        assert!(valid_at(&result, &model).is_some());
    }
    fn server(model: &mut Model, fixture: &Path, corrupt: bool) -> std::thread::JoinHandle<()> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let mut bodies = Vec::new();
        for file in model.files.values_mut() {
            file.url = format!("http://{addr}/{}", file.name);
            let mut bytes = fs::read(fixture.join(&file.name)).unwrap();
            if corrupt {
                bytes[0] ^= 1;
            }
            bodies.push(bytes);
        }
        std::thread::spawn(move || {
            for bytes in bodies.into_iter().take(if corrupt { 1 } else { 3 }) {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    assert!(request.len() < 4096);
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    bytes.len()
                )
                .unwrap();
                stream.write_all(&bytes).unwrap();
            }
        })
    }
    #[tokio::test]
    async fn download_reports_progress_and_commits_verified_config() {
        let (fixture, mut model) = fixture();
        let root = tempfile::tempdir().unwrap();
        let server = server(&mut model, fixture.path(), false);
        let mut progress = Vec::new();
        let installed = ensure_with_catalog(
            "en-ru",
            &[],
            root.path(),
            Path::new("/missing"),
            std::slice::from_ref(&model),
            &mut |p| progress.push(p),
        )
        .await
        .unwrap();
        server.join().unwrap();
        assert_eq!(progress.first(), Some(&-1));
        assert_eq!(progress.last(), Some(&100));
        assert!(progress.windows(2).all(|w| w[0] <= w[1]));
        assert!(valid_at(&installed, &model).is_some());
        assert!(installed.join("en-ru.yml").is_file());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }
    #[tokio::test]
    async fn checksum_failure_cleans_staging_and_preserves_old_directory() {
        let (fixture, mut model) = fixture();
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("en-ru");
        fs::create_dir(&old).unwrap();
        fs::write(old.join("keep"), b"existing file").unwrap();
        let server = server(&mut model, fixture.path(), true);
        let error = ensure_with_catalog(
            "en-ru",
            &[],
            root.path(),
            Path::new("/missing"),
            &[model],
            &mut |_| {},
        )
        .await
        .unwrap_err();
        server.join().unwrap();
        assert!(error.contains("SHA-256"));
        assert!(old.join("keep").is_file());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }
    #[tokio::test]
    async fn offline_error_is_actionable_and_partial_files_are_removed() {
        let (_, mut model) = fixture();
        let root = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        for f in model.files.values_mut() {
            f.url = format!("http://{addr}/missing");
        }
        let error = ensure_with_catalog(
            "en-ru",
            &[],
            root.path(),
            Path::new("/missing"),
            &[model],
            &mut |_| {},
        )
        .await
        .unwrap_err();
        assert!(error.starts_with(&missing("en-ru")));
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
}
