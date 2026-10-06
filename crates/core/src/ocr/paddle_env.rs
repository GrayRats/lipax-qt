//! Is PaddleOCR usable with the Python the user chose, and if not, what exactly is missing and how to fix it.
//!
//! A small script (`paddle_env.py`) reports facts about that Python: its version, whether it is a
//! virtual environment, where `paddleocr` and `paddlepaddle` live and how they were installed, whether
//! they import, which models are cached. Everything that is decided from the facts is here, as plain
//! functions that can be tested: the way the package was installed ([`InstallKind`]), the problems and for each
//! of them the ways to fix it. The ways differ by installation: a package from `uv` is upgraded with
//! `uv pip`, one from a git checkout with `git pull`, a system package is never touched (a virtual
//! environment is suggested instead). Nothing is installed here: the commands are shown to the user.

use crate::tesseract::Family;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

const SCRIPT: &str = include_str!("paddle_env.py");
/// The directory the documentation and the settings suggest for the virtual environment.
const VENV: &str = "~/.local/share/lipa/paddle-venv";
const PACKAGES: &str = "'paddlepaddle>=3,<4' 'paddleocr>=3,<4'";
/// What the integration speaks (see `paddle::language`).
const SUPPORTED: &str = "eng, rus, jpn, kor, chi_sim, chi_tra, deu, fra, spa, ita, por, ukr, pol";

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct Dist {
    pub version: String,
    pub location: String,
    pub direct_url: Option<serde_json::Value>,
    pub installer: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct Facts {
    pub python_version: String,
    pub executable: String,
    pub prefix: String,
    pub venv: bool,
    pub pyvenv_cfg: String,
    pub paddleocr: Option<Dist>,
    pub paddle: Option<Dist>,
    pub import_errors: HashMap<String, String>,
    pub models: Vec<String>,
}

/// How `paddleocr` got into this Python.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallKind {
    /// Not installed.
    None,
    /// From a source checkout (`pip install -e .`, `pip install git+…`).
    Git,
    /// In a virtual environment made by `uv`.
    Uv,
    /// In a virtual environment made by `venv`/`virtualenv`.
    Venv,
    /// By the system's package manager (deb, an AUR package, …): the files belong to the system.
    System,
    /// By `pip install --user`.
    User,
    Unknown,
}

impl InstallKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "не установлен",
            Self::Git => "из исходного кода (git)",
            Self::Uv => "виртуальное окружение uv",
            Self::Venv => "виртуальное окружение venv/pip",
            Self::System => "пакет системы",
            Self::User => "pip --user",
            Self::Unknown => "способ установки не определён",
        }
    }
}

/// The way the installed `paddleocr` got there. A source checkout is told first (it can sit in any
/// environment), then a virtual environment (`uv` leaves its mark in `pyvenv.cfg`), then by where the files are.
pub fn classify(facts: &Facts, home: &Path) -> InstallKind {
    let Some(dist) = &facts.paddleocr else { return InstallKind::None };
    let from_source = dist.direct_url.as_ref().is_some_and(|u| u.get("vcs_info").is_some() || u["dir_info"]["editable"].as_bool() == Some(true));
    if from_source { return InstallKind::Git; }
    if facts.venv {
        return if facts.pyvenv_cfg.lines().any(|l| l.split('=').next().is_some_and(|k| k.trim() == "uv")) { InstallKind::Uv } else { InstallKind::Venv };
    }
    let location = dist.location.as_str();
    if location.starts_with(&home.join(".local").display().to_string()) { InstallKind::User }
    else if location.starts_with("/usr/lib") || location.contains("dist-packages") { InstallKind::System }
    else { InstallKind::Unknown }
}

/// Substrings of the names of the models PaddleOCR 3.x uses for a language (cache directory names).
fn model_patterns(paddle_language: &str) -> &'static [&'static str] {
    match paddle_language {
        "ch" | "chinese_cht" | "japan" => &["pp-ocrv5", "pp-ocrv4_server_rec", "japan_pp-ocr", "chinese_cht_pp-ocr"],
        "en" => &["en_pp-ocr", "pp-ocrv5"],
        "korean" => &["korean_pp-ocr"],
        "ru" | "uk" => &["cyrillic_pp-ocr", "eslav_pp-ocr"],
        "de" | "fr" | "es" | "it" | "pt" | "pl" => &["latin_pp-ocr", "pp-ocrv5"],
        _ => &[],
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InstallOption {
    pub title: String,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Problem {
    pub id: &'static str,
    /// `error` stops PaddleOCR from working, `warning` may, `info` is a remark.
    pub severity: &'static str,
    pub title: String,
    pub detail: String,
    pub options: Vec<InstallOption>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PaddleEnv {
    pub python: String,
    pub python_found: bool,
    pub python_version: Option<String>,
    pub paddleocr_version: Option<String>,
    pub paddle_version: Option<String>,
    pub install_kind: InstallKind,
    pub install_label: &'static str,
    pub language: String,
    pub paddle_language: Option<&'static str>,
    pub language_supported: bool,
    pub models_cached: bool,
    pub ready: bool,
    pub summary: String,
    pub problems: Vec<Problem>,
}

/// What the machine offers for installing.
#[derive(Debug, Clone, Copy)]
pub struct Tools {
    pub uv: bool,
    pub git: bool,
    pub family: Family,
}

fn option(title: &str, commands: &[&str]) -> InstallOption {
    InstallOption { title: title.into(), commands: commands.iter().map(|c| c.to_string()).collect() }
}

/// The ways to get PaddleOCR from nothing: `uv` first where it exists, then `venv` + `pip`, then a source checkout.
fn fresh_install(tools: Tools) -> Vec<InstallOption> {
    let uv = option("uv (быстро, рекомендуется)", &[
        &format!("uv venv {VENV}"),
        &format!("uv pip install --python {VENV}/bin/python {PACKAGES}"),
    ]);
    let prerequisite = match tools.family {
        Family::Arch => "sudo pacman -S --needed python python-pip",
        Family::Debian => "sudo apt install python3 python3-venv python3-pip",
        Family::Other => "# установите Python 3 с модулями venv и pip средствами вашей системы (например, pkg install python3)",
    };
    let pip = option("venv и pip", &[prerequisite, &format!("python3 -m venv {VENV}"), &format!("{VENV}/bin/python -m pip install {PACKAGES}")]);
    let git = option("из исходного кода (git)", &[
        "git clone https://github.com/PaddlePaddle/PaddleOCR.git",
        &format!("python3 -m venv {VENV}"),
        &format!("{VENV}/bin/python -m pip install 'paddlepaddle>=3,<4'"),
        &format!("{VENV}/bin/python -m pip install -e ./PaddleOCR"),
    ]);
    let mut options = if tools.uv { vec![uv, pip, git] } else { vec![pip, uv, git] };
    if !tools.git { options.retain(|o| !o.title.contains("git")); }
    options.push(option("системные пакеты (deb, AUR, pkg)", &[
        "# PaddlePaddle и PaddleOCR 3.x в официальных репозиториях обычно нет: надёжнее виртуальное окружение выше.",
        "# Arch: проверьте AUR командой  paru -Ss paddleocr  (имя и версия пакета зависят от сборщика);",
        "# Debian/Ubuntu: пакета нет, используйте venv; затем укажите Python окружения в настройках.",
    ]));
    options
}

/// How to bring an installed package to the wanted version, by where it came from.
fn upgrade(kind: InstallKind, python: &str, tools: Tools, spec: &str) -> Vec<InstallOption> {
    match kind {
        InstallKind::Uv => vec![option("uv", &[&format!("uv pip install --python {python} -U {spec}")])],
        InstallKind::Git => vec![option("из исходного кода (git)", &[
            "cd путь/к/PaddleOCR   # каталог, из которого он был установлен",
            "git pull",
            &format!("{python} -m pip install -e ."),
        ])],
        InstallKind::Venv | InstallKind::User => vec![option("pip", &[&format!("{python} -m pip install -U {spec}")])],
        // The files belong to the system's package manager: they are not touched; a separate environment is made.
        InstallKind::System | InstallKind::Unknown | InstallKind::None => fresh_install(tools),
    }
}

/// Everything that can be said from the facts. `lang` is the source language of the settings (`jpn+eng`).
pub fn evaluate(python: &str, python_found: bool, facts: Option<&Facts>, error: Option<&str>, lang: &str, tools: Tools, home: &Path) -> PaddleEnv {
    let primary = crate::tesseract::primary_lang(lang).to_string();
    let paddle_language = super::paddle::language(&primary).ok();
    let mut problems: Vec<Problem> = Vec::new();
    let mut add = |id, severity, title: String, detail: String, options| problems.push(Problem { id, severity, title, detail, options });

    if !python_found {
        let command = match tools.family {
            Family::Arch => "sudo pacman -S --needed python python-pip",
            Family::Debian => "sudo apt install python3 python3-venv python3-pip",
            Family::Other => "# установите Python 3 средствами вашей системы",
        };
        add("python-missing", "error", format!("Python не найден: {python}"),
            "Укажите абсолютный путь к Python (лучше из виртуального окружения PaddleOCR) или установите Python.".into(),
            vec![option("установить Python", &[command])]);
    } else if let Some(error) = error {
        add("python-failed", "error", format!("Не удалось проверить {python}"), error.to_string(), Vec::new());
    }
    let (mut kind, mut paddleocr_version, mut paddle_version, mut python_version, mut models_cached) = (InstallKind::None, None, None, None, false);
    if let Some(facts) = facts {
        kind = classify(facts, home);
        paddleocr_version = facts.paddleocr.as_ref().map(|d| d.version.clone());
        paddle_version = facts.paddle.as_ref().map(|d| d.version.clone());
        python_version = Some(facts.python_version.clone());
        let executable = if facts.executable.is_empty() { python } else { facts.executable.as_str() };
        let minor = facts.python_version.split('.').nth(1).and_then(|m| m.parse::<u32>().ok()).unwrap_or(0);
        if facts.python_version.starts_with("3.") && !(9..=13).contains(&minor) {
            add("python-version", "warning", format!("Python {} может не подойти", facts.python_version),
                "Колёса PaddlePaddle 3.x собираются для Python 3.9–3.13; для другой версии pip может ответить «No matching distribution».".into(),
                vec![option("окружение с поддерживаемым Python (uv скачает его сам)", &[&format!("uv venv --python 3.12 {VENV}")])]);
        }
        match (&facts.paddleocr, &facts.paddle) {
            (None, _) => add("paddleocr-missing", "error", format!("PaddleOCR не установлен в {executable}"),
                "Пакет paddleocr (3.x) не найден в этом Python. Выберите способ установки:".into(), fresh_install(tools)),
            (Some(ocr), paddle) => {
                if ocr.version.split('.').next() != Some("3") {
                    add("paddleocr-version", "error", format!("Нужен PaddleOCR 3.x, установлен {}", ocr.version),
                        format!("Способ установки: {}. LipaX использует API 3.x (`predict`, `rec_texts`).", kind.label()),
                        upgrade(kind, executable, tools, "'paddleocr>=3,<4'"));
                }
                if paddle.is_none() {
                    add("paddle-missing", "error", "PaddlePaddle не установлен".into(),
                        "PaddleOCR нужен фреймворк PaddlePaddle (версия 3.x, для процессора).".into(),
                        upgrade(kind, executable, tools, "'paddlepaddle>=3,<4'"));
                }
            }
        }
        for (module, message) in &facts.import_errors {
            add("import-failed", "error", format!("Пакет {module} установлен, но не импортируется"), message.clone(),
                std::iter::once(option("пересоздать окружение", &[&format!("rm -rf {VENV}")])).chain(fresh_install(tools).into_iter().take(2)).collect());
        }
        let patterns = paddle_language.map(model_patterns).unwrap_or(&[]);
        models_cached = facts.models.iter().any(|m| { let m = m.to_lowercase(); patterns.iter().any(|p| m.contains(p)) });
        if facts.paddleocr.is_some() && paddle_language.is_some() && !models_cached {
            add("models-missing", "info", format!("Моделей для языка «{primary}» в кэше не нашлось"),
                "Они скачаются при первом распознавании (нужен интернет, до двух минут). Если не укладывается в 60 с, подготовьте их заранее:".into(),
                vec![option("загрузить модели заранее", &[&format!("{executable} -c \"from paddleocr import PaddleOCR; PaddleOCR(lang='{}')\"", paddle_language.unwrap_or("en"))])]);
        }
    } else if python_found && error.is_none() {
        add("paddleocr-missing", "error", "PaddleOCR не установлен".into(), "Выберите способ установки:".into(), fresh_install(tools));
    }
    if paddle_language.is_none() {
        add("language", "error", format!("Язык «{primary}» не поддержан интеграцией PaddleOCR"),
            format!("Поддерживаются: {SUPPORTED}. Выберите другой основной язык или движок Tesseract."), Vec::new());
    }
    let ready = !problems.iter().any(|p| p.severity == "error");
    let summary = if ready {
        format!("PaddleOCR {} · PaddlePaddle {} · {} · язык «{primary}»{}", paddleocr_version.as_deref().unwrap_or("?"), paddle_version.as_deref().unwrap_or("?"), kind.label(),
            if models_cached { "" } else { " · модели загрузятся при первом запуске" })
    } else {
        problems.iter().find(|p| p.severity == "error").map(|p| p.title.clone()).unwrap_or_default()
    };
    PaddleEnv { python: python.into(), python_found, python_version, paddleocr_version, paddle_version, install_kind: kind, install_label: kind.label(),
        language: primary, paddle_language, language_supported: paddle_language.is_some(), models_cached, ready, summary, problems }
}

/// Where `python` is: a path as it is, a bare name in `PATH`.
fn find_python(python: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let executable = |p: &Path| p.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    if python.contains('/') { return executable(Path::new(python)); }
    std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|d| executable(&d.join(python))))
}

fn which(program: &str) -> bool { find_python(program) }

/// The checks of the real machine: its tools and its distribution.
pub fn system_tools() -> Tools {
    let family = crate::tesseract::parse_os_release(&std::fs::read_to_string("/etc/os-release").unwrap_or_default()).family;
    Tools { uv: which("uv"), git: which("git"), family }
}

/// Asks the Python of the settings and evaluates the answer.
pub async fn inspect(python: &str, source_language: &str) -> PaddleEnv {
    inspect_with(python, &[], source_language, &[], system_tools()).await
}

/// `inspect` with extra interpreter flags and environment (tests hide the system's packages with `-S`).
pub async fn inspect_with(python: &str, flags: &[&str], source_language: &str, env: &[(&str, &str)], tools: Tools) -> PaddleEnv {
    let home = dirs::home_dir().unwrap_or_default();
    if !find_python(python) { return evaluate(python, false, None, None, source_language, tools, &home); }
    let run = Command::new(python).args(flags).args(["-c", SCRIPT]).envs(env.iter().copied())
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).output();
    let output = match tokio::time::timeout(Duration::from_secs(60), run).await {
        Err(_) => return evaluate(python, true, None, Some("проверка не завершилась за 60 с"), source_language, tools, &home),
        Ok(Err(e)) => return evaluate(python, true, None, Some(&e.to_string()), source_language, tools, &home),
        Ok(Ok(o)) => o,
    };
    match serde_json::from_slice::<Facts>(&output.stdout) {
        Ok(facts) if output.status.success() => evaluate(python, true, Some(&facts), None, source_language, tools, &home),
        _ => {
            let stderr: String = String::from_utf8_lossy(&output.stderr).chars().take(600).collect();
            evaluate(python, true, None, Some(&format!("Python ответил ошибкой: {}", stderr.trim())), source_language, tools, &home)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const HOME: &str = "/home/u";
    fn tools(uv: bool, family: Family) -> Tools { Tools { uv, git: true, family } }
    fn dist(version: &str, location: &str) -> Dist { Dist { version: version.into(), location: location.into(), ..Default::default() } }
    fn facts(ocr: Option<Dist>, paddle: Option<Dist>) -> Facts {
        Facts { python_version: "3.12.4".into(), executable: "/venv/bin/python".into(), paddleocr: ocr, paddle, ..Default::default() }
    }
    fn evaluate_facts(f: &Facts, lang: &str, tools: Tools) -> PaddleEnv { evaluate("/venv/bin/python", true, Some(f), None, lang, tools, Path::new(HOME)) }
    fn ids(e: &PaddleEnv) -> Vec<&'static str> { e.problems.iter().map(|p| p.id).collect() }

    // ── how it was installed ──

    #[test]
    fn the_way_of_installing_is_told_from_the_facts() {
        let home = Path::new(HOME);
        assert_eq!(classify(&facts(None, None), home), InstallKind::None);
        // A source checkout, wherever it sits: editable or from a VCS URL.
        let mut editable = dist("3.0.1", "/venv/lib/python3.12/site-packages");
        editable.direct_url = Some(serde_json::json!({ "url": "file:///src/PaddleOCR", "dir_info": { "editable": true } }));
        assert_eq!(classify(&Facts { venv: true, ..facts(Some(editable), None) }, home), InstallKind::Git);
        let mut vcs = dist("3.0.1", "/x");
        vcs.direct_url = Some(serde_json::json!({ "url": "https://github.com/PaddlePaddle/PaddleOCR.git", "vcs_info": { "vcs": "git" } }));
        assert_eq!(classify(&facts(Some(vcs), None), home), InstallKind::Git);
        // A plain wheel in a virtual environment: uv leaves its mark in pyvenv.cfg.
        let wheel = || dist("3.0.1", "/venv/lib/python3.12/site-packages");
        assert_eq!(classify(&Facts { venv: true, pyvenv_cfg: "home = /usr/bin\nuv = 0.5.1\nversion_info = 3.12.4\n".into(), ..facts(Some(wheel()), None) }, home), InstallKind::Uv);
        assert_eq!(classify(&Facts { venv: true, pyvenv_cfg: "home = /usr/bin\ninclude-system-site-packages = false\n".into(), ..facts(Some(wheel()), None) }, home), InstallKind::Venv);
        // Outside a virtual environment: by where the files are.
        assert_eq!(classify(&facts(Some(dist("3.0.1", "/usr/lib/python3.12/site-packages")), None), home), InstallKind::System);
        assert_eq!(classify(&facts(Some(dist("3.0.1", "/usr/lib/python3/dist-packages")), None), home), InstallKind::System);
        assert_eq!(classify(&facts(Some(dist("3.0.1", "/home/u/.local/lib/python3.12/site-packages")), None), home), InstallKind::User);
        assert_eq!(classify(&facts(Some(dist("3.0.1", "/opt/odd/site-packages")), None), home), InstallKind::Unknown);
    }

    // ── what is said ──

    #[test]
    fn a_working_environment_has_nothing_to_complain_about() {
        let mut f = facts(Some(dist("3.0.1", "/venv/lib/site-packages")), Some(dist("3.0.0", "/venv/lib/site-packages")));
        f.venv = true;
        f.models = vec!["en_PP-OCRv4_mobile_rec".into(), "PP-OCRv5_server_det".into()];
        let e = evaluate_facts(&f, "eng", tools(true, Family::Arch));
        assert!(e.ready && e.problems.is_empty(), "{:?}", e.problems);
        assert!(e.summary.contains("PaddleOCR 3.0.1") && e.summary.contains("PaddlePaddle 3.0.0") && e.summary.contains("venv"), "{}", e.summary);
        assert_eq!((e.paddle_language, e.models_cached, e.install_kind), (Some("en"), true, InstallKind::Venv));
    }

    #[test]
    fn missing_package_offers_the_ways_in_the_order_the_machine_suits() {
        let missing = evaluate_facts(&facts(None, Some(dist("3.0.0", "/x"))), "eng", tools(true, Family::Arch));
        assert!(!missing.ready);
        let problem = &missing.problems[0];
        assert_eq!(problem.id, "paddleocr-missing");
        let titles: Vec<&str> = problem.options.iter().map(|o| o.title.as_str()).collect();
        assert!(titles[0].starts_with("uv") && titles[1].starts_with("venv") && titles[2].contains("git") && titles[3].contains("системные"), "{titles:?}");
        assert!(problem.options[0].commands[1].contains("uv pip install --python ~/.local/share/lipa/paddle-venv/bin/python 'paddlepaddle>=3,<4' 'paddleocr>=3,<4'"));
        // Without uv the plain way comes first, and its first step installs Python's own tools for this distribution.
        let plain = evaluate_facts(&facts(None, None), "eng", tools(false, Family::Debian));
        assert!(plain.problems[0].options[0].title.starts_with("venv"));
        assert_eq!(plain.problems[0].options[0].commands[0], "sudo apt install python3 python3-venv python3-pip");
        let arch = evaluate_facts(&facts(None, None), "eng", tools(false, Family::Arch));
        assert_eq!(arch.problems[0].options[0].commands[0], "sudo pacman -S --needed python python-pip");
        // No git on the machine: no source option.
        let no_git = evaluate("/p", true, Some(&facts(None, None)), None, "eng", Tools { git: false, ..tools(true, Family::Other) }, Path::new(HOME));
        assert!(no_git.problems[0].options.iter().all(|o| !o.title.contains("(git)")));
    }

    #[test]
    fn an_old_version_is_upgraded_the_way_it_was_installed() {
        let old = |kind_facts: Facts| evaluate_facts(&kind_facts, "eng", tools(true, Family::Arch));
        let site = "/venv/lib/python3.12/site-packages";
        let command = |e: &PaddleEnv| e.problems.iter().find(|p| p.id == "paddleocr-version").unwrap().options[0].commands.join(" && ");
        let uv = old(Facts { venv: true, pyvenv_cfg: "uv = 0.5\n".into(), ..facts(Some(dist("2.9.1", site)), Some(dist("3.0.0", site))) });
        assert_eq!(command(&uv), "uv pip install --python /venv/bin/python -U 'paddleocr>=3,<4'");
        let pip = old(Facts { venv: true, ..facts(Some(dist("2.9.1", site)), Some(dist("3.0.0", site))) });
        assert_eq!(command(&pip), "/venv/bin/python -m pip install -U 'paddleocr>=3,<4'");
        let mut checkout = dist("2.9.1", site);
        checkout.direct_url = Some(serde_json::json!({ "dir_info": { "editable": true } }));
        let git = old(facts(Some(checkout), Some(dist("3.0.0", site))));
        assert!(command(&git).contains("git pull") && command(&git).contains("pip install -e ."), "{}", command(&git));
        // A system package is not touched: a separate environment is suggested.
        let system = old(facts(Some(dist("2.9.1", "/usr/lib/python3.12/site-packages")), Some(dist("3.0.0", "/usr/lib/python3.12/site-packages"))));
        let options = &system.problems.iter().find(|p| p.id == "paddleocr-version").unwrap().options;
        assert!(options.iter().all(|o| !o.commands.iter().any(|c| c.contains("sudo pip") || c.contains("pip install -U"))), "{options:?}");
        assert!(options.iter().any(|o| o.commands.iter().any(|c| c.contains("venv"))));
    }

    #[test]
    fn missing_paddlepaddle_and_broken_imports_are_reported() {
        let e = evaluate_facts(&facts(Some(dist("3.0.1", "/venv/lib/site-packages")), None), "eng", tools(true, Family::Arch));
        assert!(ids(&e).contains(&"paddle-missing") && !e.ready);
        let mut broken = facts(Some(dist("3.0.1", "/x")), Some(dist("3.0.0", "/x")));
        broken.import_errors.insert("paddle".into(), "ImportError: libcudart.so.12: cannot open shared object file".into());
        let e = evaluate_facts(&broken, "eng", tools(true, Family::Arch));
        let problem = e.problems.iter().find(|p| p.id == "import-failed").unwrap();
        assert!(problem.title.contains("paddle") && problem.detail.contains("libcudart") && !e.ready);
    }

    #[test]
    fn the_language_and_the_models_are_checked() {
        let installed = || { let mut f = facts(Some(dist("3.0.1", "/venv/s")), Some(dist("3.0.0", "/venv/s"))); f.venv = true; f };
        // A language the integration does not speak is an error whatever is installed.
        let e = evaluate_facts(&installed(), "ara", tools(true, Family::Arch));
        assert!(!e.ready && !e.language_supported && ids(&e).contains(&"language"), "{:?}", e.problems);
        // Only the first language of `jpn+eng` counts, as for the engine.
        let e = evaluate_facts(&installed(), "jpn+eng", tools(true, Family::Arch));
        assert_eq!((e.language.as_str(), e.paddle_language), ("jpn", Some("japan")));
        // Models of another language are no models for this one: a remark, not an error.
        let mut f = installed();
        f.models = vec!["korean_PP-OCRv5_mobile_rec".into()];
        let e = evaluate_facts(&f, "rus", tools(true, Family::Arch));
        assert!(e.ready, "a missing model does not stop PaddleOCR: it is downloaded");
        let note = e.problems.iter().find(|p| p.id == "models-missing").unwrap();
        assert_eq!(note.severity, "info");
        assert!(note.options[0].commands[0].contains("lang='ru'"), "{:?}", note.options);
        f.models.push("cyrillic_PP-OCRv3_mobile_rec".into());
        assert!(evaluate_facts(&f, "rus", tools(true, Family::Arch)).models_cached);
    }

    #[test]
    fn a_python_that_wheels_do_not_exist_for_gets_a_warning() {
        let mut f = facts(Some(dist("3.0.1", "/v")), Some(dist("3.0.0", "/v")));
        f.python_version = "3.14.5".into();
        let e = evaluate_facts(&f, "eng", tools(true, Family::Arch));
        assert!(e.ready, "a warning only");
        let w = e.problems.iter().find(|p| p.id == "python-version").unwrap();
        assert_eq!(w.severity, "warning");
        assert!(w.options[0].commands[0].contains("--python 3.12"));
    }

    #[test]
    fn a_missing_or_broken_python_is_the_first_thing_said() {
        let e = evaluate("/no/such/python", false, None, None, "eng", tools(false, Family::Debian), Path::new(HOME));
        assert!(!e.python_found && !e.ready);
        assert_eq!(e.problems[0].id, "python-missing");
        assert_eq!(e.problems[0].options[0].commands[0], "sudo apt install python3 python3-venv python3-pip");
        let e = evaluate("/usr/bin/python3", true, None, Some("Python ответил ошибкой: boom"), "eng", tools(false, Family::Arch), Path::new(HOME));
        assert_eq!(e.problems[0].id, "python-failed");
        assert!(e.problems[0].detail.contains("boom"));
        assert!(find_python("/bin/sh") && !find_python("/nonexistent/python") && !find_python("surely-not-a-program-name"));
    }

    // ── the real script, on a real Python, with the system's packages hidden (-S) ──

    fn fake_site(name: &str, ocr: Option<&str>, paddle: bool) -> PathBuf {
        let site = std::env::temp_dir().join(format!("lipax-paddle-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&site);
        std::fs::create_dir_all(&site).unwrap();
        let package = |dist: &str, version: &str, module: &str, init: &str| {
            let info = site.join(format!("{dist}-{version}.dist-info"));
            std::fs::create_dir_all(&info).unwrap();
            std::fs::write(info.join("METADATA"), format!("Metadata-Version: 2.1\nName: {dist}\nVersion: {version}\n")).unwrap();
            std::fs::create_dir_all(site.join(module)).unwrap();
            std::fs::write(site.join(module).join("__init__.py"), init).unwrap();
        };
        if let Some(version) = ocr { package("paddleocr", version, "paddleocr", "") }
        if paddle { package("paddlepaddle", "3.0.0", "paddle", "__version__ = '3.0.0'\n") }
        site
    }

    async fn run(site: &Path, lang: &str) -> PaddleEnv {
        inspect_with("python3", &["-S"], lang, &[("PYTHONPATH", site.to_str().unwrap()), ("HOME", site.to_str().unwrap())], tools(false, Family::Arch)).await
    }

    #[tokio::test]
    async fn the_script_sees_nothing_where_nothing_is_installed() {
        if !find_python("python3") { eprintln!("skipped: no python3"); return; }
        let e = run(&fake_site("none", None, false), "eng").await;
        assert!(e.python_found && !e.ready && e.install_kind == InstallKind::None, "{e:?}");
        assert!(ids(&e).contains(&"paddleocr-missing"), "{:?}", ids(&e));
        assert!(e.python_version.as_deref().is_some_and(|v| v.starts_with("3.")));
    }

    #[tokio::test]
    async fn the_script_finds_a_complete_install_and_imports_it() {
        if !find_python("python3") { eprintln!("skipped: no python3"); return; }
        let e = run(&fake_site("full", Some("3.0.1"), true), "eng").await;
        assert_eq!((e.paddleocr_version.as_deref(), e.paddle_version.as_deref()), (Some("3.0.1"), Some("3.0.0")), "{e:?}");
        assert!(!e.problems.iter().any(|p| p.severity == "error"), "{:?}", e.problems);
        assert!(e.ready);
    }

    #[tokio::test]
    async fn the_script_reports_a_wrong_version_and_a_missing_framework() {
        if !find_python("python3") { eprintln!("skipped: no python3"); return; }
        let e = run(&fake_site("old", Some("2.9.1"), false), "eng").await;
        assert!(ids(&e).contains(&"paddleocr-version") && ids(&e).contains(&"paddle-missing") && !e.ready, "{:?}", ids(&e));
    }
}
