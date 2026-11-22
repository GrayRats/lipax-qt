//! Latency of uncached Bergamot translations, one call per line, as the pipeline makes them.
//! `cargo run --release -p lipa-core --example bergamot_latency -- <bergamot-cli> <en-ru|...> [calls]`
//! Run it twice, with and without `LIPAX_BERGAMOT_PERSISTENT=0`, to compare the persistent worker with one process per call.
use std::time::Instant;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let binary = args.next().expect("path to the bergamot CLI");
    let pair = args.next().unwrap_or_else(|| "en-ru".into());
    let calls: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(20);
    let (src, dst) = pair.split_once('-').expect("pair like en-ru");
    let lines = ["Press any key to continue.", "You do not have enough gold.", "The door is locked from the other side.",
        "Welcome back, traveler. What brings you here?", "Save game", "Your inventory is full.", "I have a bad feeling about this.",
        "Return to the village before nightfall.", "New quest available", "The merchant looks at you suspiciously."];
    println!("persistent worker: {}", std::env::var_os("LIPAX_BERGAMOT_PERSISTENT").is_none_or(|v| v != "0"));
    let mut times = Vec::new();
    for i in 0..calls {
        // A suffix makes every call a new text, so nothing could be served from a cache.
        let text = format!("{} ({i})", lines[i % lines.len()]);
        let started = Instant::now();
        let out = lipa_core::translate::bergamot::translate(&binary, "", &text, src, dst).await.expect("translation");
        times.push(started.elapsed().as_secs_f64() * 1000.0);
        if i < 3 { println!("{text:50} -> {out}"); }
    }
    let first = times[0];
    let mut rest = times[1..].to_vec();
    rest.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("first call {first:.0} ms; next {} calls: median {:.0} ms, mean {:.0} ms, max {:.0} ms",
        rest.len(), rest[rest.len() / 2], rest.iter().sum::<f64>() / rest.len() as f64, rest[rest.len() - 1]);
}
