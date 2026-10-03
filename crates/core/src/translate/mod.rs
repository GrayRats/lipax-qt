//! Движки перевода: Google (gtx, без ключа), Yandex Cloud Translate v2, свой API.

use crate::settings::{Settings, TranslatorKind};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Mutex, time::{Duration, Instant, SystemTime}};

#[derive(Debug, thiserror::Error)]
pub enum TranslateError {
    #[error("сервис ограничил частоту запросов (HTTP 429). Подождите {retry_after} с перед повтором или выберите другой сервис перевода")]
    RateLimited { retry_after: u64 },
    #[error("сеть: {0}")]
    Http(reqwest::Error),
    #[error("сервис вернул {status}: {body}")]
    Status { status: u16, body: String },
    #[error("неожиданный ответ сервиса")]
    BadResponse,
    #[error("не настроено: {0}")]
    NotConfigured(&'static str),
}

impl From<reqwest::Error> for TranslateError {
    fn from(error: reqwest::Error) -> Self {
        // A Google query contains the original text; custom URLs may contain credentials.
        Self::Http(error.without_url())
    }
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
    cooldowns: Mutex<HashMap<String, Instant>>,
}

impl HttpTranslate {
    pub fn new() -> Self {
        Self { http: Translator::client(), cooldowns: Mutex::new(HashMap::new()) }
    }
}

impl Default for HttpTranslate {
    fn default() -> Self {
        Self::new()
    }
}

impl Translate for HttpTranslate {
    async fn translate(&self, s: &Settings, text: &str, src: &str, dst: &str) -> Result<String, TranslateError> {
        tracing::debug!(service = ?s.translator, characters = text.chars().count(), source_language = src, target_language = dst, "Запрос перевода");
        let translator = Translator::from_settings(s);
        let key = match &translator {
            Translator::Google => "google".to_string(),
            Translator::Yandex { .. } => "yandex".to_string(),
            Translator::Custom { url, .. } => format!("custom:{url}"),
        };
        {
            let mut cooldowns = self.cooldowns.lock().unwrap();
            let now = Instant::now();
            cooldowns.retain(|_, until| *until > now);
            if let Some(until) = cooldowns.get(&key) {
                let remaining = until.duration_since(now);
                return Err(TranslateError::RateLimited { retry_after: remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0) });
            }
        }
        let result = translator.translate(&self.http, text, src, dst).await;
        if let Err(TranslateError::RateLimited { retry_after }) = &result {
            let now = Instant::now();
            let until = now.checked_add(Duration::from_secs(*retry_after))
                .unwrap_or_else(|| now + Duration::from_secs(365 * 24 * 3600));
            self.cooldowns.lock().unwrap().insert(key, until);
        }
        result
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
    tracing::debug!(http_status = status.as_u16(), "Ответ сервиса перевода");
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry_after = retry_after_seconds(resp.headers().get(reqwest::header::RETRY_AFTER).and_then(|h| h.to_str().ok()), SystemTime::now());
        return Err(TranslateError::RateLimited { retry_after });
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(TranslateError::Status { status: status.as_u16(), body: error_summary(&body) });
    }
    Ok(resp.json().await?)
}

fn retry_after_seconds(header: Option<&str>, now: SystemTime) -> u64 {
    header.and_then(|h| h.trim().parse::<u64>().ok().or_else(|| {
        httpdate::parse_http_date(h).ok()?.duration_since(now).ok().map(|d| d.as_secs() + u64::from(d.subsec_nanos() > 0))
    })).unwrap_or(60).max(1)
}

fn error_summary(body: &str) -> String {
    let json = serde_json::from_str::<Value>(body).ok();
    let message = json.as_ref().and_then(|v| v["error"]["message"].as_str().or_else(|| v["message"].as_str()).or_else(|| v["error"].as_str())).unwrap_or(body);
    if message.contains('<') || message.trim().is_empty() {
        "сервис недоступен или отклонил запрос; повторите позже либо выберите другой сервис".into()
    } else {
        message.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect()
    }
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
    fn rate_limit_delay_and_safe_errors() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(retry_after_seconds(Some("120"), now), 120);
        assert_eq!(retry_after_seconds(Some(&httpdate::fmt_http_date(now + Duration::from_secs(90))), now), 90);
        assert_eq!(retry_after_seconds(None, now), 60);
        assert_eq!(retry_after_seconds(Some("invalid"), now), 60);
        assert_eq!(retry_after_seconds(Some("0"), now), 1);
        let html = "<html><head><style>body { color: red; }</style></head><body>Sorry</body></html>";
        let message = error_summary(html);
        assert!(!message.contains('<') && !message.contains("color:"));
        assert_eq!(error_summary(r#"{"error":{"message":"Quota exceeded"}}"#), "Quota exceeded");
        assert_eq!(error_summary("  Service\n unavailable  "), "Service unavailable");
    }

    #[tokio::test]
    async fn rate_limit_blocks_requests_across_texts_until_expiry() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/translate", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut request = [0; 4096];
            stream.read(&mut request).unwrap();
            stream.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 120\r\nContent-Length: 19\r\nConnection: close\r\n\r\n<html>Sorry!</html> ").unwrap();
        });
        let translator = HttpTranslate { http: reqwest::Client::builder().no_proxy().build().unwrap(), cooldowns: Mutex::new(HashMap::new()) };
        let mut s = Settings { translator: TranslatorKind::Custom, custom_url: url.clone(), ..Settings::default() };
        let first = translator.translate(&s, "First field", "en", "ru").await.unwrap_err();
        assert!(matches!(first, TranslateError::RateLimited { retry_after: 120 }));
        assert!(!first.to_string().contains("<html>"));
        server.join().unwrap(); // The server is gone: subsequent calls must be stopped locally.
        let second = translator.translate(&s, "Another field", "en", "ru").await.unwrap_err();
        assert!(matches!(second, TranslateError::RateLimited { retry_after: 1..=120 }));
        s.custom_url.clear();
        assert!(matches!(translator.translate(&s, "Other service", "en", "ru").await, Err(TranslateError::NotConfigured(_))));
        s.custom_url = url.clone();
        translator.cooldowns.lock().unwrap().insert(format!("custom:{url}"), Instant::now());
        assert!(matches!(translator.translate(&s, "After expiry", "en", "ru").await, Err(TranslateError::Http(_))));
    }

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
