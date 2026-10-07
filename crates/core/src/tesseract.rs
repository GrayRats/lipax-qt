//! `TesseractManager`: обнаружение Tesseract и его языковых данных в Linux.
//!
//! Все сведения берутся у самой системы: путь — из PATH, версия и список языков — от `tesseract`
//! (`--version`, `--list-langs`), состояние пакетов — от пакетного менеджера дистрибутива.
//! GUI сам ничего не запускает, а получает готовый [`TesseractInfo`].
//!
//! Автоматически ничего не устанавливается: [`TesseractManager::install_argv`] только описывает команду,
//! которую выполняет вызывающий код после явного подтверждения пользователя.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

// ───────────────────────── скачанные языки ─────────────────────────
//
// Не у каждого дистрибутива языковые модели есть в репозитории: в Arch/CachyOS пакетов
// `tesseract-data-*` в официальных репозиториях нет (только AUR). Тогда модель скачивается
// из официального `tessdata_fast` в пользовательский каталог — без прав администратора. Tesseract
// принимает один каталог данных, поэтому в нём лежат ссылки на системные модели рядом со скачанными.

/// Официальные быстрые модели Tesseract.
const TESSDATA_URL: &str = "https://github.com/tesseract-ocr/tessdata_fast/raw/main";
/// Меньше этого — не модель (страница ошибки, обрыв).
const MIN_MODEL_BYTES: u64 = 50_000;
const MAX_MODEL_BYTES: u64 = 200 * 1024 * 1024;

/// Каталог скачанных моделей.
pub fn user_tessdata_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_default().join("lipa/tessdata")
}

/// Системный каталог данных Tesseract.
pub fn system_tessdata_dir() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = std::env::var_os("TESSDATA_PREFIX").map(PathBuf::from).into_iter().collect();
    candidates.extend(["/usr/share/tessdata", "/usr/share/tesseract-ocr/5/tessdata", "/usr/share/tesseract-ocr/4.00/tessdata", "/usr/local/share/tessdata"].map(PathBuf::from));
    candidates.into_iter().find(|d| d.join("eng.traineddata").exists() || d.join("osd.traineddata").exists())
}

/// Коды скачанных (не ссылок) моделей в каталоге.
fn downloaded_languages(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut codes: Vec<String> = entries.flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".traineddata").map(String::from)).collect();
    codes.sort();
    codes
}

/// Ссылки на всё, что лежит в системном каталоге, в `user`: модели, но и `configs` с `tessconfigs` (без них не
/// находятся конфигурации вроде `tsv`, и Tesseract молча ничего не выводит). Существующее не трогается;
/// ссылка, ставшая битой (модель удалили из системы), убирается.
fn link_system_languages(user: &Path, system: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(user)?;
    for entry in std::fs::read_dir(user)?.flatten() {
        if entry.file_type().is_ok_and(|t| t.is_symlink()) && !entry.path().exists() { let _ = std::fs::remove_file(entry.path()); }
    }
    for entry in std::fs::read_dir(system)?.flatten() {
        let name = entry.file_name();
        if std::fs::symlink_metadata(user.join(&name)).is_err() {
            std::os::unix::fs::symlink(entry.path(), user.join(&name))?;
        }
    }
    Ok(())
}

/// Каталог, который надо передать Tesseract (`--tessdata-dir`), если есть скачанные модели; он же обновляется.
pub fn tessdata_arg_in(user: &Path, system: Option<&Path>) -> Option<PathBuf> {
    if downloaded_languages(user).is_empty() { return None; }
    if let Some(system) = system { let _ = link_system_languages(user, system); }
    Some(user.to_path_buf())
}

/// How long the answer of `tessdata_arg` is reused. It walks directories and may create links, and it is asked at every
/// reading (several a second, from several fields at once); a language installed a moment ago waits at most this long
/// (`download_language` clears the answer at once).
const TESSDATA_ARG_TTL: std::time::Duration = std::time::Duration::from_secs(5);

static TESSDATA_ARG: std::sync::Mutex<Option<(std::time::Instant, Option<PathBuf>)>> = std::sync::Mutex::new(None);

/// То же для текущего пользователя.
pub fn tessdata_arg() -> Option<PathBuf> {
    let mut cached = TESSDATA_ARG.lock().unwrap();
    if let Some((when, answer)) = cached.as_ref() && when.elapsed() < TESSDATA_ARG_TTL { return answer.clone(); }
    // The lock is held while the directories are walked: concurrent readings make the links once, not each their own.
    let answer = tessdata_arg_in(&user_tessdata_dir(), system_tessdata_dir().as_deref());
    *cached = Some((std::time::Instant::now(), answer.clone()));
    answer
}

/// Forget the answer of `tessdata_arg` (a language was added or removed).
pub fn forget_tessdata_arg() {
    *TESSDATA_ARG.lock().unwrap() = None;
}

/// Скачивает модель `code` из `tessdata_fast` в пользовательский каталог.
pub async fn download_language(code: &str) -> Result<PathBuf, String> {
    download_language_from(TESSDATA_URL, code, &user_tessdata_dir()).await
}

/// Скачивание с проверкой: только известный код языка, разумный размер, запись через временный файл.
pub async fn download_language_from(base_url: &str, code: &str, dir: &Path) -> Result<PathBuf, String> {
    if !KNOWN_LANGS.iter().any(|(c, _)| *c == code) { return Err(format!("неизвестный язык: {code}")); }
    let url = format!("{}/{code}.traineddata", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(180)).build().map_err(|e| e.to_string())?;
    let response = client.get(&url).send().await.map_err(|e| format!("не удалось скачать {url}: {e}"))?;
    if !response.status().is_success() { return Err(format!("{url}: сервер ответил {}", response.status())); }
    if response.content_length().is_some_and(|n| n > MAX_MODEL_BYTES) { return Err(format!("{url}: файл слишком большой")); }
    let bytes = response.bytes().await.map_err(|e| format!("обрыв загрузки {url}: {e}"))?;
    if (bytes.len() as u64) < MIN_MODEL_BYTES || bytes.len() as u64 > MAX_MODEL_BYTES {
        return Err(format!("{url}: получено {} байт — это не языковая модель", bytes.len()));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (target, partial) = (dir.join(format!("{code}.traineddata")), dir.join(format!("{code}.traineddata.part")));
    std::fs::write(&partial, &bytes).map_err(|e| format!("{}: {e}", partial.display()))?;
    std::fs::rename(&partial, &target).map_err(|e| format!("{}: {e}", target.display()))?;
    forget_tessdata_arg();
    Ok(target)
}

// ───────────────────────── запуск команд ─────────────────────────

#[derive(Debug, Clone, Default)]
pub struct CmdOut {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Запуск внешних команд; в тестах подменяется заготовленными ответами.
pub trait Runner {
    /// `None`, если программу не удалось запустить.
    fn run(&self, program: &str, args: &[&str]) -> Option<CmdOut>;
    /// Поиск исполняемого файла в PATH.
    fn which(&self, program: &str) -> Option<PathBuf>;
}

pub struct SystemRunner;

impl Runner for SystemRunner {
    fn run(&self, program: &str, args: &[&str]) -> Option<CmdOut> {
        let out = Command::new(program).args(args).output().ok()?;
        Some(CmdOut {
            ok: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }

    fn which(&self, program: &str) -> Option<PathBuf> {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path).map(|d| d.join(program)).find(|p| is_executable(p))
    }
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata().map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

// ───────────────────────── дистрибутив и пакетный менеджер ─────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Distro {
    pub id: String,
    pub name: String,
    pub family: Family,
}

/// Семейство дистрибутивов. Для Fedora/openSUSE добавляется вариант здесь и ветка в [`PackageManager`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    Arch,
    Debian,
    Other,
}

/// Разбор `/etc/os-release`: `ID` и `ID_LIKE` определяют семейство (CachyOS, Manjaro, Mint и т. д.).
pub fn parse_os_release(text: &str) -> Distro {
    let get = |key: &str| -> String {
        text.lines()
            .filter_map(|l| l.split_once('='))
            .find(|(k, _)| k.trim() == key)
            .map(|(_, v)| v.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
            .unwrap_or_default()
    };
    let id = get("ID").to_lowercase();
    let like = get("ID_LIKE").to_lowercase();
    let ids: Vec<&str> = std::iter::once(id.as_str()).chain(like.split_whitespace()).collect();
    let family = if ids.iter().any(|i| matches!(*i, "arch" | "cachyos" | "endeavouros" | "manjaro" | "archarm")) {
        Family::Arch
    } else if ids.iter().any(|i| matches!(*i, "debian" | "ubuntu" | "linuxmint" | "pop" | "raspbian")) {
        Family::Debian
    } else {
        Family::Other
    };
    let name = [get("PRETTY_NAME"), get("NAME"), id.clone()].into_iter().find(|s| !s.is_empty()).unwrap_or_default();
    Distro { id, name, family }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManager {
    Pacman,
    Apt,
    Unknown,
}

impl PackageManager {
    fn detect(family: Family, r: &impl Runner) -> Self {
        match family {
            Family::Arch if r.which("pacman").is_some() => Self::Pacman,
            Family::Debian if r.which("dpkg").is_some() => Self::Apt,
            _ => Self::Unknown,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Pacman => "pacman",
            Self::Apt => "apt",
            Self::Unknown => "unknown",
        }
    }

    /// Пакет самого Tesseract.
    pub fn engine_package(self) -> Option<&'static str> {
        match self {
            Self::Pacman => Some("tesseract"),
            Self::Apt => Some("tesseract-ocr"),
            Self::Unknown => None,
        }
    }

    /// Пакет языковой модели. В Debian подчёркивание в коде заменяется дефисом (`jpn_vert` → `-jpn-vert`).
    pub fn lang_package(self, code: &str) -> Option<String> {
        match self {
            Self::Pacman => Some(format!("tesseract-data-{code}")),
            Self::Apt => Some(format!("tesseract-ocr-{}", code.replace('_', "-"))),
            Self::Unknown => None,
        }
    }

    fn pkg_prefix(self) -> &'static str {
        match self {
            Self::Pacman => "tesseract-data-",
            _ => "tesseract-ocr-",
        }
    }

    pub fn is_installed(self, pkg: &str, r: &impl Runner) -> bool {
        match self {
            Self::Pacman => r.run("pacman", &["-Qi", pkg]).is_some_and(|o| o.ok),
            Self::Apt => r
                .run("dpkg-query", &["-W", "-f=${Status}", pkg])
                .is_some_and(|o| o.ok && o.stdout.contains("install ok installed")),
            Self::Unknown => false,
        }
    }

    /// Имена всех языковых пакетов, доступных в репозиториях (одним запросом).
    fn repo_lang_packages(self, r: &impl Runner) -> Vec<String> {
        let prefix = self.pkg_prefix();
        match self {
            Self::Pacman => r
                .run("pacman", &["-Ssq", &format!("^{prefix}")])
                .map(|o| o.stdout.lines().map(|l| l.trim().to_string()).collect())
                .unwrap_or_default(),
            Self::Apt => r
                .run("apt-cache", &["search", "--names-only", &format!("^{prefix}")])
                .map(|o| o.stdout.lines().filter_map(|l| l.split_whitespace().next()).map(String::from).collect())
                .unwrap_or_default(),
            Self::Unknown => vec![],
        }
    }

    /// Пакет, которому принадлежит файл.
    fn owner_of(self, path: &Path, r: &impl Runner) -> Option<String> {
        let p = path.to_str()?;
        match self {
            Self::Pacman => r.run("pacman", &["-Qoq", p]).filter(|o| o.ok).and_then(|o| first_line(&o.stdout)),
            Self::Apt => {
                // «пакет[:арх][, пакет2]: /путь»; при usr-merge dpkg знает только каноничный путь.
                let canon = path.canonicalize().ok();
                [Some(path.to_path_buf()), canon].into_iter().flatten().find_map(|c| {
                    let o = r.run("dpkg", &["-S", c.to_str()?])?;
                    let head = o.stdout.lines().next().filter(|_| o.ok)?.split_once(':')?.0;
                    head.split(',').next().map(|s| s.trim().to_string())
                })
            }
            Self::Unknown => None,
        }
    }

    /// Команда установки. `pkexec` показывает системный диалог пароля (не `sudo` из терминала).
    pub fn install_argv(self, pkg: &str) -> Option<Vec<String>> {
        let v: &[&str] = match self {
            Self::Pacman => &["pkexec", "pacman", "-S", "--needed", "--noconfirm", pkg],
            Self::Apt => &["pkexec", "apt-get", "install", "-y", pkg],
            Self::Unknown => return None,
        };
        Some(v.iter().map(|s| s.to_string()).collect())
    }
}

fn first_line(s: &str) -> Option<String> {
    s.lines().map(str::trim).find(|l| !l.is_empty()).map(String::from)
}

// ───────────────────────── языки ─────────────────────────

/// Известные языки: код Tesseract → название. Используется для подписей и списка «можно установить».
const KNOWN_LANGS: &[(&str, &str)] = &[
    ("eng", "English"),
    ("rus", "Russian"),
    ("ukr", "Ukrainian"),
    ("bel", "Belarusian"),
    ("jpn", "Japanese"),
    ("jpn_vert", "Japanese Vertical"),
    ("kor", "Korean"),
    ("kor_vert", "Korean Vertical"),
    ("chi_sim", "Chinese Simplified"),
    ("chi_sim_vert", "Chinese Simplified Vertical"),
    ("chi_tra", "Chinese Traditional"),
    ("chi_tra_vert", "Chinese Traditional Vertical"),
    ("deu", "German"),
    ("fra", "French"),
    ("spa", "Spanish"),
    ("ita", "Italian"),
    ("por", "Portuguese"),
    ("pol", "Polish"),
    ("nld", "Dutch"),
    ("tur", "Turkish"),
    ("ces", "Czech"),
    ("swe", "Swedish"),
    ("fin", "Finnish"),
    ("dan", "Danish"),
    ("nor", "Norwegian"),
    ("hun", "Hungarian"),
    ("ron", "Romanian"),
    ("bul", "Bulgarian"),
    ("ell", "Greek"),
    ("heb", "Hebrew"),
    ("ara", "Arabic"),
    ("hin", "Hindi"),
    ("tha", "Thai"),
    ("vie", "Vietnamese"),
    ("ind", "Indonesian"),
    ("lat", "Latin"),
];

pub fn lang_name(code: &str) -> String {
    KNOWN_LANGS.iter().find(|(c, _)| *c == code).map(|(_, n)| n.to_string()).unwrap_or_else(|| code.to_string())
}

/// Основной язык из строки Tesseract вида `jpn+eng` — язык источника для перевода.
pub fn primary_lang(spec: &str) -> &str {
    spec.split('+').map(str::trim).find(|s| !s.is_empty()).unwrap_or("eng")
}

/// Разбор вывода `tesseract --list-langs`: путь к tessdata (если есть) и коды языков.
pub fn parse_list_langs(out: &str) -> (Option<PathBuf>, Vec<String>) {
    let mut dir = None;
    let mut langs = Vec::new();
    for line in out.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if line.starts_with("List of available languages") {
            dir = line.split('"').nth(1).map(PathBuf::from);
        } else if !line.contains(' ') && !line.contains(':') && line != "osd" {
            langs.push(line.to_string());
        }
    }
    langs.sort();
    langs.dedup();
    (dir, langs)
}

/// «tesseract 5.5.3\n leptonica…» → «5.5.3».
pub fn parse_version(out: &str) -> Option<String> {
    out.lines().find_map(|l| l.trim().strip_prefix("tesseract ").map(|v| v.trim().to_string()))
}

// ───────────────────────── результат ─────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lang {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Installable {
    pub code: String,
    pub name: String,
    pub package: String,
    /// Пакет найден в репозиториях дистрибутива. Иначе возможна только ручная установка (например, из AUR).
    pub available: bool,
    /// Готовая команда для показа пользователю перед установкой.
    pub command: String,
    /// `package` — пакет дистрибутива (пароль администратора), `download` — модель скачивается
    /// в каталог пользователя (пароль не нужен). Второй способ — когда пакета в репозитории нет.
    #[serde(default)]
    pub method: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TesseractInfo {
    pub installed: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub tessdata: Option<String>,
    pub languages: Vec<Lang>,
    pub distro: Distro,
    pub package_manager: String,
    /// Пакет, которому принадлежит `tesseract`.
    pub engine_package: Option<String>,
    /// Пакет, которому принадлежат данные `tessdata`.
    pub tessdata_package: Option<String>,
    /// Языки, которых нет, но есть пакет в репозитории.
    pub installable: Vec<Installable>,
    pub engine_install_command: Option<String>,
}

/// Язык, выбранный пользователем, но отсутствующий в Tesseract.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MissingLang {
    pub code: String,
    pub name: String,
    pub package: Option<String>,
    pub message: String,
}

fn shell_join(argv: &[String]) -> String {
    argv.join(" ")
}

pub struct TesseractManager<R: Runner = SystemRunner> {
    runner: R,
    os_release: String,
    /// Каталог скачанных моделей; `None` — их нет (так в тестах).
    user_dir: Option<PathBuf>,
}

impl TesseractManager {
    pub fn system() -> Self {
        Self { user_dir: Some(user_tessdata_dir()), ..Self::new(SystemRunner, std::fs::read_to_string("/etc/os-release").unwrap_or_default()) }
    }
}

impl<R: Runner> TesseractManager<R> {
    pub fn new(runner: R, os_release: String) -> Self {
        Self { runner, os_release, user_dir: None }
    }

    fn pm(&self) -> (Distro, PackageManager) {
        let d = parse_os_release(&self.os_release);
        let pm = PackageManager::detect(d.family, &self.runner);
        (d, pm)
    }

    pub fn detect(&self) -> TesseractInfo {
        let (distro, pm) = self.pm();
        let path = self.runner.which("tesseract");
        let mut info = TesseractInfo {
            installed: path.is_some(),
            path: path.as_ref().map(|p| p.display().to_string()),
            version: None,
            tessdata: None,
            languages: vec![],
            distro,
            package_manager: pm.name().into(),
            engine_package: None,
            tessdata_package: None,
            installable: vec![],
            engine_install_command: pm.engine_package().and_then(|p| pm.install_argv(p)).map(|a| shell_join(&a)),
        };
        let Some(bin) = path else { return info };
        let bin_s = bin.to_string_lossy().into_owned();

        if let Some(o) = self.runner.run(&bin_s, &["--version"]) {
            info.version = parse_version(&o.stdout).or_else(|| parse_version(&o.stderr));
        }
        let mut langs = vec![];
        // Скачанные модели лежат в своём каталоге; Tesseract видит один каталог, поэтому спрашиваем его о нём.
        let data_dir = self.user_dir.as_ref().and_then(|u| tessdata_arg_in(u, system_tessdata_dir().as_deref()));
        let list_args: Vec<String> = match &data_dir {
            Some(dir) => vec!["--list-langs".into(), "--tessdata-dir".into(), dir.display().to_string()],
            None => vec!["--list-langs".into()],
        };
        if let Some(o) = self.runner.run(&bin_s, &list_args.iter().map(String::as_str).collect::<Vec<_>>()) {
            // Старые версии пишут список в stderr.
            let text = if o.stdout.trim().is_empty() { &o.stderr } else { &o.stdout };
            let (dir, l) = parse_list_langs(text);
            info.tessdata = dir.as_ref().map(|d| d.display().to_string());
            langs = l;
        }
        info.languages = langs.iter().map(|c| Lang { code: c.clone(), name: lang_name(c) }).collect();

        info.engine_package = pm.owner_of(&bin, &self.runner).or_else(|| {
            pm.engine_package().filter(|p| pm.is_installed(p, &self.runner)).map(String::from)
        });
        if let (Some(dir), Some(first)) = (&info.tessdata, langs.first()) {
            info.tessdata_package = pm.owner_of(&Path::new(dir).join(format!("{first}.traineddata")), &self.runner);
        }

        let repo = pm.repo_lang_packages(&self.runner);
        let target = self.user_dir.clone().unwrap_or_else(user_tessdata_dir);
        info.installable = KNOWN_LANGS
            .iter()
            .filter(|(c, _)| !langs.iter().any(|l| l == c))
            .map(|(c, n)| {
                let package = pm.lang_package(c).unwrap_or_default();
                let in_repo = !package.is_empty() && repo.contains(&package);
                match pm.install_argv(&package).filter(|_| in_repo) {
                    Some(argv) => Installable { code: c.to_string(), name: n.to_string(), available: true, package, command: shell_join(&argv), method: "package".into() },
                    // Нет пакета в репозитории (Arch: только AUR) или неизвестный менеджер: скачивание модели.
                    None => Installable { code: c.to_string(), name: n.to_string(), available: true, package,
                        command: format!("Скачать {TESSDATA_URL}/{c}.traineddata в {}", target.display()), method: "download".into() },
                }
            })
            .collect();
        info
    }

    /// Какие из выбранных языков (`jpn+eng`) не установлены.
    pub fn missing(&self, spec: &str, installed: &[Lang]) -> Vec<MissingLang> {
        let (distro, pm) = self.pm();
        let _ = distro;
        spec.split('+')
            .map(str::trim)
            .filter(|c| !c.is_empty() && !installed.iter().any(|l| l.code == *c))
            .map(|c| {
                let name = lang_name(c);
                let package = pm.lang_package(c);
                let message = match &package {
                    Some(p) => format!("Не установлен языковой пакет Tesseract ({name}, {c}): {p}"),
                    None => format!("Не установлена языковая модель Tesseract: {name} ({c})"),
                };
                MissingLang { code: c.into(), name, package, message }
            })
            .collect()
    }

    /// Команда установки пакета для кнопки «Установить языковой пакет»; `None` для неизвестной системы.
    pub fn install_argv(&self, package: &str) -> Option<Vec<String>> {
        self.pm().1.install_argv(package)
    }

    /// Допустимо ли имя пакета для установки: только языковые пакеты Tesseract (и сам движок).
    pub fn is_allowed_package(&self, package: &str) -> bool {
        let pm = self.pm().1;
        Some(package) == pm.engine_package()
            || (package.starts_with(pm.pkg_prefix())
                && package.len() > pm.pkg_prefix().len()
                && package.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
    }

    /// Понятное сообщение для ошибки OCR: нет Tesseract или нет языка из `spec`.
    pub fn explain_ocr_failure(&self, spec: &str) -> Option<String> {
        let info = self.detect();
        if !info.installed {
            return Some(match self.pm().1.engine_package() {
                Some(p) => format!("Tesseract не установлен: пакет {p}"),
                None => "Tesseract не установлен".into(),
            });
        }
        let missing = self.missing(spec, &info.languages);
        (!missing.is_empty()).then(|| missing.iter().map(|m| m.message.as_str()).collect::<Vec<_>>().join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Заготовленные ответы: ключ — «программа аргументы через пробел».
    #[derive(Default)]
    struct Fake {
        bins: Vec<&'static str>,
        out: HashMap<String, CmdOut>,
    }

    impl Fake {
        fn on(mut self, cmd: &str, ok: bool, stdout: &str) -> Self {
            self.out.insert(cmd.into(), CmdOut { ok, stdout: stdout.into(), stderr: String::new() });
            self
        }
    }

    impl Runner for Fake {
        fn run(&self, program: &str, args: &[&str]) -> Option<CmdOut> {
            let key = std::iter::once(program).chain(args.iter().copied()).collect::<Vec<_>>().join(" ");
            self.out.get(&key).cloned().or(Some(CmdOut::default()))
        }
        fn which(&self, program: &str) -> Option<PathBuf> {
            self.bins.contains(&program).then(|| PathBuf::from("/usr/bin").join(program))
        }
    }

    const CACHY: &str = "NAME=\"CachyOS Linux\"\nPRETTY_NAME=\"CachyOS\"\nID=cachyos\nID_LIKE=arch\n";
    const UBUNTU: &str = "PRETTY_NAME=\"Ubuntu 24.04.2 LTS\"\nNAME=\"Ubuntu\"\nID=ubuntu\nID_LIKE=debian\n";
    const MINT: &str = "NAME=\"Linux Mint\"\nID=linuxmint\nID_LIKE=\"ubuntu debian\"\n";

    #[test]
    fn os_release_families() {
        assert_eq!(parse_os_release(CACHY).family, Family::Arch);
        assert_eq!(parse_os_release(CACHY).name, "CachyOS");
        assert_eq!(parse_os_release("ID=manjaro\nID_LIKE=arch").family, Family::Arch);
        assert_eq!(parse_os_release("ID=endeavouros\nID_LIKE=arch").family, Family::Arch);
        assert_eq!(parse_os_release("ID=arch").family, Family::Arch);
        assert_eq!(parse_os_release(UBUNTU).family, Family::Debian);
        assert_eq!(parse_os_release(MINT).family, Family::Debian);
        assert_eq!(parse_os_release("ID=debian").family, Family::Debian);
        assert_eq!(parse_os_release("ID=fedora").family, Family::Other);
    }

    #[test]
    fn package_names_differ_by_distro() {
        assert_eq!(PackageManager::Pacman.lang_package("jpn").as_deref(), Some("tesseract-data-jpn"));
        assert_eq!(PackageManager::Pacman.lang_package("chi_sim").as_deref(), Some("tesseract-data-chi_sim"));
        assert_eq!(PackageManager::Apt.lang_package("jpn").as_deref(), Some("tesseract-ocr-jpn"));
        assert_eq!(PackageManager::Apt.lang_package("jpn_vert").as_deref(), Some("tesseract-ocr-jpn-vert"));
        assert_eq!(PackageManager::Pacman.engine_package(), Some("tesseract"));
        assert_eq!(PackageManager::Apt.engine_package(), Some("tesseract-ocr"));
        assert_eq!(PackageManager::Unknown.lang_package("jpn"), None);
    }

    #[test]
    fn parses_tesseract_output() {
        let out = "List of available languages in \"/usr/share/tessdata/\" (5):\neng\njpn\njpn_vert\nosd\nrus\n";
        let (dir, langs) = parse_list_langs(out);
        assert_eq!(dir, Some(PathBuf::from("/usr/share/tessdata/")));
        assert_eq!(langs, ["eng", "jpn", "jpn_vert", "rus"]);
        // Старый формат без пути.
        let (dir, langs) = parse_list_langs("List of available languages (2):\neng\nrus\n");
        assert_eq!((dir, langs.len()), (None, 2));
        assert_eq!(parse_version("tesseract 5.5.3\n leptonica-1.87.0\n").as_deref(), Some("5.5.3"));
        assert_eq!(parse_version("tesseract 4.1.1\n"), Some("4.1.1".into()));
    }

    #[test]
    fn primary_language() {
        assert_eq!(primary_lang("jpn+eng"), "jpn");
        assert_eq!(primary_lang("eng"), "eng");
        assert_eq!(primary_lang(""), "eng");
    }

    fn arch() -> TesseractManager<Fake> {
        let r = Fake { bins: vec!["tesseract", "pacman"], ..Default::default() }
            .on("/usr/bin/tesseract --version", true, "tesseract 5.5.3\n leptonica-1.87.0\n")
            .on(
                "/usr/bin/tesseract --list-langs",
                true,
                "List of available languages in \"/usr/share/tessdata/\" (4):\neng\njpn\nosd\nrus\n",
            )
            .on("pacman -Qoq /usr/bin/tesseract", true, "tesseract\n")
            .on("pacman -Qoq /usr/share/tessdata/eng.traineddata", true, "tesseract-data-eng\n")
            .on("pacman -Ssq ^tesseract-data-", true, "tesseract-data-eng\ntesseract-data-deu\ntesseract-data-jpn\ntesseract-data-rus\n");
        TesseractManager::new(r, CACHY.into())
    }

    #[test]
    fn detects_on_arch() {
        let i = arch().detect();
        assert!(i.installed);
        assert_eq!(i.version.as_deref(), Some("5.5.3"));
        assert_eq!(i.tessdata.as_deref(), Some("/usr/share/tessdata/"));
        assert_eq!(i.package_manager, "pacman");
        assert_eq!(i.engine_package.as_deref(), Some("tesseract"));
        assert_eq!(i.tessdata_package.as_deref(), Some("tesseract-data-eng"));
        assert_eq!(i.languages.iter().map(|l| l.code.as_str()).collect::<Vec<_>>(), ["eng", "jpn", "rus"]);
        let deu = i.installable.iter().find(|l| l.code == "deu").unwrap();
        assert!(deu.available);
        assert_eq!(deu.package, "tesseract-data-deu");
        assert_eq!(deu.command, "pkexec pacman -S --needed --noconfirm tesseract-data-deu");
        assert_eq!(deu.method, "package");
        // Пакета нет в репозитории (так в Arch для большинства языков) — модель скачивается без пароля.
        let ara = i.installable.iter().find(|l| l.code == "ara").unwrap();
        assert!(ara.available && ara.method == "download", "{ara:?}");
        assert!(ara.command.contains("tessdata_fast/raw/main/ara.traineddata"), "{}", ara.command);
        assert!(i.installable.iter().all(|l| l.code != "eng"));
    }

    fn ubuntu() -> TesseractManager<Fake> {
        let r = Fake { bins: vec!["tesseract", "dpkg", "apt-cache"], ..Default::default() }
            .on("/usr/bin/tesseract --version", true, "tesseract 5.3.4\n leptonica-1.82.0\n")
            .on(
                "/usr/bin/tesseract --list-langs",
                true,
                "List of available languages in \"/usr/share/tesseract-ocr/5/tessdata/\" (3):\neng\nosd\nrus\n",
            )
            .on("dpkg -S /usr/bin/tesseract", true, "tesseract-ocr: /usr/bin/tesseract\n")
            .on(
                "dpkg -S /usr/share/tesseract-ocr/5/tessdata/eng.traineddata",
                true,
                "tesseract-ocr-eng: /usr/share/tesseract-ocr/5/tessdata/eng.traineddata\n",
            )
            .on(
                "apt-cache search --names-only ^tesseract-ocr-",
                true,
                "tesseract-ocr-eng - tesseract-ocr language files for English\ntesseract-ocr-jpn - tesseract-ocr language files for Japanese\ntesseract-ocr-jpn-vert - tesseract-ocr language files for Japanese vertical\n",
            );
        TesseractManager::new(r, UBUNTU.into())
    }

    #[test]
    fn detects_on_ubuntu() {
        let i = ubuntu().detect();
        assert_eq!(i.package_manager, "apt");
        assert_eq!(i.engine_package.as_deref(), Some("tesseract-ocr"));
        assert_eq!(i.tessdata_package.as_deref(), Some("tesseract-ocr-eng"));
        let jpn = i.installable.iter().find(|l| l.code == "jpn").unwrap();
        assert!(jpn.available);
        assert_eq!(jpn.package, "tesseract-ocr-jpn");
        assert_eq!(jpn.command, "pkexec apt-get install -y tesseract-ocr-jpn");
        assert!(i.installable.iter().find(|l| l.code == "jpn_vert").unwrap().available);
        assert_eq!(i.installable.iter().find(|l| l.code == "jpn_vert").unwrap().package, "tesseract-ocr-jpn-vert");
    }

    #[test]
    fn missing_language_messages() {
        let m = arch();
        let info = m.detect();
        let miss = m.missing("deu+eng", &info.languages);
        assert_eq!(miss.len(), 1);
        assert_eq!(miss[0].message, "Не установлен языковой пакет Tesseract (German, deu): tesseract-data-deu");
        let u = ubuntu();
        let info = u.detect();
        assert_eq!(
            u.missing("jpn", &info.languages)[0].message,
            "Не установлен языковой пакет Tesseract (Japanese, jpn): tesseract-ocr-jpn"
        );
        assert!(m.missing("eng+rus", &m.detect().languages).is_empty());
    }

    #[test]
    fn tesseract_not_installed() {
        let m = TesseractManager::new(Fake { bins: vec!["pacman"], ..Default::default() }, CACHY.into());
        let i = m.detect();
        assert!(!i.installed);
        assert!(i.languages.is_empty());
        assert_eq!(m.explain_ocr_failure("eng").as_deref(), Some("Tesseract не установлен: пакет tesseract"));
        let u = TesseractManager::new(Fake { bins: vec!["dpkg"], ..Default::default() }, UBUNTU.into());
        assert_eq!(u.explain_ocr_failure("eng").as_deref(), Some("Tesseract не установлен: пакет tesseract-ocr"));
    }

    #[test]
    fn unknown_distro_has_no_install_command() {
        let m = TesseractManager::new(Fake { bins: vec!["tesseract"], ..Default::default() }, "ID=fedora".into());
        assert_eq!(m.install_argv("tesseract-langpack-jpn"), None);
        assert_eq!(m.missing("jpn", &[])[0].package, None);
    }

    #[test]
    fn install_only_tesseract_packages() {
        let m = arch();
        assert!(m.is_allowed_package("tesseract-data-jpn"));
        assert!(m.is_allowed_package("tesseract"));
        assert!(!m.is_allowed_package("firefox"));
        assert!(!m.is_allowed_package("tesseract-data-jpn; rm -rf /"));
        assert!(!m.is_allowed_package("tesseract-data-"));
        assert!(ubuntu().is_allowed_package("tesseract-ocr-jpn-vert"));
        assert!(!ubuntu().is_allowed_package("tesseract-data-jpn"));
    }

    // ── Скачанные языки ──

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lipax-tess-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn with_nothing_downloaded_the_system_data_is_used_as_it_is() {
        let (user, system) = (scratch("u0"), scratch("s0"));
        std::fs::write(system.join("eng.traineddata"), b"x").unwrap();
        assert_eq!(tessdata_arg_in(&user, Some(&system)), None);
        assert!(std::fs::read_dir(&user).unwrap().next().is_none(), "nothing is linked when nothing was downloaded");
    }

    #[test]
    fn a_downloaded_model_sits_beside_links_to_the_system_ones() {
        let (user, system) = (scratch("u1"), scratch("s1"));
        for c in ["eng", "rus", "osd"] { std::fs::write(system.join(format!("{c}.traineddata")), b"system").unwrap(); }
        std::fs::write(system.join("pdf.ttf"), b"font").unwrap();
        std::fs::create_dir_all(system.join("configs")).unwrap();
        std::fs::write(system.join("configs/tsv"), b"tessedit_create_tsv 1\n").unwrap();
        std::fs::write(user.join("deu.traineddata"), b"downloaded").unwrap();
        assert_eq!(tessdata_arg_in(&user, Some(&system)), Some(user.clone()));
        let listed = |dir: &Path| { let mut v: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect(); v.sort(); v };
        assert_eq!(listed(&user), ["configs", "deu.traineddata", "eng.traineddata", "osd.traineddata", "pdf.ttf", "rus.traineddata"], "models and the rest of the directory");
        assert!(user.join("configs/tsv").exists(), "the configurations are reachable: without them `tsv` is not found");
        assert_eq!(std::fs::read(user.join("rus.traineddata")).unwrap(), b"system");
        assert_eq!(downloaded_languages(&user), ["deu"], "links are not downloads");
        // A model the user got later does not collide with a link; one removed from the system loses its link.
        std::fs::remove_file(system.join("rus.traineddata")).unwrap();
        std::fs::write(system.join("fra.traineddata"), b"new").unwrap();
        assert!(tessdata_arg_in(&user, Some(&system)).is_some());
        assert_eq!(listed(&user), ["configs", "deu.traineddata", "eng.traineddata", "fra.traineddata", "osd.traineddata", "pdf.ttf"]);
        assert_eq!(std::fs::read(user.join("deu.traineddata")).unwrap(), b"downloaded");
    }

    #[test]
    fn a_manager_with_downloads_asks_tesseract_about_the_merged_directory() {
        let (user, system) = (scratch("u2"), scratch("s2"));
        std::fs::write(user.join("deu.traineddata"), b"downloaded").unwrap();
        let r = Fake { bins: vec!["tesseract", "pacman"], ..Default::default() }
            .on(&format!("/usr/bin/tesseract --list-langs --tessdata-dir {}", user.display()), true,
                &format!("List of available languages in \"{}\" (3):\neng\ndeu\nosd\n", user.display()))
            .on("pacman -Ssq ^tesseract-data-", true, "");
        let manager = TesseractManager { user_dir: Some(user.clone()), ..TesseractManager::new(r, "ID=arch\nID_LIKE=\"\"\n".into()) };
        let _ = system;
        let info = manager.detect();
        assert_eq!(info.languages.iter().map(|l| l.code.as_str()).collect::<Vec<_>>(), ["deu", "eng"]);
        assert_eq!(info.tessdata.as_deref(), Some(user.to_str().unwrap()));
        assert!(info.installable.iter().all(|l| l.code != "deu"), "an installed language is not offered again");
    }

    /// Answers one HTTP request with `status` and `body`.
    async fn serve_once(status: &'static str, body: Vec<u8>) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 2048];
            let _ = socket.read(&mut request).await;
            let head = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            let _ = socket.write_all(head.as_bytes()).await;
            let _ = socket.write_all(&body).await;
        });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn a_model_is_downloaded_into_the_users_directory() {
        let dir = scratch("dl-ok");
        let base = serve_once("200 OK", vec![7u8; 120_000]).await;
        let path = download_language_from(&base, "deu", &dir).await.unwrap();
        assert_eq!(path, dir.join("deu.traineddata"));
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 120_000);
        assert!(!dir.join("deu.traineddata.part").exists(), "written through a temporary file");
    }

    #[tokio::test]
    async fn bad_downloads_leave_nothing_behind() {
        let dir = scratch("dl-bad");
        // An error page, a truncated file and an unknown language are all refused.
        let missing = serve_once("404 Not Found", b"not found".to_vec()).await;
        assert!(download_language_from(&missing, "deu", &dir).await.unwrap_err().contains("404"));
        let tiny = serve_once("200 OK", b"<html>oops</html>".to_vec()).await;
        assert!(download_language_from(&tiny, "deu", &dir).await.unwrap_err().contains("не языковая модель"));
        // No request is made for a code that is not a known language (nothing listens on this address).
        assert!(download_language_from("http://127.0.0.1:1", "../../etc/passwd", &dir).await.unwrap_err().contains("неизвестный язык"));
        assert!(download_language_from("http://127.0.0.1:1", "xyz", &dir).await.unwrap_err().contains("неизвестный язык"));
        assert!(!dir.join("deu.traineddata").exists());
        let unreachable = download_language_from("http://127.0.0.1:1", "deu", &dir).await.unwrap_err();
        assert!(unreachable.contains("не удалось скачать"), "{unreachable}");
    }

    /// The real thing, on the real network and the real Tesseract: `cargo test -p lipa-core -- --ignored real_download`.
    #[tokio::test]
    #[ignore = "needs network and Tesseract"]
    async fn real_download_gives_a_language_tesseract_can_use() {
        let (user, system) = (scratch("real-u"), system_tessdata_dir().expect("a system tessdata directory"));
        let path = download_language_from(TESSDATA_URL, "deu", &user).await.expect("download");
        assert!(std::fs::metadata(&path).unwrap().len() > 500_000);
        let dir = tessdata_arg_in(&user, Some(&system)).expect("merged directory");
        let listed = Command::new("tesseract").args(["--list-langs", "--tessdata-dir"]).arg(&dir).output().unwrap();
        let text = String::from_utf8_lossy(&listed.stdout);
        let (_, langs) = parse_list_langs(&text);
        assert!(langs.contains(&"deu".to_string()) && langs.contains(&"eng".to_string()), "downloaded and system languages together: {langs:?}");
        let _ = std::fs::remove_dir_all(&user);
    }

    /// The regression: with downloaded languages Tesseract is given the user's directory, and the `tsv` configuration
    /// has to be found there too, or it prints nothing at all ("Can't open tsv") and nothing is ever recognised.
    #[test]
    fn tesseract_finds_its_configurations_in_the_merged_directory() {
        let Some(system) = system_tessdata_dir().filter(|s| s.join("configs").exists()) else { eprintln!("skipped: no system tessdata"); return };
        if Command::new("tesseract").arg("--version").output().is_err() { eprintln!("skipped: no tesseract"); return; }
        let user = scratch("merged-real");
        std::fs::copy(system.join("eng.traineddata"), user.join("deu.traineddata")).unwrap();
        let dir = tessdata_arg_in(&user, Some(&system)).expect("merged directory");
        let png = user.join("blank.png");
        image::RgbaImage::from_pixel(60, 30, image::Rgba([255, 255, 255, 255])).save(&png).unwrap();
        let out = Command::new("tesseract").arg(&png).arg("stdout").args(["-l", "deu", "--tessdata-dir"]).arg(&dir).arg("tsv").output().unwrap();
        let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(!stderr.contains("Can't open"), "{stderr}");
        assert!(stdout.starts_with("level\tpage_num"), "the TSV header at least: {stdout:?}");
        let _ = std::fs::remove_dir_all(&user);
    }
}
