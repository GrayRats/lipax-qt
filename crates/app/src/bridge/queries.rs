//! The invokable methods of the controller that only read state.

use super::*;

impl qobject::Controller {
    pub(super) fn theme_policy(&self, requested: &QString) -> QString {
        QString::from(crate::ui_settings::theme_policy(&requested.to_string()).to_string().as_str())
    }
    pub(super) fn qt_version(&self) -> QString {
        QString::from(format!("Qt {} (сборка {}), стиль {}", crate::icon::qt_version(), env!("LIPAX_BUILD_QT_VERSION"), crate::icon::quick_style()).as_str())
    }
    pub(super) fn app_version(&self) -> QString { QString::from(env!("CARGO_PKG_VERSION")) }

    pub(super) fn refresh_paddle(mut self: Pin<&mut Self>) {
        if *self.paddle_busy() { return; }
        self.as_mut().set_paddle_busy(true);
        let (python, language) = { let s = self.rust().shared.settings.borrow(); (s.recognition.paddle_python.clone(), s.recognition.language.clone()) };
        let python = if python.trim().is_empty() { "python3".to_owned() } else { python.trim().to_owned() };
        let qt = self.qt_thread();
        spawn_service(async move {
            let env = lipa_core::ocr::paddle_env::inspect(&python, &language).await;
            tracing::debug!(component = "paddleocr", ready = env.ready, summary = %env.summary, "PaddleOCR environment");
            let json = serde_json::to_string(&env).unwrap_or_default();
            let _ = qt.queue(move |mut o| {
                o.as_mut().set_paddle_json(QString::from(json.as_str()));
                o.as_mut().set_paddle_busy(false);
            });
        });
    }

    pub(super) fn refresh_tesseract(mut self: Pin<&mut Self>) {
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

    pub(super) fn refresh_gamepads(self: Pin<&mut Self>) {
        let qt = self.qt_thread();
        spawn_service(async move {
            let probe = tokio::task::spawn_blocking(lipa_core::gamepad::probe).await;
            let _ = qt.queue(move |mut o| {
                if let Ok(probe) = probe {
                    o.as_mut().set_gamepad_json(QString::from(serde_json::to_string(&probe).unwrap_or_default().as_str()));
                }
            });
        });
    }

    pub(super) fn install_package(mut self: Pin<&mut Self>, package: &QString) {
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

    pub(super) fn install_language(mut self: Pin<&mut Self>, code: &QString) {
        let code = code.to_string();
        if *self.tesseract_busy() { return; }
        self.as_mut().set_tesseract_busy(true);
        self.as_mut().set_status(QString::from(format!("Загрузка языковой модели {code}…").as_str()));
        let qt = self.qt_thread();
        spawn_service(async move {
            let res = lipa_core::tesseract::download_language(&code).await;
            match &res {
                Ok(path) => tracing::info!(component = "tesseract", %code, path = %path.display(), "Языковая модель скачана"),
                Err(e) => tracing::error!(component = "tesseract", %code, error = %e, "Не удалось скачать языковую модель"),
            }
            // The list of languages is read again either way: it is what the user sees.
            let info = tokio::task::spawn_blocking(tesseract_snapshot).await.ok();
            let _ = qt.queue(move |mut o| {
                let status = match &res {
                    Ok(_) => format!("Языковая модель {code} скачана"),
                    Err(e) => format!("Не удалось скачать языковую модель {code}: {e}"),
                };
                o.as_mut().set_status(QString::from(status.as_str()));
                o.as_mut().set_status_kind(QString::from(if res.is_ok() { "info" } else { "error" }));
                if let Some(info) = info {
                    o.as_mut().set_tesseract_json(QString::from(serde_json::to_string(&info).unwrap_or_default().as_str()));
                }
                o.as_mut().set_tesseract_busy(false);
            });
        });
    }

    pub(super) fn missing_languages(&self, spec: &QString) -> QString {
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
