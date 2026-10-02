//! Движки перевода: Google (gtx, без ключа), Yandex Cloud Translate v2, свой API.

use crate::settings::{Settings, TranslatorKind};
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum TranslateError {
    #[error("сеть: {0}")]
    Http(#[from] reqwest::Error),
    #[error("сервис вернул {status}: {body}")]
    Status { status: u16, body: String },
    #[error("неожиданный ответ сервиса")]
    BadResponse,
    #[error("не настроено: {0}")]
    NotConfigured(&'static str),
}

/// Код Tesseract (eng, rus, jpn...) -> ISO 639-1 для сервисов перевода.
pub fn tess_to_iso(code: &str) -> &str {
    match code {
        "eng" => "en",
        "rus" => "ru",
        "jpn" | "jpn_vert" => "ja",
        "deu" => "de",
        "fra" => "fr",
        "spa" => "es",
        "ita" => "it",
        "por" => "pt",
        "kor" => "ko",
        "chi_sim" => "zh",
        "chi_tra" => "zh-TW",
        "ukr" => "uk",
        "pol" => "pl",
        other => other,
    }
}

/// Абстракция для pipeline (и подмены в тестах).
pub trait Translate: Send + Sync {
    fn translate(
        &self,
        settings: &Settings,
        text: &str,
        src: &str,
        dst: &str,
    ) -> impl std::future::Future<Output = Result<String, TranslateError>> + Send;
}

/// Рабочая реализация: движок выбирается из настроек на каждый вызов.
pub struct HttpTranslate {
    http: reqwest::Client,
}

impl HttpTranslate {
    pub fn new() -> Self {
        Self { http: Translator::client() }
    }
}

impl Default for HttpTranslate {
    fn default() -> Self {
        Self::new()
    }
}

impl Translate for HttpTranslate {
    async fn translate(&self, s: &Settings, text: &str, src: &str, dst: &str) -> Result<String, TranslateError> {
        Translator::from_settings(s).translate(&self.http, text, src, dst).await
    }
}

#[derive(Debug, Clone)]
pub enum Translator {
    Google,
    Yandex { api_key: String, folder_id: String },
    Custom { url: String, api_key: String },
}

impl Translator {
    pub fn from_settings(s: &Settings) -> Self {
        match s.translator {
            TranslatorKind::Google => Self::Google,
            TranslatorKind::Yandex => Self::Yandex {
                api_key: s.yandex_api_key.clone(),
                folder_id: s.yandex_folder_id.clone(),
            },
            TranslatorKind::Custom => Self::Custom {
                url: s.custom_url.clone(),
                api_key: s.custom_api_key.clone(),
            },
        }
    }

    pub fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest client")
    }

    /// `src` — ISO-код или "auto".
    pub async fn translate(
        &self,
        http: &reqwest::Client,
        text: &str,
        src: &str,
        dst: &str,
    ) -> Result<String, TranslateError> {
        match self {
            Self::Google => {
                let resp = http
                    .get("https://translate.googleapis.com/translate_a/single")
                    .query(&[("client", "gtx"), ("sl", src), ("tl", dst), ("dt", "t"), ("q", text)])
                    .send()
                    .await?;
                parse_google(&checked(resp).await?)
            }
            Self::Yandex { api_key, folder_id } => {
                if api_key.is_empty() {
                    return Err(TranslateError::NotConfigured("Yandex API key"));
                }
                let mut body = json!({
                    "folderId": folder_id,
                    "texts": [text],
                    "targetLanguageCode": dst,
                });
                if src != "auto" {
                    body["sourceLanguageCode"] = json!(src);
                }
                let resp = http
                    .post("https://translate.api.cloud.yandex.net/translate/v2/translate")
                    .header("Authorization", format!("Api-Key {api_key}"))
                    .json(&body)
                    .send()
                    .await?;
                parse_yandex(&checked(resp).await?)
            }
            Self::Custom { url, api_key } => {
                if url.is_empty() {
                    return Err(TranslateError::NotConfigured("URL своего API"));
                }
                let mut body = json!({"q": text, "source": src, "target": dst, "format": "text"});
                if !api_key.is_empty() {
                    body["api_key"] = json!(api_key);
                }
                let resp = http.post(url).json(&body).send().await?;
                parse_custom(&checked(resp).await?)
            }
        }
    }
}

async fn checked(resp: reqwest::Response) -> Result<Value, TranslateError> {
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(TranslateError::Status { status: status.as_u16(), body: body.chars().take(200).collect() });
    }
    Ok(resp.json().await?)
}

pub fn parse_google(v: &Value) -> Result<String, TranslateError> {
    let arr = v.get(0).and_then(Value::as_array).ok_or(TranslateError::BadResponse)?;
    let out: String = arr.iter().filter_map(|i| i.get(0).and_then(Value::as_str)).collect();
    if out.is_empty() { Err(TranslateError::BadResponse) } else { Ok(out) }
}

pub fn parse_yandex(v: &Value) -> Result<String, TranslateError> {
    v["translations"][0]["text"].as_str().map(str::to_owned).ok_or(TranslateError::BadResponse)
}

pub fn parse_custom(v: &Value) -> Result<String, TranslateError> {
    v["translatedText"].as_str().map(str::to_owned).ok_or(TranslateError::BadResponse)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_joins_segments() {
        let v = json!([[["Куда ", "Where ", null], ["ты идёшь?", "are you going?", null]], null, "en"]);
        assert_eq!(parse_google(&v).unwrap(), "Куда ты идёшь?");
    }

    #[test]
    fn yandex_and_custom() {
        assert_eq!(parse_yandex(&json!({"translations":[{"text":"привет"}]})).unwrap(), "привет");
        assert_eq!(parse_custom(&json!({"translatedText":"hola"})).unwrap(), "hola");
        assert!(parse_custom(&json!({"error":"x"})).is_err());
    }

    #[test]
    fn lang_mapping() {
        assert_eq!(tess_to_iso("eng"), "en");
        assert_eq!(tess_to_iso("xx"), "xx");
    }

    #[tokio::test]
    async fn unconfigured_services_fail_fast() {
        let http = Translator::client();
        let y = Translator::Yandex { api_key: String::new(), folder_id: String::new() };
        assert!(matches!(y.translate(&http, "a", "en", "ru").await, Err(TranslateError::NotConfigured(_))));
        let c = Translator::Custom { url: String::new(), api_key: String::new() };
        assert!(matches!(c.translate(&http, "a", "en", "ru").await, Err(TranslateError::NotConfigured(_))));
    }
}
