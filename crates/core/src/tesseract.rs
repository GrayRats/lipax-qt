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
}

impl TesseractManager {
    pub fn system() -> Self {
        Self::new(SystemRunner, std::fs::read_to_string("/etc/os-release").unwrap_or_default())
    }
}

impl<R: Runner> TesseractManager<R> {
    pub fn new(runner: R, os_release: String) -> Self {
        Self { runner, os_release }
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
        if let Some(o) = self.runner.run(&bin_s, &["--list-langs"]) {
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
        info.installable = KNOWN_LANGS
            .iter()
            .filter(|(c, _)| !langs.iter().any(|l| l == c))
            .filter_map(|(c, n)| {
                let package = pm.lang_package(c)?;
                let command = shell_join(&pm.install_argv(&package)?);
                Some(Installable { code: c.to_string(), name: n.to_string(), available: repo.contains(&package), package, command })
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
        // Пакета нет в репозитории — помечается недоступным, автоустановка не предлагается.
        assert!(!i.installable.iter().find(|l| l.code == "ara").unwrap().available);
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
}
