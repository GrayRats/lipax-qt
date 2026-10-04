//! Один экземпляр LipaX и командная строка.
//!
//! Первый процесс занимает на сессионной шине имя `io.lipa.Translator` и обслуживает
//! стандартный интерфейс `org.freedesktop.Application` (`Activate`, `ActivateAction`): так же
//! работают Desktop Actions KDE и `KDBusService`. Повторный запуск (`lipax --show`, `--quit`, …
//! или просто `lipax`) не открывает второй процесс, а передаёт действие первому и завершается.
//! Действия доходят до `Controller` через очередь; QML выполняет их в одном месте.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tokio::sync::mpsc;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{connection, interface};

const APP_ID: &str = "io.lipa.Translator";
const OBJECT_PATH: &str = "/io/lipa/Translator";
const APPLICATION_INTERFACE: &str = "org.freedesktop.Application";

/// Что можно попросить у приложения из командной строки, меню трея и Desktop Actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Show,
    Settings,
    Capture,
    StartAutoTranslate,
    StopAutoTranslate,
    Quit,
}

impl Action {
    const ALL: [Action; 6] = [Self::Show, Self::Settings, Self::Capture, Self::StartAutoTranslate, Self::StopAutoTranslate, Self::Quit];

    /// Имя действия в QML (`Controller::actionRequested`) и в D-Bus `ActivateAction`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Show => "show",
            Self::Settings => "settings",
            Self::Capture => "capture",
            Self::StartAutoTranslate => "start-autotranslate",
            Self::StopAutoTranslate => "stop-autotranslate",
            Self::Quit => "quit",
        }
    }

    /// Идентификатор секции `[Desktop Action …]` в `.desktop`-файле.
    fn desktop_id(self) -> &'static str {
        match self {
            Self::Show => "ShowWindow",
            Self::Settings => "Settings",
            Self::Capture => "CaptureWindow",
            Self::StartAutoTranslate => "StartAutoTranslate",
            Self::StopAutoTranslate => "StopAutoTranslate",
            Self::Quit => "Quit",
        }
    }

    fn flag(self) -> String {
        format!("--{}", self.name())
    }

    /// Принимает и имя действия, и идентификатор Desktop Action (`ActivateAction` от оболочки).
    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.name() == name || a.desktop_id() == name)
    }
}

/// Запрос действия: что сделать и токен активации окна от того, кто его вызвал.
#[derive(Debug, Clone)]
pub struct Request {
    pub action: Action,
    pub activation_token: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Cli {
    Run(Option<Action>),
    Help,
    Version,
}

pub fn usage() -> String {
    let mut text = String::from("Использование: lipax [ПАРАМЕТР]\n\nБез параметров запускает LipaX или показывает уже запущенный экземпляр.\n\n");
    for (action, help) in [
        (Action::Show, "показать главное окно"),
        (Action::Settings, "открыть настройки"),
        (Action::Capture, "выбрать окно игры для захвата"),
        (Action::StartAutoTranslate, "запустить автоперевод"),
        (Action::StopAutoTranslate, "остановить автоперевод"),
        (Action::Quit, "полностью завершить LipaX"),
    ] {
        text.push_str(&format!("  {:<24}{help}\n", action.flag()));
    }
    text.push_str("  -h, --help              эта справка\n  -V, --version           версия\n");
    text
}

/// Разбор аргументов (без имени программы). Параметры Qt (`-platform` и подобные, с одним
/// дефисом) пропускаются: их разбирает сам Qt. Неизвестный `--параметр` — ошибка.
pub fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<Cli, String> {
    let mut action: Option<Action> = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Cli::Help),
            "-V" | "--version" => return Ok(Cli::Version),
            // У этих параметров Qt есть значение в следующем аргументе.
            "-platform" | "-style" | "-qmljsdebugger" | "-display" => { args.next(); }
            other => match Action::ALL.into_iter().find(|a| a.flag() == other) {
                Some(next) if action.is_some_and(|prev| prev != next) =>
                    return Err(format!("параметры {} и {other} нельзя использовать вместе", action.unwrap().flag())),
                Some(next) => action = Some(next),
                None if other.starts_with("--") => return Err(format!("неизвестный параметр: {other}")),
                None => {}
            },
        }
    }
    Ok(Cli::Run(action))
}

/// Имя на шине. `LIPAX_INSTANCE_ID` отделяет экземпляр (тесты, отладка) от рабочего.
fn bus_name() -> String {
    match std::env::var("LIPAX_INSTANCE_ID") {
        Ok(id) if !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') => format!("{APP_ID}.{id}"),
        _ => APP_ID.to_owned(),
    }
}

struct Application {
    requests: mpsc::UnboundedSender<Request>,
}

impl Application {
    fn forward(&self, action: Action, platform_data: &HashMap<String, OwnedValue>) {
        let activation_token = platform_data.get("activation-token")
            .and_then(|v| <&str>::try_from(&**v).ok())
            .unwrap_or_default().to_owned();
        let _ = self.requests.send(Request { action, activation_token });
    }
}

#[interface(name = "org.freedesktop.Application")]
impl Application {
    fn activate(&self, platform_data: HashMap<String, OwnedValue>) {
        self.forward(Action::Show, &platform_data);
    }

    fn open(&self, _uris: Vec<String>, platform_data: HashMap<String, OwnedValue>) {
        self.forward(Action::Show, &platform_data);
    }

    fn activate_action(&self, action_name: String, _parameter: Vec<OwnedValue>, platform_data: HashMap<String, OwnedValue>) {
        match Action::from_name(&action_name) {
            Some(action) => self.forward(action, &platform_data),
            None => tracing::warn!(target: "instance", action = %action_name, "неизвестное действие D-Bus"),
        }
    }
}

pub enum Startup {
    /// Этот процесс — основной экземпляр; `Request` из командной строки уже в очереди.
    Primary,
    /// Действие передано уже запущенному экземпляру; этот процесс должен завершиться.
    Forwarded,
    /// Запущенного экземпляра нет, а действие ничего не запускает (`--quit`).
    NothingToDo,
}

struct Primary {
    // Пока соединение живо, имя на шине принадлежит нам.
    _connection: zbus::Connection,
    requests: Mutex<Option<mpsc::UnboundedReceiver<Request>>>,
}

static PRIMARY: OnceLock<Primary> = OnceLock::new();

/// Занять имя на шине или передать `action` тому, кто его уже занял. Вызывается до создания
/// `QApplication`. Без сессионной шины приложение работает как раньше, но без единственного
/// экземпляра и без действий извне.
pub fn acquire(action: Option<Action>) -> Startup {
    let name = bus_name();
    let result = crate::bridge::rt().block_on(async {
        let (tx, rx) = mpsc::unbounded_channel();
        let built = connection::Builder::session()?
            // Второй экземпляр не должен вытеснять первый (по умолчанию zbus разрешает замену).
            .allow_name_replacements(false)
            .replace_existing_names(false)
            .serve_at(OBJECT_PATH, Application { requests: tx.clone() })?
            .name(name.as_str())?
            .build()
            .await;
        match built {
            Ok(connection) => Ok(Some((connection, tx, rx))),
            Err(zbus::Error::NameTaken) => Ok(None),
            Err(e) => Err(e),
        }
    });
    match result {
        Ok(Some((connection, tx, rx))) => {
            if action == Some(Action::Quit) {
                eprintln!("LipaX не запущен");
                return Startup::NothingToDo;
            }
            if let Some(action) = action {
                let _ = tx.send(Request { action, activation_token: activation_token_from_env() });
            }
            let _ = PRIMARY.set(Primary { _connection: connection, requests: Mutex::new(Some(rx)) });
            Startup::Primary
        }
        Ok(None) => match crate::bridge::rt().block_on(forward(&name, action)) {
            Ok(()) => Startup::Forwarded,
            Err(e) => {
                // Владелец имени есть, но не отвечает: лучше сообщить, чем молча запустить копию.
                eprintln!("LipaX уже запущен, но не принимает команды: {e}");
                std::process::exit(1);
            }
        },
        Err(e) => {
            tracing::warn!(target: "instance", error = %e, "сессионная шина D-Bus недоступна: единственный экземпляр и действия извне отключены");
            if action == Some(Action::Quit) { Startup::NothingToDo } else { Startup::Primary }
        }
    }
}

/// Токен xdg-activation, который оболочка передаёт запущенному `Exec` процессу.
fn activation_token_from_env() -> String {
    std::env::var("XDG_ACTIVATION_TOKEN").or_else(|_| std::env::var("DESKTOP_STARTUP_ID")).unwrap_or_default()
}

async fn forward(name: &str, action: Option<Action>) -> zbus::Result<()> {
    let connection = connection::Builder::session()?.build().await?;
    let mut platform_data: HashMap<&str, Value<'_>> = HashMap::new();
    let token = activation_token_from_env();
    if !token.is_empty() { platform_data.insert("activation-token", Value::from(token.as_str())); }
    let destination = Some(name);
    let path = OBJECT_PATH;
    let interface = Some(APPLICATION_INTERFACE);
    match action {
        None => connection.call_method(destination, path, interface, "Activate", &(platform_data,)).await?,
        Some(action) => connection.call_method(destination, path, interface, "ActivateAction", &(action.name(), Vec::<Value<'_>>::new(), platform_data)).await?,
    };
    Ok(())
}

/// Забрать очередь запросов; вызывается один раз при создании `Controller`.
pub fn take_requests() -> Option<mpsc::UnboundedReceiver<Request>> {
    PRIMARY.get()?.requests.lock().unwrap().take()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, String> { parse_args(args.iter().map(|a| a.to_string())) }

    #[test]
    fn every_action_has_a_flag_and_round_trips() {
        for action in Action::ALL {
            assert_eq!(parse(&[&action.flag()]), Ok(Cli::Run(Some(action))));
            assert_eq!(Action::from_name(action.name()), Some(action));
            assert_eq!(Action::from_name(action.desktop_id()), Some(action));
        }
        assert_eq!(Action::from_name("bogus"), None);
    }

    #[test]
    fn no_arguments_just_run() {
        assert_eq!(parse(&[]), Ok(Cli::Run(None)));
        assert_eq!(parse(&["--help"]), Ok(Cli::Help));
        assert_eq!(parse(&["-V"]), Ok(Cli::Version));
    }

    #[test]
    fn qt_options_pass_through_and_unknown_ones_fail() {
        assert_eq!(parse(&["-platform", "offscreen", "--show"]), Ok(Cli::Run(Some(Action::Show))));
        assert!(parse(&["--nonsense"]).is_err());
        assert!(parse(&["--show", "--quit"]).is_err());
        assert_eq!(parse(&["--show", "--show"]), Ok(Cli::Run(Some(Action::Show))));
    }

    #[test]
    fn desktop_file_actions_match_the_cli() {
        let desktop = include_str!("../../../packaging/io.lipa.Translator.desktop");
        for action in Action::ALL {
            assert!(desktop.contains(&format!("[Desktop Action {}]", action.desktop_id())), "{:?}", action);
            assert!(desktop.contains(&format!("Exec=/usr/bin/lipax {}\n", action.flag())), "{:?}", action);
        }
        let listed = desktop.lines().find_map(|l| l.strip_prefix("Actions=")).unwrap();
        for action in Action::ALL { assert!(listed.split(';').any(|id| id == action.desktop_id())); }
    }
}
