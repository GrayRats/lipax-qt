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
        /// Translations drawn over the original text, JSON array (one entry per active region).
        #[qproperty(QString, inplace_json, cxx_name = "inplaceJson")]
        #[qproperty(QString, window_title, cxx_name = "windowTitle")]
        #[qproperty(QString, preview_source, cxx_name = "previewSource")]
        #[qproperty(QString, tesseract_json, cxx_name = "tesseractJson")]
        #[qproperty(bool, tesseract_busy, cxx_name = "tesseractBusy")]
        #[qproperty(QString, game_geometry, cxx_name = "gameGeometry")]
        #[qproperty(bool, running)]
        #[qproperty(bool, has_region, cxx_name = "hasRegion")]
        /// Видимость перевода поверх оригинала. Не связана с окном перевода.
        #[qproperty(bool, inplace_visible, cxx_name = "inplaceVisible")]
        /// Видимость окна перевода («Поверх игры»). Не связана с переводом поверх оригинала.
        #[qproperty(bool, window_overlay_visible, cxx_name = "windowOverlayVisible")]
        /// Какой рендер работает сейчас: "inplace" или "window" (с учётом отката, см. `displayNote`).
        #[qproperty(QString, effective_display, cxx_name = "effectiveDisplay")]
        /// Почему выбранный режим заменён другим (пусто, если не заменён).
        #[qproperty(QString, display_note, cxx_name = "displayNote")]
        type Controller = super::ControllerRust;

        /// Закрепить/открепить окно перевода; закреплённое встаёт туда, где было свободное.
        #[qinvokable]
        #[cxx_name = "setOverlayPinned"]
        fn set_overlay_pinned(self: Pin<&mut Controller>, pinned: bool, source: &QString);
        /// Mark the next automatic floating-window show for one-time KWin focus restoration.
        #[qinvokable]
        #[cxx_name = "armFloatingFocusRestore"]
        fn arm_floating_focus_restore(self: &Controller);
        /// Геометрия свободного окна от QML (только X11: там Qt знает положение окна).
        #[qinvokable]
        #[cxx_name = "reportFloatingGeometry"]
        fn report_floating_geometry(self: Pin<&mut Controller>, json: &QString, reason: &QString);
        #[qinvokable]
        #[cxx_name = "setInplaceVisibility"]
        fn set_inplace_visibility(self: Pin<&mut Controller>, visible: bool, source: &QString);
        #[qinvokable]
        #[cxx_name = "setWindowOverlayVisibility"]
        fn set_window_overlay_visibility(self: Pin<&mut Controller>, visible: bool, source: &QString);

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
        fn configure_overlay(self: &Controller, passthrough: bool, rects: &QString);
        #[qinvokable]
        #[cxx_name = "configureOverlayBlur"]
        fn configure_overlay_blur(self: &Controller, enable: bool, radius: i32);
        #[qinvokable]
        #[cxx_name = "settingsJson"]
        fn settings_json(self: &Controller) -> QString;
        #[qinvokable]
        #[cxx_name = "applySettings"]
        fn apply_settings(self: Pin<&mut Controller>, json: &QString);
        #[qinvokable]
        #[cxx_name = "applySettingsPatch"]
        fn apply_settings_patch(self: Pin<&mut Controller>, json: &QString);
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
        /// «Поверх оригинала»: определить шрифты полей заново (выбор шрифта иначе зафиксирован за полем).
        /// Семейства шрифтов приложения (JSON-массив): для выбора шрифта в настройках.
        #[qinvokable]
        #[cxx_name = "bundledFonts"]
        fn bundled_fonts(self: &Controller) -> QString;
        #[qinvokable]
        #[cxx_name = "reanalyzeFonts"]
        fn reanalyze_fonts(self: Pin<&mut Controller>);
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
        /// Горячая клавиша «закрепить / открепить перевод»: работает при любом режиме рамки.
        #[qsignal]
        #[cxx_name = "togglePinRequested"]
        fn toggle_pin_requested(self: Pin<&mut Controller>);
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
use lipa_core::layout::engine::InplaceFrame;
use lipa_core::layout::place::{PlacementCache, RegionInput, place_regions};
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

fn spawn_service(future: impl std::future::Future<Output = ()> + Send + 'static) {
    let task = rt().spawn(future);
    let mut services = services().lock().unwrap();
    services.tasks.retain(|task| !task.is_finished());
    services.tasks.push(task);
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
        let _ = self.update_checked(|s| { f(s); Ok(()) });
    }

    fn update_checked(&self, f: impl FnOnce(&mut Settings) -> Result<(), String>) -> Result<(), String> {
        let before = self.settings.borrow().processing_key();
        let mut error = None;
        self.settings.send_if_modified(|s| match f(s) {
            Ok(()) => { s.sanitize(); true }
            Err(e) => { error = Some(e); false }
        });
        if let Some(e) = error { return Err(e); }
        if let Err(e) = self.settings.borrow().save() {
            tracing::error!(component = "settings", error = %e, "Не удалось сохранить настройки");
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
        Ok(())
    }
}

/// Apply only the edited top-level fields to the latest backend state. A delayed appearance
/// autosave must never replace a newly selected capture window, region or Portal token.
fn merge_settings_patch(current: &Settings, patch: &serde_json::Map<String, serde_json::Value>) -> Result<Settings, String> {
    let mut state = serde_json::to_value(current).map_err(|e| e.to_string())?;
    let fields = state.as_object_mut().ok_or("settings are not a JSON object")?;
    for (key, value) in patch {
        if ["window", "region", "portal_token"].contains(&key.as_str()) {
            return Err(format!("field {key} is managed by the capture backend"));
        }
        if !fields.contains_key(key) { return Err(format!("unknown settings field: {key}")); }
        fields.insert(key.clone(), value.clone());
    }
    serde_json::from_value(state).map_err(|e| e.to_string())
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
    /// Последние поля «поверх оригинала» каждой области: размер кадра и поля.
    region_inplace: std::collections::BTreeMap<String, Arc<InplaceFrame>>,
    /// Подгонка полей кэшируется: перемещение окна игры и повторные публикации ничего не пересчитывают.
    placement_cache: PlacementCache,
    /// PNG подложек: (область, поле) → (ревизия, URL). Файл пишется только при новой ревизии.
    inplace_images: std::collections::HashMap<(String, u64), (u64, String)>,
    inplace_json: QString,
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
    inplace_visible: bool,
    window_overlay_visible: bool,
    effective_display: QString,
    display_note: QString,
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
            region_inplace: Default::default(),
            placement_cache: PlacementCache::default(),
            inplace_images: Default::default(),
            inplace_json: QString::from("[]"),
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
            inplace_visible: true,
            window_overlay_visible: true,
            effective_display: QString::from(effective_display(&settings).0),
            display_note: QString::from(effective_display(&settings).1.as_str()),
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
        spawn_service(async move {
            Pipeline::new(capture, AnyOcr::default(), HttpTranslate::new()).run(settings_rx, running_rx, cmd_rx, ev_tx).await
        });

        {
            let mut services = services().lock().unwrap();
            services.capture = Some(shared.capture.clone());
        }

        // Токен восстановления portal сохраняется, чтобы окно выбиралось без диалога при следующем запуске.
        let sh = shared.clone();
        spawn_service(async move {
            let mut rx = sh.capture.portal.token_updates();
            while rx.changed().await.is_ok() {
                let token = rx.borrow_and_update().clone();
                sh.settings.send_modify(|s| s.portal_token = token);
                if let Err(e) = sh.settings.borrow().save() {
                    tracing::error!(component = "portal", error = %e, "Не удалось сохранить настройки с токеном Portal");
                }
            }
        });

        // Глобальные горячие клавиши (KGlobalAccel). Недоступность не критична — только статус.
        let (hk_tx, mut hk_rx) = mpsc::unbounded_channel();
        let hk = shared.hotkeys.subscribe();
        let qt = self.qt_thread();
        spawn_service(async move {
            if let Err(e) = hotkeys::listen(hk, hk_tx).await {
                tracing::error!(component = "KGlobalAccel/D-Bus", error = %e, "Глобальные клавиши недоступны");
                let _ = qt.queue(move |mut o| {
                    o.as_mut()
                        .set_status(QString::from(format!("{e}").as_str()))
                });
            }
        });
        let qt = self.qt_thread();
        spawn_service(async move {
            while let Some(ev) = hk_rx.recv().await {
                let _ = qt.queue(move |mut o| match ev {
                    HotkeyEvent::Conflict(a) => {
                        tracing::warn!(component = "KGlobalAccel", action = a.title(), "Сочетание занято или недопустимо");
                        o.as_mut().set_status(QString::from(format!("Сочетание для «{}» занято или недопустимо", a.title()).as_str()));
                    },
                    HotkeyEvent::Pressed(action) => match action {
                        HotkeyAction::Toggle if *o.running() => o.as_mut().stop(),
                        HotkeyAction::Toggle if *o.has_region() => o.as_mut().start(),
                        HotkeyAction::Toggle => o
                            .as_mut()
                            .set_status(QString::from("Сначала выберите область")),
                        HotkeyAction::TranslateOnce => o.as_mut().translate_once(),
                        HotkeyAction::SelectRegion => o.as_mut().select_region_requested(),
                        // Показывает/скрывает перевод активного режима — второй режим не трогается.
                        HotkeyAction::ToggleOverlay => {
                            let source = QString::from("hotkey");
                            if o.effective_display().to_string() == "inplace" {
                                let v = !*o.inplace_visible();
                                o.as_mut().set_inplace_visibility(v, &source);
                            } else {
                                let v = !*o.window_overlay_visible();
                                o.as_mut().set_window_overlay_visibility(v, &source);
                            }
                        }
                        // Закрепление есть только у окна перевода.
                        HotkeyAction::TogglePin => {
                            if o.effective_display().to_string() == "window" { o.as_mut().toggle_pin_requested(); }
                        }
                    },
                });
            }
        });

        // Свободное окно перевода — обычное окно: на Wayland его положение знает и задаёт только
        // композитор. Скрипт KWin восстанавливает сохранённое место и сообщает новое после
        // перемещения; без KWin место выбирает композитор.
        let sh = shared.clone();
        let qt = self.qt_thread();
        spawn_service(async move {
            let kwin = match sh.kwin().await {
                Ok(k) => k,
                Err(e) => {
                    tracing::info!(target: "overlay.geometry", error = %e, "KWin недоступен: положение свободного окна выбирает композитор");
                    return;
                }
            };
            let mut settings = sh.settings.subscribe();
            let placement = floating_placement(&settings.borrow_and_update());
            if !kwin.set_floating_placement(placement).await {
                tracing::warn!(target: "overlay.geometry", "скрипт KWin недоступен: положение свободного окна не восстанавливается");
                return;
            }
            let Some(mut moves) = kwin.floating_moves().await else { return };
            loop {
                tokio::select! {
                    changed = settings.changed() => {
                        if changed.is_err() { break; }
                        let placement = floating_placement(&settings.borrow_and_update());
                        kwin.set_floating_placement(placement).await;
                    }
                    changed = moves.changed() => {
                        if changed.is_err() { break; }
                        let moved = moves.borrow_and_update().clone();
                        if let Some((g, reason)) = moved {
                            store_floating_geometry(&sh, g, &reason);
                            if qt.queue(|mut o| o.as_mut().publish_settings()).is_err() { break; }
                        }
                    }
                }
            }
        });

        // Client geometry follows window moves and monitor changes, even while paused.
        let sh = shared.clone();
        let qt = self.qt_thread();
        spawn_service(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_millis(500));
            // Only changes reach the GUI thread.
            let mut last: Option<String> = None;
            loop {
                ticker.tick().await;
                let window = sh.settings.borrow().window.clone();
                let geometry = match window.as_ref().filter(|w| !is_portal_window(w)) {
                    Some(w) => match sh.kwin().await {
                        Ok(k) => k.window_geometry(&w.uuid).await,
                        Err(e) => { tracing::debug!(component = "KWin/D-Bus", error = %e, "Геометрия окна недоступна"); None },
                    },
                    None => None,
                };
                // Ignore a result for a window that was replaced during the await.
                if sh.settings.borrow().window != window { continue; }
                let json = geometry.map(|g| serde_json::json!([g.x, g.y, g.w, g.h]).to_string()).unwrap_or_default();
                if last.as_ref() == Some(&json) { continue; }
                last = Some(json.clone());
                if qt.queue(move |mut o| {
                    o.as_mut().set_game_geometry(QString::from(json.as_str()));
                    // Кегль и поля подгоняются в пикселях экрана: окно изменило размер — подогнать заново.
                    o.as_mut().publish_inplace();
                }).is_err() { break; }
            }
        });


        // Первичная проверка Tesseract в фоне, чтобы окно настроек открывалось сразу с данными.
        self.as_mut().refresh_tesseract();

        // Доставка событий в GUI-поток.
        let qt = self.qt_thread();
        spawn_service(async move {
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
                    Event::Inplace { region_id, region_name, frame } => {
                        let settings = o.rust().shared.settings.borrow().clone();
                        if !settings.capture_regions().iter().any(|r| r.id == region_id) { return; }
                        o.as_mut().rust_mut().faults.remove(&region_id);
                        let original = frame.blocks.iter().map(|b| b.original.as_str()).collect::<Vec<_>>().join("\n");
                        let text = frame.blocks.iter().map(|b| b.translation.as_str()).collect::<Vec<_>>().join("\n");
                        o.as_mut().rust_mut().region_text.insert(region_id.clone(), (original, text));
                        if settings.history_enabled && !frame.new_translations.is_empty() {
                            let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
                            for (original, translation) in &frame.new_translations {
                                o.as_mut().rust_mut().history.push(Entry { timestamp, region: region_name.clone(), original: original.clone(), translation: translation.clone() }, settings.history_limit);
                            }
                            o.as_mut().publish_history();
                        }
                        let frame = *frame;
                        o.as_mut().rust_mut().region_inplace.insert(region_id, Arc::new(frame));
                        o.as_mut().publish_translation();
                        o.as_mut().set_status(QString::from("Перевод обновлён"));
                        o.as_mut().set_status_kind(QString::from("info"));
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
    fn arm_floating_focus_restore(&self) {
        lipa_core::capture::arm_floating_focus_restore();
        tracing::debug!(target: "overlay.focus", "armed one-time focus restoration for automatic floating show");
    }
    fn default_settings_json(&self) -> QString {
        QString::from(serde_json::to_string(&Settings::default()).unwrap().as_str())
    }
    fn copy_text(&self, text: &QString) { crate::icon::copy_text(text); }
    fn configure_overlay_blur(&self, enable: bool, radius: i32) { crate::icon::overlay_blur(enable, radius); }
    /// `rects` — JSON `[[x, y, w, h], ...]`: где overlay принимает ввод, когда клики идут сквозь него.
    fn configure_overlay(&self, passthrough: bool, rects: &QString) {
        let rects: Vec<[i32; 4]> = serde_json::from_str(&rects.to_string()).unwrap_or_default();
        crate::icon::overlay_input(passthrough, &rects.concat());
    }
    fn set_overlay_pinned(mut self: Pin<&mut Self>, pinned: bool, source: &QString) {
        // Cancel an automatic show that was superseded before KWin saw the floating surface.
        if pinned { lipa_core::capture::clear_floating_focus_restore(); }
        self.rust().shared.update(|s| {
            if pinned {
                // Закреплённое окно — на месте свободного, на том же выходе.
                if let Some(g) = s.floating_geometry.clone() {
                    s.overlay_screen = g.output.clone();
                    s.overlay_pos = g.relative();
                    tracing::debug!(target: "overlay.geometry", reason = "pin_transition", screen = %g.output,
                        x = s.overlay_pos.0, y = s.overlay_pos.1, "pinned placement from floating geometry");
                }
            }
            s.overlay_pinned = pinned;
        });
        tracing::debug!(target: "overlay.state", pinned, floating = !pinned,
            window_role = if pinned { "layer_shell" } else { "xdg_toplevel" }, source = %source, "overlay pin state");
        self.as_mut().publish_settings();
    }
    fn report_floating_geometry(mut self: Pin<&mut Self>, json: &QString, reason: &QString) {
        match serde_json::from_str::<lipa_core::settings::FloatingGeometry>(&json.to_string()) {
            Ok(g) if g.is_valid() => {
                store_floating_geometry(&self.rust().shared, g, &reason.to_string());
                self.as_mut().publish_settings();
            }
            _ => tracing::warn!(target: "overlay.geometry", json = %json, "invalid floating geometry from QML"),
        }
    }
    fn set_inplace_visibility(mut self: Pin<&mut Self>, visible: bool, source: &QString) {
        if *self.inplace_visible() != visible {
            tracing::debug!(target: "inplace.state", visible, source = %source, "inplace visibility");
            self.as_mut().set_inplace_visible(visible);
        }
    }
    fn set_window_overlay_visibility(mut self: Pin<&mut Self>, visible: bool, source: &QString) {
        if *self.window_overlay_visible() != visible {
            tracing::debug!(target: "overlay.state", visible, source = %source, "window overlay visibility");
            self.as_mut().set_window_overlay_visible(visible);
        }
    }
    /// Активный рендер по настройкам и бэкенду захвата; меняется — пишется в журнал.
    fn publish_display(mut self: Pin<&mut Self>) {
        let (mode, note) = effective_display(&self.rust().shared.settings.borrow());
        if self.effective_display().to_string() != mode || self.display_note().to_string() != note {
            if note.is_empty() {
                tracing::info!(target: "inplace.state", mode, "translation display");
            } else {
                tracing::warn!(target: "inplace.state", mode, reason = %note, "translation display fallback");
            }
            self.as_mut().set_effective_display(QString::from(mode));
            self.as_mut().set_display_note(QString::from(note.as_str()));
        }
    }
    fn publish_settings(mut self: Pin<&mut Self>) {
        self.as_mut().publish_display();
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
        self.as_mut().publish_inplace();
    }
    /// Поля «поверх оригинала» для QML: всё уже решено (шрифт, кегль после подгонки метриками
    /// Qt, типографика, фон); QML только рисует. Неизменное состояние не переустанавливается.
    fn publish_inplace(mut self: Pin<&mut Self>) {
        // Только нужное из настроек: полный клон `Settings` на каждую публикацию не нужен.
        let (regions, inplace) = {
            let settings = self.rust().shared.settings.borrow();
            (settings.capture_regions(), settings.inplace.clone())
        };
        self.as_mut().rust_mut().region_inplace.retain(|id, _| regions.iter().any(|r| &r.id == id));
        let window = serde_json::from_str::<[f64; 4]>(&self.game_geometry().to_string()).ok();
        // Арк-указатели: клонируются только счётчики, не блоки и не картинки.
        let frames: Vec<(String, lipa_core::settings::NormRect, Arc<InplaceFrame>)> = match window {
            Some(_) => regions.iter().filter_map(|r| Some((r.id.clone(), r.rect?, self.rust().region_inplace.get(&r.id)?.clone()))).collect(),
            None => Vec::new(),
        };
        // Подложки пишутся на диск только при новой ревизии поля.
        let mut images = std::collections::HashMap::new();
        for (region, _, frame) in &frames {
            for b in &frame.blocks {
                let Some(img) = b.background.image.as_ref() else { continue };
                let key = (region.clone(), b.id);
                let url = match self.rust().inplace_images.get(&key) {
                    Some((rev, url)) if *rev == b.revision => url.clone(),
                    _ => match save_backdrop(region, b.id, b.revision, img) {
                        Some(url) => {
                            self.as_mut().rust_mut().inplace_images.insert(key.clone(), (b.revision, url.clone()));
                            url
                        }
                        None => {
                            tracing::warn!(target: "inplace.render", region = %region, block_id = b.id, "backdrop image could not be written");
                            continue;
                        }
                    },
                };
                images.insert(key, url);
            }
        }
        // Подложки исчезнувших полей больше не нужны.
        let gone: Vec<_> = self.rust().inplace_images.keys().filter(|k| !images.contains_key(*k)).cloned().collect();
        for key in gone {
            self.as_mut().rust_mut().inplace_images.remove(&key);
            let _ = std::fs::remove_file(backdrop_path(&key.0, key.1));
        }
        let placed = match window {
            Some(window) => {
                let inputs: Vec<RegionInput> = frames.iter().map(|(id, rect, frame)| RegionInput { id, rect: *rect, frame }).collect();
                let mut cache = std::mem::take(&mut self.as_mut().rust_mut().placement_cache);
                let placed = place_regions(&inputs, window, &inplace, &images, &crate::icon::QtMeasure, &mut cache);
                self.as_mut().rust_mut().placement_cache = cache;
                placed
            }
            None => Vec::new(),
        };
        let json = serde_json::to_string(&placed).unwrap_or_else(|e| {
            tracing::error!(target: "inplace.render", error = %e, "cannot serialize the placed fields");
            "[]".into()
        });
        if *self.inplace_json() != QString::from(json.as_str()) {
            self.as_mut().set_inplace_json(QString::from(json.as_str()));
        }
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
        if let Err(e) = result {
            tracing::error!(component = "history", error = %e, "История не сохранена");
            self.as_mut().set_status(QString::from(format!("История не сохранена: {e}").as_str()));
        }
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
        spawn_service(async move {
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
                let before = self.rust().shared.settings.borrow().processing_key();
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
                if self.rust().shared.settings.borrow().processing_key() != before {
                    // Поля, найденные в прежней области, не рисуются поверх новой.
                    self.as_mut().rust_mut().region_inplace.clear();
                }
                self.as_mut().publish_settings();
                self.as_mut().publish_translation();
                self.as_mut().publish_history();
            }
            Err(e) => {
                tracing::error!(component = "settings", error = %e, "Настройки из QML не приняты");
                self.as_mut().set_status(QString::from(format!("Настройки не сохранены: {e}").as_str()));
                self.as_mut().set_status_kind(QString::from("error"));
            },
        }
    }

    fn apply_settings_patch(mut self: Pin<&mut Self>, json: &QString) {
        let patch = match serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&json.to_string()) {
            Ok(patch) => patch,
            Err(e) => {
                tracing::error!(component = "settings", error = %e, "Некорректное изменение настроек из QML");
                self.as_mut().set_status(QString::from(format!("Настройки не сохранены: {e}").as_str()));
                self.as_mut().set_status_kind(QString::from("error"));
                return;
            }
        };
        if patch.is_empty() { return; }
        let before = self.rust().shared.settings.borrow().processing_key();
        match self.rust().shared.update_checked(|s| {
            *s = merge_settings_patch(s, &patch)?;
            Ok(())
        }) {
            Ok(()) => {
                if self.rust().shared.settings.borrow().processing_key() != before {
                    self.as_mut().rust_mut().region_inplace.clear();
                }
                self.as_mut().publish_settings();
                self.as_mut().publish_translation();
                self.as_mut().publish_history();
            }
            Err(e) => {
                tracing::error!(component = "settings", error = %e, "Изменение настроек не принято");
                self.as_mut().set_status(QString::from(format!("Настройки не сохранены: {e}").as_str()));
                self.as_mut().set_status_kind(QString::from("error"));
            }
        }
    }

    fn pick_window(self: Pin<&mut Self>) {
        let shared = self.rust().shared.clone();
        let qt = self.qt_thread();
        let _ = qt.queue(|mut o| {
            o.as_mut()
                .set_status(QString::from("Кликните по окну игры (Esc — отмена)"))
        });
        spawn_service(async move {
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
                    Err(e) => {
                        tracing::warn!(component = "KWin", error = %e, "Переключение на Portal-захват");
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
                    show_frame(shared.clone(), qt.clone());
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
                    tracing::error!(component = "capture", error = %e, "Не удалось выбрать окно");
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
        spawn_service(async move {
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
            if let Err(e) = &res {
                tracing::error!(component = "capture", error = %e, "Не удалось получить кадр для выбора области");
            }
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
                Err(e) => {
                    o.as_mut().set_status(QString::from(format!("Ошибка: {e}").as_str()));
                },
            });
        });
    }

    fn set_region(mut self: Pin<&mut Self>, x: f64, y: f64, w: f64, h: f64) {
        let rect = NormRect { x, y, w, h };
        self.rust().shared.update(|s| {
            s.region = None;
            if let Some(r) = s.regions.iter_mut().find(|r| r.id == s.active_region) { r.rect = Some(rect); r.enabled = true; }
        });
        let active = self.rust().shared.settings.borrow().active_region.clone();
        self.as_mut().rust_mut().region_inplace.remove(&active);
        self.as_mut().publish_translation();
        // The region outline (RegionFrame) flashes by itself when the area changes.
        self.as_mut().publish_settings();
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
        tracing::info!("Слежение за областями запущено");
        let _ = self.rust().shared.running.send(true);
        self.as_mut().set_running(true);
        self.as_mut().set_status(QString::from("Ожидание текста"));
        self.as_mut().set_status_kind(QString::from("info"));
    }

    fn stop(mut self: Pin<&mut Self>) {
        tracing::info!("Слежение за областями остановлено");
        let _ = self.rust().shared.running.send(false);
        self.as_mut().set_running(false);
        self.as_mut().set_status(QString::from("Остановлено"));
    }

    fn translate_once(mut self: Pin<&mut Self>) {
        tracing::info!("Запрошен ручной перевод / повтор");
        self.as_mut().rust_mut().faults.clear();
        self.as_mut().set_status_kind(QString::from("info"));
        self.as_mut().set_status(QString::from("Распознавание окна…"));
        let _ = self.rust().shared.cmds.send(Cmd::TranslateOnce);
    }

    fn bundled_fonts(&self) -> QString {
        let db = lipa_core::layout::font_database::InstalledFontDatabase::bundled();
        let names: Vec<&str> = db.fonts().iter().map(|f| f.family.as_str()).collect();
        QString::from(serde_json::to_string(&names).unwrap_or_else(|_| "[]".into()).as_str())
    }

    fn reanalyze_fonts(mut self: Pin<&mut Self>) {
        tracing::info!(component = "fonts", "Запрошено повторное определение шрифтов");
        lipa_core::layout::font_database::InstalledFontDatabase::reload();
        let _ = self.rust().shared.cmds.send(Cmd::ReanalyzeFonts);
        self.as_mut().set_status(QString::from("Шрифты полей будут определены заново"));
    }
}

fn backdrop_path(region_id: &str, block: u64) -> std::path::PathBuf {
    let name: String = region_id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    dirs::runtime_dir().unwrap_or_else(std::env::temp_dir).join("lipa").join(format!("inplace-{}-{name}-{block}.png", std::process::id()))
}

/// Размытая подложка поля рядом с превью окна; ревизия в URL заставляет QML перечитать файл.
fn save_backdrop(region_id: &str, block: u64, revision: u64, img: &image::RgbaImage) -> Option<String> {
    let path = backdrop_path(region_id, block);
    std::fs::create_dir_all(path.parent()?).inspect_err(|e| tracing::error!(component = "inplace", error = %e, "Не удалось создать каталог подложек")).ok()?;
    img.save(&path).inspect_err(|e| tracing::error!(component = "inplace", error = %e, "Не удалось сохранить подложку")).ok()?;
    Some(format!("file://{}?r={revision}", path.display()))
}

/// Положение свободного окна сохраняется после перемещения — не во время него.
fn store_floating_geometry(shared: &Shared, g: lipa_core::settings::FloatingGeometry, reason: &str) {
    tracing::debug!(target: "overlay.geometry", reason, x = g.x, y = g.y, width = g.w, height = g.h, screen = %g.output, "floating geometry");
    shared.update(|s| s.floating_geometry = Some(g));
}

/// JSON для скрипта KWin: где поставить свободное окно при появлении. Сначала — где его
/// оставили, иначе — там, где стоит закреплённое; `null` — пусть место выберет KWin.
fn floating_placement(s: &Settings) -> String {
    match &s.floating_geometry {
        Some(g) => {
            let (rx, ry) = g.relative();
            serde_json::json!({ "x": g.x, "y": g.y, "output": g.output, "rx": rx, "ry": ry }).to_string()
        }
        None if !s.overlay_screen.is_empty() => serde_json::json!({ "output": s.overlay_screen, "rx": s.overlay_pos.0, "ry": s.overlay_pos.1 }).to_string(),
        None => "null".into(),
    }
}

/// Какой рендер перевода работает: выбранный в настройках, кроме случая, когда для
/// «поверх оригинала» нет положения окна на экране (захват через portal).
fn effective_display(s: &Settings) -> (&'static str, String) {
    use lipa_core::settings::TranslationDisplay;
    match s.translation_display {
        TranslationDisplay::Window => ("window", String::new()),
        TranslationDisplay::Inplace if s.window.as_ref().is_some_and(is_portal_window) =>
            ("window", "окно выбрано через portal: его положение на экране неизвестно, перевод показывается в окне перевода".into()),
        TranslationDisplay::Inplace if {
            let db = lipa_core::layout::font_database::InstalledFontDatabase::bundled();
            let regions = s.capture_regions();
            let langs: Vec<&str> = if regions.is_empty() { vec![s.target_lang.as_str()] }
                else { regions.iter().map(|r| if r.target_lang.is_empty() { s.target_lang.as_str() } else { r.target_lang.as_str() }).collect() };
            langs.into_iter().any(|lang| !db.fonts().iter().any(|font| font.covers(lipa_core::layout::Script::from_lang(lang).fontconfig_lang())))
        } =>
            ("window", "нет встроенного шрифта для языка перевода: используется окно перевода".into()),
        TranslationDisplay::Inplace => ("inplace", String::new()),
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
        spawn_service(async move {
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
        spawn_service(async move {
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
            if let Err(e) = &res {
                tracing::error!(component = "packages", %package, error = %e, "Установка языкового пакета не удалась");
            }
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
                    Err(e) => {
                        o.as_mut().set_status(QString::from(format!("Не удалось установить {package}: {e}").as_str()));
                    },
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

/// Рамка вокруг только что выбранного окна. Геометрию отдаёт KWin; рамки областей рисует QML (RegionFrame).
fn show_frame(shared: Arc<Shared>, qt: cxx_qt::CxxQtThread<qobject::Controller>) {
    spawn_service(async move {
        let Some(uuid) = shared.settings.borrow().window.clone().filter(|w| !is_portal_window(w)).map(|w| w.uuid) else {
            return; // у portal-окна геометрия на рабочем столе неизвестна
        };
        let Ok(kwin) = shared.kwin().await else { return };
        let Some(g) = kwin.window_geometry(&uuid).await else { return };
        let _ = qt.queue(move |mut o| o.as_mut().frame_requested(g.x, g.y, g.w, g.h));
    });
}

#[cfg(test)]
mod display_tests {
    use super::*;
    use lipa_core::settings::{FloatingGeometry, TranslationDisplay, WindowKey};

    #[test]
    fn appearance_patch_preserves_live_capture_settings() {
        let mut current = Settings::default();
        current.window = Some(WindowKey { uuid: "game-now".into(), resource_class: "Game".into(), caption: "Game".into() });
        current.capture_backend = CaptureBackendKind::Portal;
        let before = current.processing_key();
        let patch = serde_json::json!({ "font_size": 30, "overlay_pinned_corner_radius": 18 });
        let updated = merge_settings_patch(&current, patch.as_object().unwrap()).unwrap();
        assert_eq!(updated.window, current.window);
        assert_eq!(updated.capture_backend, CaptureBackendKind::Portal);
        assert_eq!(updated.processing_key(), before, "appearance must not reset the capture pipeline");
        assert_eq!((updated.font_size, updated.overlay_pinned_corner_radius), (30, 18));
        let forbidden = serde_json::json!({ "window": null });
        assert!(merge_settings_patch(&current, forbidden.as_object().unwrap()).is_err());
    }

    #[test]
    fn inplace_does_not_depend_on_the_window_overlay() {
        let mut s = Settings { translation_display: TranslationDisplay::Inplace, ..Settings::default() };
        assert_eq!(effective_display(&s), ("inplace", String::new()));
        // Portal capture has no window position: logged fallback to the translation window.
        s.window = Some(WindowKey { uuid: "portal:window".into(), resource_class: String::new(), caption: String::new() });
        let (mode, note) = effective_display(&s);
        assert_eq!(mode, "window");
        assert!(note.contains("portal"));
        s.translation_display = TranslationDisplay::Window;
        assert_eq!(effective_display(&s), ("window", String::new()));
    }

    #[test]
    fn floating_placement_restores_where_the_user_left_it() {
        let mut s = Settings::default();
        assert_eq!(floating_placement(&s), "null", "first start: KWin chooses");
        s.overlay_screen = "DP-1".into();
        s.overlay_pos = (40, 50);
        let p: serde_json::Value = serde_json::from_str(&floating_placement(&s)).unwrap();
        assert_eq!((p["output"].as_str(), p["rx"].as_i64(), p["ry"].as_i64()), (Some("DP-1"), Some(40), Some(50)), "where the pinned one was");
        s.floating_geometry = Some(FloatingGeometry { x: 2140.0, y: 382.0, w: 740.0, h: 240.0, output: "DP-2".into(), output_x: 2560.0, output_y: -200.0 });
        let p: serde_json::Value = serde_json::from_str(&floating_placement(&s)).unwrap();
        assert_eq!((p["x"].as_f64(), p["y"].as_f64(), p["output"].as_str()), (Some(2140.0), Some(382.0), Some("DP-2")));
        assert_eq!((p["rx"].as_i64(), p["ry"].as_i64()), (Some(-420), Some(582)), "negative layout offsets are kept");
    }
}
