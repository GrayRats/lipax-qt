//! Мост Rust ↔ Qt. Контроллер тонкий: состояние и логика живут в `lipa-core`,
//! тяжёлая работа выполняется в tokio-задачах, GUI-поток только получает события.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, status)]
        #[qproperty(QString, status_kind, cxx_name = "statusKind")]
        #[qproperty(QString, settings_state, cxx_name = "settingsState")]
        #[qproperty(QString, history_json, cxx_name = "historyJson")]
        #[qproperty(QString, diagnostics_json, cxx_name = "diagnosticsJson")]
        #[qproperty(bool, diagnostics_busy, cxx_name = "diagnosticsBusy")]
        #[qproperty(QString, original)]
        #[qproperty(QString, translation)]
        #[qproperty(QString, window_title, cxx_name = "windowTitle")]
        #[qproperty(QString, preview_source, cxx_name = "previewSource")]
        #[qproperty(QString, tesseract_json, cxx_name = "tesseractJson")]
        #[qproperty(bool, tesseract_busy, cxx_name = "tesseractBusy")]
        #[qproperty(QString, game_geometry, cxx_name = "gameGeometry")]
        #[qproperty(bool, running)]
        #[qproperty(bool, has_region, cxx_name = "hasRegion")]
        type Controller = super::ControllerRust;

        #[qinvokable]
        #[cxx_name = "defaultSettingsJson"]
        fn default_settings_json(self: &Controller) -> QString;
        #[qinvokable]
        #[cxx_name = "clearHistory"]
        fn clear_history(self: Pin<&mut Controller>);
        #[qinvokable]
        #[cxx_name = "copyText"]
        fn copy_text(self: &Controller, text: &QString);
        #[qinvokable]
        #[cxx_name = "refreshDiagnostics"]
        fn refresh_diagnostics(self: Pin<&mut Controller>);
        #[qinvokable]
        #[cxx_name = "configureOverlay"]
        fn configure_overlay(self: &Controller, pinned: bool, passthrough: bool, edge: i32);
        #[qinvokable]
        #[cxx_name = "settingsJson"]
        fn settings_json(self: &Controller) -> QString;
        #[qinvokable]
        #[cxx_name = "applySettings"]
        fn apply_settings(self: Pin<&mut Controller>, json: &QString);
        #[qinvokable]
        #[cxx_name = "pickWindow"]
        fn pick_window(self: Pin<&mut Controller>);
        #[qinvokable]
        #[cxx_name = "requestPreview"]
        fn request_preview(self: Pin<&mut Controller>);
        #[qinvokable]
        #[cxx_name = "setRegion"]
        fn set_region(self: Pin<&mut Controller>, x: f64, y: f64, w: f64, h: f64);
        #[qinvokable]
        #[cxx_name = "resetRegion"]
        fn reset_region(self: Pin<&mut Controller>);
        #[qinvokable]
        fn start(self: Pin<&mut Controller>);
        #[qinvokable]
        fn stop(self: Pin<&mut Controller>);
        #[qinvokable]
        #[cxx_name = "translateOnce"]
        fn translate_once(self: Pin<&mut Controller>);
        /// Заново проверить Tesseract и список языков (без перезапуска приложения).
        #[qinvokable]
        #[cxx_name = "refreshTesseract"]
        fn refresh_tesseract(self: Pin<&mut Controller>);
        /// Установить языковой пакет. Вызывается только после подтверждения пользователя в GUI.
        #[qinvokable]
        #[cxx_name = "installPackage"]
        fn install_package(self: Pin<&mut Controller>, package: &QString);
        /// Сообщения о недостающих языках из строки вида `jpn+eng` (JSON-массив).
        #[qinvokable]
        #[cxx_name = "missingLanguages"]
        fn missing_languages(self: &Controller, spec: &QString) -> QString;

        #[qsignal]
        #[cxx_name = "previewReady"]
        fn preview_ready(self: Pin<&mut Controller>);
        /// Показать рамку (координаты рабочего стола): после выбора окна или области.
        #[qsignal]
        #[cxx_name = "frameRequested"]
        fn frame_requested(self: Pin<&mut Controller>, x: f64, y: f64, w: f64, h: f64);
        /// Горячая клавиша «выбрать область»: окно выбора открывает QML.
        #[qsignal]
        #[cxx_name = "selectRegionRequested"]
        fn select_region_requested(self: Pin<&mut Controller>);
        /// Горячая клавиша «показать/скрыть overlay»: видимостью управляет QML.
        #[qsignal]
        #[cxx_name = "toggleOverlayRequested"]
        fn toggle_overlay_requested(self: Pin<&mut Controller>);
    }

    impl cxx_qt::Threading for Controller {}
    impl cxx_qt::Initialize for Controller {}
}

use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use lipa_core::capture::kwin::KwinCapture;
use lipa_core::capture::portal::is_portal_window;
use lipa_core::capture::AnyCapture;
use lipa_core::hotkeys::{self, HotkeyAction, HotkeyEvent};
use lipa_core::ocr::AnyOcr;
use lipa_core::history::{History, Entry};
use lipa_core::pipeline::{Cmd, Event, Pipeline};
use lipa_core::settings::{CaptureBackendKind, NormRect, Settings};
use lipa_core::tesseract::{TesseractInfo, TesseractManager};
use lipa_core::translate::HttpTranslate;
use std::sync::{Arc, OnceLock};
use tokio::sync::{mpsc, watch};

fn rt() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

#[derive(Default)]
struct RuntimeServices {
    tasks: Vec<tokio::task::JoinHandle<()>>,
    capture: Option<Arc<AnyCapture>>,
}

fn services() -> &'static std::sync::Mutex<RuntimeServices> {
    static SERVICES: OnceLock<std::sync::Mutex<RuntimeServices>> = OnceLock::new();
    SERVICES.get_or_init(Default::default)
}

pub fn shutdown() {
    let services = std::mem::take(&mut *services().lock().unwrap());
    rt().block_on(async move {
        for task in &services.tasks { task.abort(); }
        for task in services.tasks { let _ = task.await; }
        if let Some(capture) = services.capture { capture.portal.close().await; }
        lipa_core::capture::shutdown_geometry().await;
    });
}

struct Shared {
    settings: watch::Sender<Settings>,
    running: watch::Sender<bool>,
    cmds: mpsc::UnboundedSender<Cmd>,
    hotkeys: watch::Sender<lipa_core::settings::Hotkeys>,
    capture: Arc<AnyCapture>,
}

impl Shared {
    async fn kwin(&self) -> Result<Arc<KwinCapture>, String> {
        self.capture.kwin().await.map_err(|e| e.to_string())
    }

    /// Изменить настройки, сохранить на диск и сбросить состояние pipeline.
    fn update(&self, f: impl FnOnce(&mut Settings)) {
        let before = self.settings.borrow().processing_key();
        self.settings.send_modify(|s| { f(s); s.sanitize(); });
        if let Err(e) = self.settings.borrow().save() {
            eprintln!("не удалось сохранить настройки: {e}");
        }
        if self.settings.borrow().processing_key() != before { let _ = self.cmds.send(Cmd::Reset); }
        // Смена горячих клавиш применяется сразу, без перезапуска.
        let hk = self.settings.borrow().hotkeys.clone();
        self.hotkeys.send_if_modified(|h| {
            let changed = *h != hk;
            if changed {
                *h = hk;
            }
            changed
        });
    }
}

pub struct ControllerRust {
    status: QString,
    status_kind: QString,
    settings_state: QString,
    history_json: QString,
    diagnostics_json: QString,
    diagnostics_busy: bool,
    history: History,
    region_text: std::collections::BTreeMap<String, (String, String)>,
    faults: std::collections::BTreeMap<String, (String, bool)>,
    original: QString,
    translation: QString,
    tesseract_json: QString,
    tesseract_busy: bool,
    window_title: QString,
    preview_source: QString,
    game_geometry: QString,
    running: bool,
    has_region: bool,
    shared: Arc<Shared>,
    events: Option<(
        mpsc::UnboundedReceiver<Event>,
        mpsc::UnboundedSender<Event>,
        mpsc::UnboundedReceiver<Cmd>,
    )>,
    settings_rx: watch::Receiver<Settings>,
    running_rx: watch::Receiver<bool>,
}

impl Default for ControllerRust {
    fn default() -> Self {
        let settings = Settings::load();
        let history = if settings.history_persist { History::load(&History::path(), settings.history_limit) } else { History::default() };
        let (settings_tx, settings_rx) = watch::channel(settings.clone());
        let (running_tx, running_rx) = watch::channel(false);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (ev_tx, ev_rx) = mpsc::unbounded_channel();
        Self {
            status: QString::from("Выберите окно игры"),
            status_kind: QString::from("info"),
            settings_state: QString::from(serde_json::to_string(&settings).unwrap().as_str()),
            history_json: QString::from(serde_json::to_string(&history.entries).unwrap().as_str()),
            diagnostics_json: QString::from("[]"),
            diagnostics_busy: false,
            history,
            region_text: Default::default(),
            faults: Default::default(),
            original: QString::default(),
            translation: QString::default(),
            tesseract_json: QString::default(),
            tesseract_busy: false,
            window_title: QString::from(
                settings
                    .window
                    .as_ref()
                    .map(|w| w.caption.as_str())
                    .unwrap_or(""),
            ),
            preview_source: QString::default(),
            game_geometry: QString::default(),
            running: false,
            has_region: !settings.capture_regions().is_empty(),
            shared: Arc::new(Shared {
                hotkeys: watch::channel(settings.hotkeys.clone()).0,
                settings: settings_tx,
                running: running_tx,
                cmds: cmd_tx,
                capture: Arc::new(AnyCapture::new(settings.portal_token.clone())),
            }),
            events: Some((ev_rx, ev_tx, cmd_rx)),
            settings_rx,
            running_rx,
        }
    }
}

impl cxx_qt::Initialize for qobject::Controller {
    fn initialize(mut self: Pin<&mut Self>) {
        let (mut ev_rx, ev_tx, cmd_rx) = self
            .as_mut()
            .rust_mut()
            .events
            .take()
            .expect("initialize вызывается один раз");
        let shared = self.rust().shared.clone();
        let (settings_rx, running_rx) = (
            self.rust().settings_rx.clone(),
            self.rust().running_rx.clone(),
        );

        // Pipeline: захват → OCR → перевод в фоне. Бэкенд захвата выбирается по ключу окна.
        let capture = shared.capture.clone();
        let pipeline = rt().spawn(async move {
            Pipeline::new(capture, AnyOcr::default(), HttpTranslate::new()).run(settings_rx, running_rx, cmd_rx, ev_tx).await
        });

        {
            let mut services = services().lock().unwrap();
            services.tasks.push(pipeline);
            services.capture = Some(shared.capture.clone());
        }

        // Токен восстановления portal сохраняется, чтобы окно выбиралось без диалога при следующем запуске.
        let sh = shared.clone();
        rt().spawn(async move {
            let mut rx = sh.capture.portal.token_updates();
            while rx.changed().await.is_ok() {
                let token = rx.borrow_and_update().clone();
                sh.settings.send_modify(|s| s.portal_token = token);
                if let Err(e) = sh.settings.borrow().save() {
                    eprintln!("не удалось сохранить настройки: {e}");
                }
            }
        });

        // Глобальные горячие клавиши (KGlobalAccel). Недоступность не критична — только статус.
        let (hk_tx, mut hk_rx) = mpsc::unbounded_channel();
        let hk = shared.hotkeys.subscribe();
        let qt = self.qt_thread();
        rt().spawn(async move {
            if let Err(e) = hotkeys::listen(hk, hk_tx).await {
                let _ = qt.queue(move |mut o| {
                    o.as_mut()
                        .set_status(QString::from(format!("{e}").as_str()))
                });
            }
        });
        let qt = self.qt_thread();
        rt().spawn(async move {
            while let Some(ev) = hk_rx.recv().await {
                let _ = qt.queue(move |mut o| match ev {
                    HotkeyEvent::Conflict(a) => o.as_mut().set_status(QString::from(
                        format!("Сочетание для «{}» занято или недопустимо", a.title()).as_str(),
                    )),
                    HotkeyEvent::Pressed(action) => match action {
                        HotkeyAction::Toggle if *o.running() => o.as_mut().stop(),
                        HotkeyAction::Toggle if *o.has_region() => o.as_mut().start(),
                        HotkeyAction::Toggle => o
                            .as_mut()
                            .set_status(QString::from("Сначала выберите область")),
                        HotkeyAction::TranslateOnce => o.as_mut().translate_once(),
                        HotkeyAction::SelectRegion => o.as_mut().select_region_requested(),
                        HotkeyAction::ToggleOverlay => o.as_mut().toggle_overlay_requested(),
                    },
                });
            }
        });

        // Client geometry follows window moves and monitor changes, even while paused.
        let sh = shared.clone();
        let qt = self.qt_thread();
        let geometry_task = rt().spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_millis(500));
            // Only changes reach the GUI thread.
            let mut last: Option<String> = None;
            loop {
                ticker.tick().await;
                let window = sh.settings.borrow().window.clone();
                let geometry = match window.as_ref().filter(|w| !is_portal_window(w)) {
                    Some(w) => match sh.kwin().await {
                        Ok(k) => k.window_geometry(&w.uuid).await,
                        Err(_) => None,
                    },
                    None => None,
                };
                // Ignore a result for a window that was replaced during the await.
                if sh.settings.borrow().window != window { continue; }
                let json = geometry.map(|g| serde_json::json!([g.x, g.y, g.w, g.h]).to_string()).unwrap_or_default();
                if last.as_ref() == Some(&json) { continue; }
                last = Some(json.clone());
                if qt.queue(move |mut o| o.as_mut().set_game_geometry(QString::from(json.as_str()))).is_err() { break; }
            }
        });

        services().lock().unwrap().tasks.push(geometry_task);

        // Первичная проверка Tesseract в фоне, чтобы окно настроек открывалось сразу с данными.
        self.as_mut().refresh_tesseract();

        // Доставка событий в GUI-поток.
        let qt = self.qt_thread();
        rt().spawn(async move {
            while let Some(ev) = ev_rx.recv().await {
                let _ = qt.queue(move |mut o| match ev {
                    Event::Status(s) => {
                        if o.rust().faults.is_empty() {
                            o.as_mut().set_status(QString::from(s.as_str()));
                            o.as_mut().set_status_kind(QString::from("info"));
                        }
                    },
                    Event::Cleared { region_id } => {
                        o.as_mut().rust_mut().faults.remove(&region_id);
                        if o.rust().faults.is_empty() {
                            o.as_mut().set_status(QString::from("Ожидание текста"));
                            o.as_mut().set_status_kind(QString::from("info"));
                        }
                        o.as_mut().publish_faults();
                    },
                    Event::Error { region_id, message, terminal } => {
                        o.as_mut().rust_mut().faults.insert(region_id, (message, terminal));
                        o.as_mut().publish_faults();
                    },
                    Event::Translation { region_id, region_name, original, text } => {
                        let settings = o.rust().shared.settings.borrow().clone();
                        if !settings.capture_regions().iter().any(|r| r.id == region_id) { return; }
                        o.as_mut().rust_mut().faults.remove(&region_id);
                        o.as_mut().rust_mut().region_text.insert(region_id, (original.clone(), text.clone()));
                        if settings.history_enabled {
                            let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
                            o.as_mut().rust_mut().history.push(Entry { timestamp, region: region_name, original, translation: text }, settings.history_limit);
                            o.as_mut().publish_history();
                        }
                        o.as_mut().publish_translation();
                        o.as_mut().set_status(QString::from("Перевод обновлён"));
                        o.as_mut().set_status_kind(QString::from("info"));
                        o.as_mut().publish_faults();
                    }
                });
            }
        });
    }
}

impl qobject::Controller {
    fn default_settings_json(&self) -> QString {
        QString::from(serde_json::to_string(&Settings::default()).unwrap().as_str())
    }
    fn copy_text(&self, text: &QString) { crate::icon::copy_text(text); }
    fn configure_overlay(&self, pinned: bool, passthrough: bool, edge: i32) { crate::icon::overlay_input(pinned, passthrough, edge); }
    fn publish_settings(mut self: Pin<&mut Self>) {
        let s = self.rust().shared.settings.borrow().clone();
        self.as_mut().set_settings_state(QString::from(serde_json::to_string(&s).unwrap().as_str()));
        self.as_mut().set_has_region(!s.capture_regions().is_empty());
        let active: Vec<_> = s.capture_regions().into_iter().map(|r| r.id).collect();
        self.as_mut().rust_mut().faults.retain(|id, _| active.contains(id));
        self.as_mut().publish_faults();
    }
    fn publish_translation(mut self: Pin<&mut Self>) {
        let regions = self.rust().shared.settings.borrow().capture_regions();
        self.as_mut().rust_mut().region_text.retain(|id, _| regions.iter().any(|r| &r.id == id));
        let texts = &self.rust().region_text;
        let original = regions.iter().filter_map(|r| texts.get(&r.id).map(|t| format!("{}: {}", r.name, t.0))).collect::<Vec<_>>().join("\n");
        let translation = regions.iter().filter_map(|r| texts.get(&r.id).map(|t| if regions.len() > 1 { format!("{}: {}", r.name, t.1) } else { t.1.clone() })).collect::<Vec<_>>().join("\n\n");
        self.as_mut().set_original(QString::from(original.as_str()));
        self.as_mut().set_translation(QString::from(translation.as_str()));
    }
    fn publish_faults(mut self: Pin<&mut Self>) {
        if self.rust().faults.is_empty() { return; }
        let text = self.rust().faults.values().map(|(s, _)| s.as_str()).collect::<Vec<_>>().join(" · ");
        let kind = if self.rust().faults.values().any(|(_, terminal)| *terminal) { "error" } else { "warning" };
        self.as_mut().set_status(QString::from(text.as_str()));
        self.as_mut().set_status_kind(QString::from(kind));
    }
    fn publish_history(mut self: Pin<&mut Self>) {
        let s = self.rust().shared.settings.borrow().clone();
        self.as_mut().rust_mut().history.trim(s.history_limit);
        let result = self.rust().history.save(&History::path(), s.history_persist);
        let json = serde_json::to_string(&self.rust().history.entries).unwrap();
        self.as_mut().set_history_json(QString::from(json.as_str()));
        if let Err(e) = result { self.as_mut().set_status(QString::from(format!("История не сохранена: {e}").as_str())); }
    }
    fn clear_history(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().history.entries.clear();
        self.as_mut().publish_history();
    }
    fn refresh_diagnostics(mut self: Pin<&mut Self>) {
        if *self.diagnostics_busy() { return; }
        self.as_mut().set_diagnostics_busy(true);
        let s = self.rust().shared.settings.borrow().clone();
        let qt = self.qt_thread();
        rt().spawn(async move {
            let result = lipa_core::diagnostics::inspect(&s).await;
            let json = serde_json::to_string(&result).unwrap();
            let _ = qt.queue(move |mut o| {
                o.as_mut().set_diagnostics_json(QString::from(json.as_str()));
                o.as_mut().set_diagnostics_busy(false);
            });
        });
    }

    fn settings_json(&self) -> QString {
        let s = self.rust().shared.settings.borrow().clone();
        QString::from(serde_json::to_string(&s).unwrap_or_default().as_str())
    }

    fn apply_settings(mut self: Pin<&mut Self>, json: &QString) {
        match serde_json::from_str::<Settings>(&json.to_string()) {
            Ok(new) => {
                // Окно и область меняются отдельными действиями — не затираем их из формы.
                self.rust().shared.update(|s| {
                    let (w, r, t) = (s.window.take(), s.region.take(), std::mem::take(&mut s.portal_token));
                    *s = Settings {
                        window: w,
                        region: r,
                        portal_token: t,
                        ..new
                    };
                });
                self.as_mut().publish_settings();
                self.as_mut().publish_translation();
                self.as_mut().publish_history();
            }
            Err(e) => {
                self.as_mut().set_status(QString::from(format!("Настройки не сохранены: {e}").as_str()));
                self.as_mut().set_status_kind(QString::from("error"));
            },
        }
    }

    fn pick_window(self: Pin<&mut Self>) {
        let shared = self.rust().shared.clone();
        let qt = self.qt_thread();
        let _ = qt.queue(|mut o| {
            o.as_mut()
                .set_status(QString::from("Кликните по окну игры (Esc — отмена)"))
        });
        rt().spawn(async move {
            let backend = shared.settings.borrow().capture_backend;
            let by_kwin = async {
                match shared.kwin().await {
                    Ok(k) => k.pick_window().await.map_err(|e| e.to_string()),
                    Err(e) => Err(e),
                }
            };
            let by_portal = async { shared.capture.portal.select_window().await.map(Some).map_err(|e| e.to_string()) };
            let res = match backend {
                CaptureBackendKind::Kwin => by_kwin.await,
                CaptureBackendKind::Portal => by_portal.await,
                // KWin недоступен (другой композитор, нет прав на ScreenShot2) — запасной путь через portal.
                CaptureBackendKind::Auto => match by_kwin.await {
                    Ok(r) => Ok(r),
                    Err(_) => {
                        let _ = qt.queue(|mut o| {
                            o.as_mut().set_status(QString::from("KWin недоступен, выберите окно в системном диалоге (portal)"))
                        });
                        by_portal.await
                    }
                },
            };
            match res {
                Ok(Some(key)) => {
                    let title = key.caption.clone();
                    // Смена окна делает прежнюю область недействительной.
                    shared.update(|s| {
                        s.window = Some(key);
                        s.region = None;
                        for r in &mut s.regions { r.rect = None; }
                    });
                    show_frame(shared.clone(), qt.clone(), None);
                    let _ = qt.queue(move |mut o| {
                        o.as_mut().set_window_title(QString::from(title.as_str()));
                        o.as_mut().set_has_region(false);
                        o.as_mut().rust_mut().region_text.clear();
                        o.as_mut().rust_mut().faults.clear();
                        o.as_mut().publish_settings();
                        o.as_mut().publish_translation();
                        o.as_mut().set_status(QString::from(
                            "Окно выбрано. Теперь выберите область перевода",
                        ));
                    });
                }
                Ok(None) => {
                    let _ = qt
                        .queue(|mut o| o.as_mut().set_status(QString::from("Выбор окна отменён")));
                }
                Err(e) => {
                    let _ = qt.queue(move |mut o| {
                        o.as_mut()
                            .set_status(QString::from(format!("Ошибка: {e}").as_str()))
                    });
                }
            }
        });
    }

    /// Снимок окна для выбора области: показывается в нашем окне, поэтому
    /// не требует рисовать поверх чужого окна (чего Wayland не позволяет).
    fn request_preview(self: Pin<&mut Self>) {
        let shared = self.rust().shared.clone();
        let qt = self.qt_thread();
        rt().spawn(async move {
            let Some(window) = shared.settings.borrow().window.clone() else {
                let _ = qt.queue(|mut o| {
                    o.as_mut()
                        .set_status(QString::from("Сначала выберите окно"))
                });
                return;
            };
            let res = async {
                let img = shared.capture.grab_full(&window).await.map_err(|e| e.to_string())?;
                let dir = dirs::runtime_dir()
                    .unwrap_or_else(std::env::temp_dir)
                    .join("lipa");
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let path = dir.join(format!("preview-{}.png", std::process::id()));
                img.save(&path).map_err(|e| e.to_string())?;
                Ok::<_, String>(path)
            }
            .await;
            let _ = qt.queue(move |mut o| match res {
                Ok(path) => {
                    // Меняющийся параметр отключает кэш изображения в QML.
                    let t = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    let url = format!("file://{}?{t}", path.display());
                    o.as_mut().set_preview_source(QString::from(url.as_str()));
                    o.as_mut().preview_ready();
                }
                Err(e) => o
                    .as_mut()
                    .set_status(QString::from(format!("Ошибка: {e}").as_str())),
            });
        });
    }

    fn set_region(mut self: Pin<&mut Self>, x: f64, y: f64, w: f64, h: f64) {
        let rect = NormRect { x, y, w, h };
        self.rust().shared.update(|s| {
            s.region = None;
            if let Some(r) = s.regions.iter_mut().find(|r| r.id == s.active_region) { r.rect = Some(rect); r.enabled = true; }
        });
        self.as_mut().publish_settings();
        show_frame(self.rust().shared.clone(), self.qt_thread(), Some(rect));
        self.as_mut().set_has_region(true);
        self.as_mut().set_status(QString::from("Область сохранена"));
    }

    fn reset_region(mut self: Pin<&mut Self>) {
        self.rust().shared.update(|s| {
            s.region = None;
            if let Some(r) = s.regions.iter_mut().find(|r| r.id == s.active_region) { r.rect = None; }
        });
        self.as_mut().publish_settings();
        self.as_mut().publish_translation();
        self.as_mut().set_status(QString::from("Область сброшена"));
    }

    fn start(mut self: Pin<&mut Self>) {
        let _ = self.rust().shared.running.send(true);
        self.as_mut().set_running(true);
        self.as_mut().set_status(QString::from("Ожидание текста"));
        self.as_mut().set_status_kind(QString::from("info"));
    }

    fn stop(mut self: Pin<&mut Self>) {
        let _ = self.rust().shared.running.send(false);
        self.as_mut().set_running(false);
        self.as_mut().set_status(QString::from("Остановлено"));
    }

    fn translate_once(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().faults.clear();
        self.as_mut().set_status_kind(QString::from("info"));
        self.as_mut().set_status(QString::from("Распознавание окна…"));
        let _ = self.rust().shared.cmds.send(Cmd::TranslateOnce);
    }
}

/// Снимок Tesseract в JSON; тяжёлые вызовы (tesseract, pacman/dpkg) выполняются вне GUI-потока.
fn tesseract_snapshot() -> TesseractInfo {
    TesseractManager::system().detect()
}

impl qobject::Controller {
    fn refresh_tesseract(mut self: Pin<&mut Self>) {
        if *self.tesseract_busy() {
            return;
        }
        self.as_mut().set_tesseract_busy(true);
        let qt = self.qt_thread();
        rt().spawn(async move {
            let info = tokio::task::spawn_blocking(tesseract_snapshot).await;
            let _ = qt.queue(move |mut o| {
                if let Ok(info) = info {
                    o.as_mut().set_tesseract_json(QString::from(
                        serde_json::to_string(&info).unwrap_or_default().as_str(),
                    ));
                }
                o.as_mut().set_tesseract_busy(false);
            });
        });
    }

    fn install_package(mut self: Pin<&mut Self>, package: &QString) {
        let package = package.to_string();
        let manager = TesseractManager::system();
        // В командную строку попадают только пакеты Tesseract этого дистрибутива.
        let argv = match manager
            .is_allowed_package(&package)
            .then(|| manager.install_argv(&package))
            .flatten()
        {
            Some(a) => a,
            None => {
                self.as_mut()
                    .set_status(QString::from("Установка этого пакета не поддерживается"));
                return;
            }
        };
        if *self.tesseract_busy() {
            return;
        }
        self.as_mut().set_tesseract_busy(true);
        self.as_mut().set_status(QString::from(
            format!("Установка {package}: подтвердите пароль в системном окне").as_str(),
        ));
        let qt = self.qt_thread();
        rt().spawn(async move {
            let res = tokio::task::spawn_blocking(move || {
                std::process::Command::new(&argv[0])
                    .args(&argv[1..])
                    .output()
                    .map_err(|e| e.to_string())
                    .and_then(|o| {
                        if o.status.success() {
                            Ok(())
                        } else {
                            let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
                            // Код 126/127 от pkexec — окно закрыто или нет прав.
                            Err(if err.is_empty() {
                                format!("команда завершилась с кодом {:?}", o.status.code())
                            } else {
                                err
                            })
                        }
                    })
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
            let info = if res.is_ok() {
                tokio::task::spawn_blocking(tesseract_snapshot).await.ok()
            } else {
                None
            };
            let _ = qt.queue(move |mut o| {
                match &res {
                    Ok(()) => o.as_mut().set_status(QString::from(
                        format!("Пакет {package} установлен").as_str(),
                    )),
                    Err(e) => o.as_mut().set_status(QString::from(
                        format!("Не удалось установить {package}: {e}").as_str(),
                    )),
                }
                // Список языков обновляется автоматически после успешной установки.
                if let Some(info) = info {
                    o.as_mut().set_tesseract_json(QString::from(
                        serde_json::to_string(&info).unwrap_or_default().as_str(),
                    ));
                }
                o.as_mut().set_tesseract_busy(false);
            });
        });
    }

    fn missing_languages(&self, spec: &QString) -> QString {
        let info: Result<TesseractInfo, _> =
            serde_json::from_str(&self.tesseract_json().to_string());
        let list = match info {
            Ok(i) if i.installed => {
                TesseractManager::system().missing(&spec.to_string(), &i.languages)
            }
            _ => vec![],
        };
        QString::from(serde_json::to_string(&list).unwrap_or_default().as_str())
    }
}

/// Рамка вокруг выбранного окна (`region == None`) или области внутри него. Геометрию отдаёт KWin.
fn show_frame(shared: Arc<Shared>, qt: cxx_qt::CxxQtThread<qobject::Controller>, region: Option<NormRect>) {
    rt().spawn(async move {
        let Some(uuid) = shared.settings.borrow().window.clone().filter(|w| !is_portal_window(w)).map(|w| w.uuid) else {
            return; // у portal-окна геометрия на рабочем столе неизвестна
        };
        let Ok(kwin) = shared.kwin().await else { return };
        let Some(g) = kwin.window_geometry(&uuid).await else { return };
        let g = region.map_or(g, |r| g.region(r));
        let _ = qt.queue(move |mut o| o.as_mut().frame_requested(g.x, g.y, g.w, g.h));
    });
}
