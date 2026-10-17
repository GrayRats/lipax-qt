//! Opt-in end-to-end test with a native engine and a verified English→Russian model.
//! LIPAX_BERGAMOT_TEST_BINARY=/path/bergamot LIPAX_BERGAMOT_TEST_MODELS=/path/en-ru \
//! cargo test -p lipa-core --test bergamot_native -- --ignored
#[tokio::test]
#[ignore = "requires the compiled native Bergamot engine and an installed en-ru model"]
async fn translates_with_the_native_engine_and_verified_model() {
    let binary = std::env::var("LIPAX_BERGAMOT_TEST_BINARY").expect("native engine path");
    let models = std::env::var("LIPAX_BERGAMOT_TEST_MODELS").expect("en-ru model directory");
    let output = lipa_core::translate::bergamot::translate(
        &binary, &models, "Someone's knocking at the door. Definitely.", "en", "ru"
    ).await.expect("native translation");
    assert!(output.contains("двер"), "unexpected Russian translation: {output}");
    assert!(!output.contains("Someone"));
}

#[tokio::test]
#[ignore = "requires the compiled native Bergamot engine and an installed en-ru model"]
async fn translates_a_batch_in_one_process_in_order() {
    use std::time::Instant;
    let binary = std::env::var("LIPAX_BERGAMOT_TEST_BINARY").expect("native engine path");
    let models = std::env::var("LIPAX_BERGAMOT_TEST_MODELS").expect("en-ru model directory");
    let texts: Vec<String> = ["Hello there.", "Where are you going?\nTell me.", "Start game"].map(String::from).to_vec();
    let started = Instant::now();
    let single = lipa_core::translate::bergamot::translate(&binary, &models, &texts[0], "en", "ru").await.expect("single");
    let single_time = started.elapsed();
    let started = Instant::now();
    let batch = lipa_core::translate::bergamot::translate_many(&binary, &models, &texts, "en", "ru").await.expect("batch");
    let batch_time = started.elapsed();
    eprintln!("single call: {single_time:?}, batch of {}: {batch_time:?}; output: {batch:?}", texts.len());
    assert_eq!(batch.len(), 3);
    let has_cyrillic = |t: &String| t.chars().any(|c| ('\u{400}'..='\u{4ff}').contains(&c));
    assert!(batch.iter().all(has_cyrillic), "{batch:?}");
    assert!(!batch.iter().any(|t| t.contains("Hello") || t.contains("Start game")), "{batch:?}");
    assert!(single.contains(&batch[0]) || batch[0].contains(&single) || has_cyrillic(&single));
    assert!(batch_time <= single_time * 2, "batch {batch_time:?} is slower than twice a single call {single_time:?}");
}
