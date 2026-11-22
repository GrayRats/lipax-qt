//! A long-lived `bergamot-server` per model: the model is loaded once and answers many requests, instead of one
//! process start plus model load (≈0.17 s measured) for every translation. The protocol is documented in
//! `packaging/bergamot-server.cpp`; everything here is an optimization with a fallback. When the server binary is
//! missing (a stock CLI, a manual override), does not start, or breaks, the caller runs the one-shot CLI as before.
//!
//! Set `LIPAX_BERGAMOT_PERSISTENT=0` to turn the persistent worker off.

use super::super::TranslateError;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, LazyLock, Mutex as StdMutex, Weak},
    time::{Duration, Instant, SystemTime},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};

const HANDSHAKE: &str = "LIPAX-BERGAMOT-SERVER 1";
const START_TIMEOUT: Duration = Duration::from_secs(30);
/// An idle worker is stopped after this long; the next request starts a new one.
#[cfg(not(test))]
const IDLE: Duration = Duration::from_secs(180);
#[cfg(test)]
const IDLE: Duration = Duration::from_millis(600);
/// After the worker failed to start or broke twice in a row, the one-shot path is used for this long.
const COOLDOWN: Duration = Duration::from_secs(60);
const MAX_FRAME: usize = 16 * 1024 * 1024;
const STDERR_TAIL: usize = 400;

/// What to run: the server executable and the model config (its directory is the working directory).
pub struct Spec { pub server: PathBuf, pub config: PathBuf, pub threads: u32 }

/// Why the worker could not answer.
pub enum Fail {
    /// Not started / broke / disabled: the caller should use the one-shot CLI.
    Unavailable(String),
    /// The worker is fine but the request failed (timeout, bad reply): report it, do not retry.
    Error(TranslateError),
}

/// The server binary shipped next to the CLI, if there is one and the feature is not disabled.
pub fn server_for(cli: &Path) -> Option<PathBuf> {
    if std::env::var_os("LIPAX_BERGAMOT_PERSISTENT").is_some_and(|v| v == "0") { return None; }
    let server = cli.parent()?.join("bergamot-server");
    let usable = std::fs::metadata(&server).is_ok_and(|m| {
        use std::os::unix::fs::PermissionsExt;
        m.is_file() && m.permissions().mode() & 0o111 != 0
    });
    usable.then_some(server)
}

#[derive(PartialEq, Clone)]
struct Stamp { server: PathBuf, config: Option<SystemTime>, dir: Option<SystemTime> }

fn stamp(spec: &Spec) -> Stamp {
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    Stamp { server: spec.server.clone(), config: modified(&spec.config), dir: spec.config.parent().and_then(modified) }
}

struct Worker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    stderr: Arc<StdMutex<String>>,
    last_used: Instant,
    served: u64,
}

#[derive(Default)]
struct Slot { worker: Option<Worker>, stamp: Option<Stamp>, cooldown_until: Option<Instant>, failures: u32 }

type Pool = StdMutex<HashMap<PathBuf, Arc<Mutex<Slot>>>>;
static POOL: LazyLock<Pool> = LazyLock::new(Default::default);

fn slot_for(config: &Path) -> Arc<Mutex<Slot>> {
    let mut pool = POOL.lock().unwrap_or_else(|e| e.into_inner());
    pool.entry(config.to_path_buf()).or_default().clone()
}

/// Translate `input` with the worker of `spec`, starting it if needed. Requests for one model are serialized.
pub async fn request(spec: &Spec, input: &str, timeout: Duration) -> Result<String, Fail> {
    let slot = slot_for(&spec.config);
    let mut guard = slot.lock().await;
    let now = Instant::now();
    if guard.cooldown_until.is_some_and(|until| until > now) {
        return Err(Fail::Unavailable("постоянный процесс Bergamot недавно не запустился".into()));
    }
    let current = stamp(spec);
    if guard.stamp.as_ref() != Some(&current) { guard.worker = None; }
    // A worker that died while idle makes its first request fail: retry once with a fresh one.
    for attempt in 0..2 {
        let fresh = guard.worker.is_none();
        if fresh {
            match spawn(spec).await {
                Ok(worker) => {
                    guard.worker = Some(worker);
                    guard.stamp = Some(current.clone());
                    reaper(Arc::downgrade(&slot));
                }
                Err(detail) => return Err(give_up(&mut guard, detail)),
            }
        }
        let worker = guard.worker.as_mut().expect("worker was just ensured");
        match tokio::time::timeout(timeout, worker.exchange(input)).await {
            Ok(Ok(text)) => {
                worker.last_used = Instant::now();
                worker.served += 1;
                guard.failures = 0;
                return String::from_utf8(text).map_err(|_| Fail::Error(TranslateError::Local("Bergamot вернул текст не в UTF-8".into())));
            }
            Ok(Err(error)) => {
                let detail = format!("{error}{}", worker.stderr_tail());
                let was_reused = !fresh;
                guard.worker = None;
                if was_reused && attempt == 0 { continue; }
                return Err(give_up(&mut guard, detail));
            }
            Err(_) => {
                guard.worker = None;
                return Err(Fail::Error(TranslateError::Local(format!("Bergamot не ответил за {} с", timeout.as_secs()))));
            }
        }
    }
    unreachable!("the second attempt always returns")
}

fn give_up(slot: &mut Slot, detail: String) -> Fail {
    slot.worker = None;
    slot.failures += 1;
    slot.cooldown_until = Some(Instant::now() + COOLDOWN);
    tracing::warn!(%detail, "Постоянный процесс Bergamot недоступен, перевод одноразовым процессом");
    Fail::Unavailable(detail)
}

async fn spawn(spec: &Spec) -> Result<Worker, String> {
    let mut child = Command::new(&spec.server)
        .arg("--model-config-paths").arg(&spec.config)
        .arg("--cpu-threads").arg(spec.threads.to_string())
        .current_dir(spec.config.parent().ok_or("у конфигурации модели нет каталога")?)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .kill_on_drop(true).spawn()
        .map_err(|e| format!("запуск {}: {e}", spec.server.display()))?;
    let stdin = child.stdin.take().expect("piped stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
    // The engine logs to stderr; it must be drained or a full pipe would stop it.
    let stderr = Arc::new(StdMutex::new(String::new()));
    if let Some(mut pipe) = child.stderr.take() {
        let tail = stderr.clone();
        tokio::spawn(async move {
            let mut buffer = [0u8; 2048];
            while let Ok(n) = pipe.read(&mut buffer).await {
                if n == 0 { break; }
                let mut tail = tail.lock().unwrap_or_else(|e| e.into_inner());
                tail.push_str(&String::from_utf8_lossy(&buffer[..n]));
                if tail.len() > STDERR_TAIL * 4 {
                    let cut = tail.len() - STDERR_TAIL;
                    let cut = (cut..tail.len()).find(|&i| tail.is_char_boundary(i)).unwrap_or(tail.len());
                    tail.drain(..cut);
                }
            }
        });
    }
    let mut line = String::new();
    let ready = tokio::time::timeout(START_TIMEOUT, stdout.read_line(&mut line)).await;
    let mut worker = Worker { child, stdin, stdout, stderr, last_used: Instant::now(), served: 0 };
    match ready {
        Ok(Ok(n)) if n > 0 && line.trim_end() == HANDSHAKE => Ok(worker),
        Ok(Ok(0)) | Ok(Err(_)) => {
            let status = worker.child.try_wait().ok().flatten().map(|s| format!(" ({s})")).unwrap_or_default();
            Err(format!("процесс завершился до готовности{status}{}", worker.stderr_tail()))
        }
        Ok(Ok(_)) => Err(format!("неожиданное приветствие: {:?}", line.trim_end().chars().take(60).collect::<String>())),
        Err(_) => Err(format!("не стал готов за {} с", START_TIMEOUT.as_secs())),
    }
}

/// Stops the worker of a slot once it has been idle for [`IDLE`]; ends when the slot has no worker.
fn reaper(slot: Weak<Mutex<Slot>>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(IDLE / 3).await;
            let Some(slot) = slot.upgrade() else { return };
            let mut slot = slot.lock().await;
            match &slot.worker {
                None => return,
                Some(worker) if worker.last_used.elapsed() >= IDLE => { slot.worker = None; return; }
                Some(_) => {}
            }
        }
    });
}

impl Worker {
    async fn exchange(&mut self, input: &str) -> std::io::Result<Vec<u8>> {
        let bytes = input.as_bytes();
        let length = u32::try_from(bytes.len()).ok().filter(|&n| n as usize <= MAX_FRAME)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "запрос слишком велик"))?;
        // One write, so a request is never half sent.
        let mut frame = Vec::with_capacity(4 + bytes.len());
        frame.extend_from_slice(&length.to_be_bytes());
        frame.extend_from_slice(bytes);
        self.stdin.write_all(&frame).await?;
        self.stdin.flush().await?;
        let mut header = [0u8; 4];
        self.stdout.read_exact(&mut header).await?;
        let size = u32::from_be_bytes(header) as usize;
        if size > MAX_FRAME { return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "ответ слишком велик")); }
        let mut text = vec![0u8; size];
        self.stdout.read_exact(&mut text).await?;
        Ok(text)
    }

    fn stderr_tail(&self) -> String {
        let tail = self.stderr.lock().unwrap_or_else(|e| e.into_inner());
        let text: String = tail.split_whitespace().collect::<Vec<_>>().join(" ");
        let text: String = text.chars().rev().take(STDERR_TAIL).collect::<Vec<_>>().into_iter().rev().collect();
        if text.is_empty() { String::new() } else { format!(": {text}") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A fake server: the handshake, then `cat` echoes every frame back unchanged, so the "translation" is the input.
    /// `extra` runs before the echo loop; every start appends a line to `starts`.
    fn fake(dir: &Path, name: &str, extra: &str) -> Spec {
        let server = dir.join(name);
        // Outside the model directory: a file created there would change the directory time that marks a replaced model.
        std::fs::create_dir_all(dir.join("starts")).unwrap();
        let starts = dir.join("starts").join(name);
        std::fs::write(&server, format!("#!/bin/sh\necho start >> '{}'\nprintf 'LIPAX-BERGAMOT-SERVER 1\\n'\n{extra}\nexec cat\n", starts.display())).unwrap();
        std::fs::set_permissions(&server, std::fs::Permissions::from_mode(0o755)).unwrap();
        let config = dir.join(format!("{name}.yml"));
        std::fs::write(&config, "x").unwrap();
        Spec { server, config, threads: 1 }
    }

    fn starts(spec: &Spec) -> usize {
        let path = spec.server.parent().unwrap().join("starts").join(spec.server.file_name().unwrap());
        std::fs::read_to_string(path).map(|s| s.lines().count()).unwrap_or(0)
    }

    const T: Duration = Duration::from_secs(10);

    #[tokio::test]
    async fn one_process_serves_many_requests() {
        let dir = tempfile::tempdir().unwrap();
        let spec = fake(dir.path(), "reuse", "");
        let big = "x".repeat(300_000);
        for text in ["Hello", "Привет, мир\nвторая строка", "", big.as_str()] {
            let out = request(&spec, text, T).await.ok().expect("answer");
            assert_eq!(out, text);
        }
        assert_eq!(starts(&spec), 1, "the model is loaded once");
    }

    #[tokio::test]
    async fn a_worker_that_died_while_idle_is_replaced_without_failing_the_request() {
        let dir = tempfile::tempdir().unwrap();
        let spec = fake(dir.path(), "died", "");
        assert_eq!(request(&spec, "one", T).await.ok().unwrap(), "one");
        // Kill the running worker behind the pool's back.
        {
            let slot = slot_for(&spec.config);
            let mut guard = slot.lock().await;
            guard.worker.as_mut().unwrap().child.kill().await.unwrap();
        }
        assert_eq!(request(&spec, "two", T).await.ok().unwrap(), "two");
        assert_eq!(starts(&spec), 2);
    }

    #[tokio::test]
    async fn a_server_that_never_greets_is_unavailable_and_then_cooled_down() {
        let dir = tempfile::tempdir().unwrap();
        let server = dir.path().join("mute");
        std::fs::write(&server, "#!/bin/sh\necho 'bad model' >&2\nexit 1\n").unwrap();
        std::fs::set_permissions(&server, std::fs::Permissions::from_mode(0o755)).unwrap();
        let config = dir.path().join("mute.yml");
        std::fs::write(&config, "x").unwrap();
        let spec = Spec { server, config, threads: 1 };
        let Err(Fail::Unavailable(detail)) = request(&spec, "a", T).await else { panic!("must be unavailable") };
        assert!(detail.contains("bad model"), "{detail}");
        let Err(Fail::Unavailable(detail)) = request(&spec, "a", T).await else { panic!("must stay unavailable") };
        assert!(detail.contains("недавно"), "cooled down, no new process: {detail}");
    }

    #[tokio::test]
    async fn a_server_that_breaks_right_after_the_greeting_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let spec = fake(dir.path(), "broken", "head -c 4 >/dev/null; exit 1");
        assert!(matches!(request(&spec, "a", T).await, Err(Fail::Unavailable(_))));
    }

    #[tokio::test]
    async fn a_silent_server_hits_the_request_timeout_and_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let spec = fake(dir.path(), "slow", "head -c 4 >/dev/null; sleep 30");
        let started = Instant::now();
        let Err(Fail::Error(error)) = request(&spec, "a", Duration::from_millis(300)).await else { panic!("a timeout is an error") };
        assert!(error.to_string().contains("не ответил"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(5));
        let slot = slot_for(&spec.config);
        assert!(slot.lock().await.worker.is_none(), "a worker that timed out is stopped");
    }

    #[tokio::test]
    async fn a_replaced_model_restarts_the_worker() {
        let dir = tempfile::tempdir().unwrap();
        let spec = fake(dir.path(), "stamp", "");
        request(&spec, "a", T).await.ok().unwrap();
        std::thread::sleep(Duration::from_millis(30));
        std::fs::write(&spec.config, "changed").unwrap();
        request(&spec, "b", T).await.ok().unwrap();
        assert_eq!(starts(&spec), 2, "a changed model config starts a fresh worker");
    }

    #[tokio::test]
    async fn an_idle_worker_is_stopped_and_the_next_request_starts_a_new_one() {
        let dir = tempfile::tempdir().unwrap();
        let spec = fake(dir.path(), "idle", "");
        request(&spec, "a", T).await.ok().unwrap();
        tokio::time::sleep(IDLE * 3).await;
        assert!(slot_for(&spec.config).lock().await.worker.is_none(), "stopped after the idle time");
        assert_eq!(request(&spec, "b", T).await.ok().unwrap(), "b");
        assert_eq!(starts(&spec), 2);
    }

    #[test]
    fn the_server_is_found_next_to_the_cli_and_can_be_switched_off() {
        let dir = tempfile::tempdir().unwrap();
        let cli = dir.path().join("bergamot");
        assert!(server_for(&cli).is_none(), "no server binary");
        let server = dir.path().join("bergamot-server");
        std::fs::write(&server, "#!/bin/sh\n").unwrap();
        assert!(server_for(&cli).is_none(), "not executable");
        std::fs::set_permissions(&server, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(server_for(&cli).as_deref(), Some(server.as_path()));
    }
}
