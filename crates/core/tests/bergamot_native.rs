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
