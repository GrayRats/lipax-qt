//! Qt model-management actions. The controller publishes state; QThread owns I/O.
use super::{Settings, qobject};
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use std::pin::Pin;

/// Compare paths structurally: Unix filenames can contain newlines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ModelPaths {
    manual: String,
    directory: String,
    legacy: String,
}

fn is_selected(settings: &Settings, pair: &str) -> bool {
    settings.translation.service == lipa_core::settings::TranslationService::Bergamot
        && lipa_core::translate::models::pair(
            settings.translation_source_language(),
            &settings.translation.target_language,
        )
        .as_deref()
            == Ok(pair)
}

impl qobject::Controller {
    pub(super) fn set_model_path(mut self: Pin<&mut Self>, path: &QString) {
        let settings = self.rust().shared.settings.borrow().clone();
        if let Ok(pair) = lipa_core::translate::models::pair(
            settings.translation_source_language(),
            &settings.translation.target_language,
        ) {
            let path = path.to_string();
            let saved = self.rust().shared.update_checked(|s| {
                if path.trim().is_empty() {
                    s.translation.bergamot_model_paths.remove(&pair);
                } else {
                    s.translation
                        .bergamot_model_paths
                        .insert(pair.clone(), path.trim().into());
                }
                Ok(())
            });
            if let Err(error) = saved {
                self.as_mut().set_status(QString::from(error.as_str()));
                self.as_mut().set_status_kind(QString::from("error"));
            }
            self.as_mut().rust_mut().model_checked.remove(&pair);
            self.as_mut().publish_settings();
        }
    }

    pub(super) fn refresh_models(mut self: Pin<&mut Self>) {
        if *self.model_busy() {
            return;
        }
        self.as_mut().rust_mut().model_checked.clear();
        self.as_mut().check_models();
    }

    pub(super) fn check_models(mut self: Pin<&mut Self>) {
        use lipa_core::{settings::TranslationService, translate::models};
        if *self.model_busy() {
            return;
        }
        let settings = self.rust().shared.settings.borrow().clone();
        if settings.translation.service != TranslationService::Bergamot {
            self.as_mut().set_model_state(QString::from("{}"));
            return;
        }
        let mut effective = vec![settings.clone()];
        for region in settings.capture_regions() {
            let mut s = settings.clone();
            if !region.recognition_language.is_empty() {
                s.recognition.language = region.recognition_language;
            }
            if !region.target_language.is_empty() {
                s.translation.target_language = region.target_language;
            }
            effective.push(s);
        }
        for (index, s) in effective.into_iter().enumerate() {
            let pair = match models::pair(
                s.translation_source_language(),
                &s.translation.target_language,
            ) {
                Ok(pair) => pair,
                Err(error) => {
                    self.as_mut().set_model_state(QString::from(
                        serde_json::json!({"status":"Not Found", "error":error})
                            .to_string()
                            .as_str(),
                    ));
                    continue;
                }
            };
            if s.translation_source_language()
                .eq_ignore_ascii_case(&s.translation.target_language)
            {
                if index == 0 {
                    self.as_mut().set_model_state(QString::from(
                        serde_json::json!({"pair":pair,"status":"Not required"})
                            .to_string()
                            .as_str(),
                    ));
                }
                continue;
            }
            let manual = s
                .translation
                .bergamot_model_paths
                .get(&pair)
                .cloned()
                .unwrap_or_default();
            let directory = s.translation.bergamot_models_dir.clone();
            let legacy = if directory.is_empty() {
                std::env::var("BERGAMOT_MODELS_DIR").unwrap_or_default()
            } else {
                directory.clone()
            };
            let signature = ModelPaths {
                manual: manual.clone(),
                directory: directory.clone(),
                legacy: legacy.clone(),
            };
            if self.rust().model_checked.get(&pair) == Some(&signature) {
                if index == 0 {
                    let previous = self.rust().model_results.get(&pair).cloned();
                    if let Some(previous) = previous {
                        self.as_mut()
                            .set_model_state(QString::from(previous.as_str()));
                    }
                }
                continue;
            }
            self.as_mut()
                .rust_mut()
                .model_checked
                .insert(pair.clone(), signature);
            self.as_mut().set_model_busy(true);
            self.as_mut().set_model_state(QString::from(
                serde_json::json!({"pair":pair,"status":"Searching...","progress":-1})
                    .to_string()
                    .as_str(),
            ));
            let qt = self.qt_thread();
            let progress_qt = self.qt_thread();
            let progress_pair = pair.clone();
            let paths: Vec<_> = [&manual, &legacy]
                .into_iter()
                .filter(|p| !p.is_empty())
                .map(std::path::PathBuf::from)
                .collect();
            crate::model_worker::spawn(move |mut cancel| {
                let result = crate::model_worker::prepare(&pair, &paths, &mut cancel, |percent| {
                    let state = serde_json::json!({"pair":progress_pair,"status":if percent < 0 {"Searching..."} else {"Downloading..."},"progress":percent}).to_string();
                    let _ = progress_qt.queue(move |mut o| {
                        o.as_mut().set_model_state(QString::from(state.as_str()))
                    });
                });
                let _ = qt.queue(move |mut o| {
                    o.as_mut().set_model_busy(false);
                    // A completed download must not overwrite a manual path edited while it ran.
                    let current = o.rust().shared.settings.borrow().clone();
                    let unchanged = current.translation.bergamot_model_paths.get(&pair).cloned().unwrap_or_default() == manual
                        && current.translation.bergamot_models_dir == directory;
                    if unchanged {
                        let state = match result {
                            Ok(path) => {
                                let path = path.to_string_lossy().into_owned();
                                let saved = o.rust().shared.update_checked(|s| {
                                    s.translation.bergamot_model_paths.insert(pair.clone(), path.clone());
                                    Ok(())
                                });
                                // The in-memory setting is updated even if persistence fails.
                                // Record it to avoid an endless verify/save/retry loop on a read-only config.
                                o.as_mut().rust_mut().model_checked.insert(pair.clone(), ModelPaths { manual: path.clone(), directory: directory.clone(), legacy: legacy.clone() });
                                match saved {
                                    Ok(()) => {
                                        if is_selected(&current, &pair) {
                                            o.as_mut().set_status(QString::from(format!("Translation model for {pair}: Found").as_str()));
                                            o.as_mut().set_status_kind(QString::from("info"));
                                        }
                                        serde_json::json!({"pair":pair,"status":"Found","progress":100,"path":path})
                                    }
                                    Err(e) => serde_json::json!({"pair":pair,"status":"Found","path":path,"error":e}),
                                }
                            }
                            Err(error) => {
                                if is_selected(&current, &pair) {
                                    o.as_mut().set_status(QString::from(error.as_str()));
                                    o.as_mut().set_status_kind(QString::from("error"));
                                }
                                serde_json::json!({"pair":pair,"status":"Not Found","error":error})
                            }
                        };
                        o.as_mut().rust_mut().model_results.insert(pair.clone(), state.to_string());
                        o.as_mut().set_model_state(QString::from(state.to_string().as_str()));
                    }
                    o.as_mut().publish_settings();
                });
            });
            break; // Serialize installations; selection changes are picked up at completion.
        }
    }
}

/// The catalog model with this id; ids come from `rapidJson`, never paths.
fn ocr_model(id: &str) -> Option<&'static lipa_core::ocr::rapid_models::OcrModel> {
    lipa_core::ocr::rapid_models::catalog().iter().find(|m| m.id == id)
}

impl qobject::Controller {
    /// RapidOCR models: the status is read on the blocking pool (it hashes the selected model), downloads and deletions
    /// run on a model QThread and happen only on the user's request.
    pub(super) fn refresh_rapid(mut self: Pin<&mut Self>) {
        let settings = self.rust().shared.settings.borrow().clone();
        let generation = self.rust().rapid_generation + 1;
        self.as_mut().rust_mut().rapid_generation = generation;
        let qt = self.qt_thread();
        super::spawn_service(async move {
            use lipa_core::ocr::{rapid, rapid_models};
            let r = settings.recognition;
            let status = tokio::task::spawn_blocking(move || {
                rapid_models::status(&r.language, &r.rapid_variant, r.rapid_threads, &rapid_models::cache_root(), rapid::library_path(None))
            }).await;
            let Ok(status) = status else { return };
            tracing::debug!(component = "rapidocr", ready = status.ready, summary = %status.summary, "RapidOCR status");
            let json = serde_json::to_string(&status).unwrap_or_default();
            let _ = qt.queue(move |mut o| {
                if o.rust().rapid_generation == generation {
                    o.as_mut().set_rapid_json(QString::from(json.as_str()));
                }
            });
        });
    }

    pub(super) fn download_rapid_model(mut self: Pin<&mut Self>, id: &QString) {
        if *self.rapid_busy() {
            return;
        }
        let Some(model) = ocr_model(&id.to_string()) else {
            self.as_mut().set_rapid_error(QString::from("Неизвестная модель RapidOCR."));
            return;
        };
        self.as_mut().set_rapid_busy(true);
        self.as_mut().set_rapid_model(QString::from(model.id.as_str()));
        self.as_mut().set_rapid_progress(0);
        self.as_mut().set_rapid_error(QString::default());
        tracing::info!(component = "rapidocr", model = %model.id, bytes = model.size(), "Загрузка модели RapidOCR по запросу пользователя");
        let (qt, progress_qt) = (self.qt_thread(), self.qt_thread());
        crate::model_worker::spawn(move |mut cancel| {
            let mut last = -1;
            let result = crate::model_worker::download_ocr_model(model, &mut cancel, |percent| {
                if percent != last {
                    last = percent;
                    let _ = progress_qt.queue(move |o| o.set_rapid_progress(percent));
                }
            });
            let _ = qt.queue(move |mut o| {
                o.as_mut().set_rapid_busy(false);
                match result {
                    Ok(path) => {
                        tracing::info!(component = "rapidocr", model = %model.id, path = %path.display(), "Модель RapidOCR установлена");
                        o.as_mut().set_rapid_progress(100);
                        o.as_mut().set_status(QString::from(format!("Модель RapidOCR «{}» скачана и проверена.", model.label).as_str()));
                        o.as_mut().set_status_kind(QString::from("info"));
                    }
                    Err(error) => {
                        tracing::error!(component = "rapidocr", model = %model.id, %error, "Модель RapidOCR не скачана");
                        o.as_mut().set_rapid_error(QString::from(error.as_str()));
                    }
                }
                o.as_mut().refresh_rapid();
            });
        });
    }

    pub(super) fn delete_rapid_model(mut self: Pin<&mut Self>, id: &QString) {
        if *self.rapid_busy() {
            return;
        }
        let Some(model) = ocr_model(&id.to_string()) else { return };
        self.as_mut().set_rapid_busy(true);
        self.as_mut().set_rapid_model(QString::from(model.id.as_str()));
        self.as_mut().set_rapid_error(QString::default());
        let qt = self.qt_thread();
        crate::model_worker::spawn(move |_| {
            let result = crate::model_worker::delete_ocr_model(model);
            let _ = qt.queue(move |mut o| {
                o.as_mut().set_rapid_busy(false);
                if let Err(error) = result {
                    o.as_mut().set_rapid_error(QString::from(error.as_str()));
                } else {
                    tracing::info!(component = "rapidocr", model = %model.id, "Модель RapidOCR удалена");
                }
                o.as_mut().refresh_rapid();
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rapid_actions_accept_catalog_ids_only() {
        assert!(ocr_model("eslav-mobile").is_some());
        assert!(ocr_model("../eslav-mobile").is_none());
        assert!(ocr_model("").is_none());
    }
    #[test]
    fn old_download_cannot_report_errors_for_another_service_or_pair() {
        let mut settings = Settings::default();
        settings.translation.service = lipa_core::settings::TranslationService::Bergamot;
        assert!(is_selected(&settings, "en-ru"));
        settings.translation.target_language = "de".into();
        assert!(!is_selected(&settings, "en-ru"));
        settings.translation.target_language = "ru".into();
        settings.translation.service = lipa_core::settings::TranslationService::Google;
        assert!(!is_selected(&settings, "en-ru"));
    }
}
