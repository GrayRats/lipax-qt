//! Кэш переводов (LRU в памяти) по ключу «пара языков + нормализованный текст».

use std::collections::{HashMap, VecDeque};

pub struct TranslationCache {
    cap: usize,
    map: HashMap<String, String>,
    order: VecDeque<String>,
}

fn key(src: &str, dst: &str, text: &str) -> String {
    format!("{src}\u{1}{dst}\u{1}{text}")
}

impl TranslationCache {
    pub fn new(cap: usize) -> Self {
        Self { cap: cap.max(1), map: HashMap::new(), order: VecDeque::new() }
    }

    pub fn get(&mut self, src: &str, dst: &str, text: &str) -> Option<String> {
        let k = key(src, dst, text);
        let v = self.map.get(&k).cloned()?;
        self.order.retain(|x| x != &k);
        self.order.push_back(k);
        Some(v)
    }

    pub fn put(&mut self, src: &str, dst: &str, text: &str, translation: String) {
        let k = key(src, dst, text);
        if self.map.insert(k.clone(), translation).is_some() {
            self.order.retain(|x| x != &k);
        } else if self.map.len() > self.cap
            && let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        self.order.push_back(k);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_and_miss() {
        let mut c = TranslationCache::new(2);
        assert!(c.get("en", "ru", "hi").is_none());
        c.put("en", "ru", "hi", "привет".into());
        assert_eq!(c.get("en", "ru", "hi").as_deref(), Some("привет"));
        assert!(c.get("en", "de", "hi").is_none());
    }

    #[test]
    fn evicts_least_recently_used() {
        let mut c = TranslationCache::new(2);
        c.put("a", "b", "1", "x".into());
        c.put("a", "b", "2", "y".into());
        c.get("a", "b", "1");
        c.put("a", "b", "3", "z".into());
        assert!(c.get("a", "b", "2").is_none());
        assert!(c.get("a", "b", "1").is_some());
    }
}
