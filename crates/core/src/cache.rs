//! Кэш переводов (LRU в памяти) по ключу «сервис + пара языков + текст».
//! Pipeline включает идентификатор сервиса в аргумент `src` для сохранения интерфейса кэша.

use std::collections::HashMap;

const NIL: usize = usize::MAX;

struct Node {
    key: Box<str>,
    value: String,
    prev: usize,
    next: usize,
}

/// LRU-кэш с O(1) на `get`/`put`: хеш-индекс указывает в slab-массив узлов,
/// которые образуют intrusive двусвязный список (`head` — самый свежий, `tail` — самый старый).
pub struct TranslationCache {
    cap: usize,
    index: HashMap<Box<str>, usize>,
    nodes: Vec<Node>,
    free: Vec<usize>,
    head: usize,
    tail: usize,
}

fn key(src: &str, dst: &str, text: &str) -> String {
    format!("{src}\u{1}{dst}\u{1}{text}")
}

impl TranslationCache {
    pub fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            index: HashMap::new(),
            nodes: Vec::new(),
            free: Vec::new(),
            head: NIL,
            tail: NIL,
        }
    }

    pub fn get(&mut self, src: &str, dst: &str, text: &str) -> Option<String> {
        let i = *self.index.get(key(src, dst, text).as_str())?;
        self.touch(i);
        Some(self.nodes[i].value.clone())
    }

    pub fn put(&mut self, src: &str, dst: &str, text: &str, translation: String) {
        let k = key(src, dst, text);
        if let Some(&i) = self.index.get(k.as_str()) {
            self.nodes[i].value = translation;
            self.touch(i);
            return;
        }
        if self.index.len() >= self.cap {
            let victim = self.tail;
            self.unlink(victim);
            self.index.remove(&self.nodes[victim].key);
            self.free.push(victim);
        }
        let key: Box<str> = k.into_boxed_str();
        let node = Node { key: key.clone(), value: translation, prev: NIL, next: NIL };
        let i = match self.free.pop() {
            Some(i) => {
                self.nodes[i] = node;
                i
            }
            None => {
                self.nodes.push(node);
                self.nodes.len() - 1
            }
        };
        self.index.insert(key, i);
        self.push_front(i);
    }

    /// Отцепляет узел `i` от списка (узел остаётся в `nodes`).
    fn unlink(&mut self, i: usize) {
        let (prev, next) = (self.nodes[i].prev, self.nodes[i].next);
        if prev == NIL {
            self.head = next;
        } else {
            self.nodes[prev].next = next;
        }
        if next == NIL {
            self.tail = prev;
        } else {
            self.nodes[next].prev = prev;
        }
    }

    /// Вставляет отцепленный узел `i` в голову списка.
    fn push_front(&mut self, i: usize) {
        self.nodes[i].prev = NIL;
        self.nodes[i].next = self.head;
        if self.head == NIL {
            self.tail = i;
        } else {
            self.nodes[self.head].prev = i;
        }
        self.head = i;
    }

    /// Помечает узел `i` как самый свежий.
    fn touch(&mut self, i: usize) {
        if self.head != i {
            self.unlink(i);
            self.push_front(i);
        }
    }

    /// Ключи от самого свежего к самому старому (для тестов).
    #[cfg(test)]
    fn order(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut i = self.head;
        while i != NIL {
            out.push(self.nodes[i].key.to_string());
            i = self.nodes[i].next;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Эталонная модель: список от самого свежего к самому старому.
    struct Model {
        cap: usize,
        items: Vec<(String, String)>,
    }

    impl Model {
        fn new(cap: usize) -> Self {
            Self { cap: cap.max(1), items: Vec::new() }
        }

        fn get(&mut self, k: &str) -> Option<String> {
            let pos = self.items.iter().position(|(key, _)| key == k)?;
            let item = self.items.remove(pos);
            let value = item.1.clone();
            self.items.insert(0, item);
            Some(value)
        }

        fn put(&mut self, k: &str, v: String) {
            if let Some(pos) = self.items.iter().position(|(key, _)| key == k) {
                self.items.remove(pos);
            } else if self.items.len() >= self.cap {
                self.items.pop();
            }
            self.items.insert(0, (k.to_string(), v));
        }

        fn keys(&self) -> Vec<String> {
            self.items.iter().map(|(k, _)| k.clone()).collect()
        }
    }

    /// Минимальный xorshift64 для детерминированной псевдослучайной последовательности.
    struct XorShift(u64);

    impl XorShift {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
    }

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

    #[test]
    fn cap_zero_behaves_as_cap_one() {
        let mut c = TranslationCache::new(0);
        c.put("a", "b", "1", "x".into());
        c.put("a", "b", "2", "y".into());
        assert!(c.get("a", "b", "1").is_none());
        assert_eq!(c.get("a", "b", "2").as_deref(), Some("y"));
        c.put("a", "b", "2", "z".into());
        assert_eq!(c.get("a", "b", "2").as_deref(), Some("z"));
        assert_eq!(c.order().len(), 1);
    }

    #[test]
    fn re_put_refreshes_recency_and_replaces_value() {
        let mut c = TranslationCache::new(2);
        c.put("a", "b", "1", "x".into());
        c.put("a", "b", "2", "y".into());
        // Повторный put старого ключа делает его самым свежим и заменяет значение.
        c.put("a", "b", "1", "x2".into());
        c.put("a", "b", "3", "z".into());
        assert!(c.get("a", "b", "2").is_none());
        assert_eq!(c.get("a", "b", "1").as_deref(), Some("x2"));
        assert_eq!(c.get("a", "b", "3").as_deref(), Some("z"));
    }

    #[test]
    fn capacity_one_keeps_only_latest() {
        let mut c = TranslationCache::new(1);
        c.put("a", "b", "1", "x".into());
        assert_eq!(c.get("a", "b", "1").as_deref(), Some("x"));
        c.put("a", "b", "2", "y".into());
        assert!(c.get("a", "b", "1").is_none());
        assert_eq!(c.get("a", "b", "2").as_deref(), Some("y"));
    }

    #[test]
    fn many_gets_match_reference_model_at_cap_4096() {
        const CAP: usize = 4096;
        const KEYS: u64 = 5000;
        const STEPS: usize = 100_000;
        let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
        let mut c = TranslationCache::new(CAP);
        let mut m = Model::new(CAP);
        for step in 0..STEPS {
            let text = format!("t{}", rng.next() % KEYS);
            let k = key("en", "ru", &text);
            let got = c.get("en", "ru", &text);
            assert_eq!(got, m.get(&k), "get mismatch at step {step}");
            if got.is_none() {
                let v = format!("v{step}");
                c.put("en", "ru", &text, v.clone());
                m.put(&k, v);
            }
            if step % 1000 == 0 || step + 1 == STEPS {
                assert_eq!(c.order(), m.keys(), "LRU order mismatch at step {step}");
            }
        }
        assert_eq!(c.order(), m.keys());
    }

    #[test]
    fn mixed_ops_match_reference_model() {
        for (cap, keys, seed) in [(1usize, 3u64, 1u64), (3, 8, 7), (16, 40, 42), (64, 100, 99)] {
            let mut rng = XorShift(seed);
            let mut c = TranslationCache::new(cap);
            let mut m = Model::new(cap);
            for step in 0..20_000usize {
                let text = format!("t{}", rng.next() % keys);
                let k = key("en", "ru", &text);
                if rng.next().is_multiple_of(3) {
                    let v = format!("v{step}");
                    c.put("en", "ru", &text, v.clone());
                    m.put(&k, v);
                } else {
                    assert_eq!(c.get("en", "ru", &text), m.get(&k), "cap {cap} step {step}");
                }
                assert_eq!(c.order(), m.keys(), "cap {cap} order at step {step}");
            }
        }
    }
}
