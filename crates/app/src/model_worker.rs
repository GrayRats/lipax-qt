//! Own model work independently of windows and the capture pipeline.
#[cxx::bridge]
mod ffi {
    extern "Rust" {
        type ModelJob;
        fn run_model_job(job: Box<ModelJob>);
    }
    unsafe extern "C++" {
        include!("model_worker.h");
        fn startModelThread(job: Box<ModelJob>);
        fn joinModelThreads();
    }
}

pub struct ModelJob {
    run: Box<dyn FnOnce() + Send>,
}
#[allow(clippy::boxed_local)] // CXX transfers ownership of this opaque Rust type via Box.
fn run_model_job(job: Box<ModelJob>) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(job.run)).is_err() {
        tracing::error!("Model worker panicked");
    }
}
fn cancellation() -> &'static tokio::sync::watch::Sender<bool> {
    static CANCEL: std::sync::OnceLock<tokio::sync::watch::Sender<bool>> =
        std::sync::OnceLock::new();
    CANCEL.get_or_init(|| tokio::sync::watch::channel(false).0)
}
pub fn spawn(run: impl FnOnce(tokio::sync::watch::Receiver<bool>) + Send + 'static) {
    let cancel = cancellation().subscribe();
    ffi::startModelThread(Box::new(ModelJob {
        run: Box::new(move || run(cancel)),
    }));
}
pub fn shutdown() {
    cancellation().send_replace(true);
    ffi::joinModelThreads();
}

/// Run networking and filesystem work on the calling QThread. Convert a panic to a
/// normal completion so the GUI can clear its busy state and offer a retry.
pub fn prepare(
    pair: &str,
    paths: &[std::path::PathBuf],
    cancel: &mut tokio::sync::watch::Receiver<bool>,
    progress: impl FnMut(i32),
) -> Result<std::path::PathBuf, String> {
    prepare_catching_panic(|| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        runtime.block_on(async {
            if *cancel.borrow() { return Err("Model installation cancelled.".into()); }
            let root = lipa_core::translate::models::cache_root();
            let firefox = dirs::home_dir().unwrap_or_default().join(".mozilla/firefox");
            tokio::select! {
                _ = cancel.changed() => Err("Model installation cancelled.".into()),
                result = lipa_core::translate::models::ensure(pair, paths, &root, &firefox, progress) => result,
            }
        })
    })
}

fn prepare_catching_panic<T>(work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
        .unwrap_or_else(|_| Err("Model worker failed. Retry the download in Settings.".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panic_completes_the_job_with_a_retryable_error() {
        let result: Result<(), String> = prepare_catching_panic(|| panic!("worker failure"));
        assert!(result.unwrap_err().contains("Retry"));
    }
    #[test]
    fn shutdown_before_start_does_not_touch_models() {
        let (_sender, mut cancel) = tokio::sync::watch::channel(true);
        assert_eq!(
            prepare("en-ru", &[], &mut cancel, |_| panic!("must not start")),
            Err("Model installation cancelled.".into())
        );
    }
}
