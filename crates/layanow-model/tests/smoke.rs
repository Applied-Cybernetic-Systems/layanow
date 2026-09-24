//! M0 smoke test against the real ONNX bundle.
//!
//! Ignored by default because it needs the ~1.7 GB `receptron/laya-onnx`
//! bundle. Run it with the bundle already cached (or with
//! `LAYANOW_ALLOW_MODEL_DOWNLOAD=1` to fetch it first):
//!
//! ```sh
//! cargo test -p layanow-model --test smoke -- --ignored
//! ```

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use layanow_model::{Decider, ModelError, Quant, bundle};

#[test]
#[ignore = "needs the Laya ONNX bundle; set LAYANOW_ALLOW_MODEL_DOWNLOAD=1 to fetch it"]
fn three_option_choice_is_plausible() {
    let spec = &bundle::ENGLISH;
    let variant = bundle::variant_or_default(spec, Quant::Fp32).expect("variant");
    let dir = bundle::ensure_bundle(spec, variant).expect("bundle unavailable");
    bundle::verify_bundle_from(spec, variant, &dir).expect("bundle integrity");
    let checkpoint = bundle::checkpoint_from_dir(spec, variant, &dir).expect("descriptor");
    let mut decider = Decider::load(&checkpoint).expect("load session");

    let answers = vec!["Venus".to_string(), "Mars".to_string(), "Jupiter".to_string()];
    let ranked = decider
        .decide_choice("{}", "Which planet is known as the Red Planet?", &answers)
        .expect("decision");

    assert_eq!(ranked.len(), 3);
    let total: f32 = ranked.iter().map(|answer| answer.probability).sum();
    assert!((total - 1.0).abs() < 1e-3, "probabilities sum to {total}");
    assert!(ranked.windows(2).all(|pair| pair[0].probability >= pair[1].probability));
    assert_eq!(ranked[0].text, "Mars");
}

#[test]
#[ignore = "needs the multilingual ONNX bundle; set LAYANOW_ALLOW_MODEL_DOWNLOAD=1 to fetch it"]
fn multilingual_choice_is_plausible() {
    let spec = &bundle::MULTILINGUAL;
    let variant = bundle::variant_or_default(spec, Quant::Fp32).expect("variant");
    let dir = bundle::ensure_bundle(spec, variant).expect("bundle unavailable");
    let checkpoint = bundle::checkpoint_from_dir(spec, variant, &dir).expect("descriptor");
    let mut decider = Decider::load(&checkpoint).expect("load session");

    let answers = vec!["Venus".to_string(), "Mars".to_string(), "Jupiter".to_string()];
    let ranked = decider
        .decide_choice("{}", "Which planet is known as the Red Planet?", &answers)
        .expect("decision");

    assert_eq!(ranked.len(), 3);
    let total: f32 = ranked.iter().map(|answer| answer.probability).sum();
    assert!((total - 1.0).abs() < 1e-3, "probabilities sum to {total}");
    assert_eq!(ranked[0].text, "Mars");
}

#[test]
#[ignore = "needs network to read the model manifest"]
fn missing_files_fail_verification() {
    // An empty directory can never match the manifest's sizes/digests.
    let dir = std::env::temp_dir().join(format!("layanow-verify-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let spec = &bundle::ENGLISH;
    let variant = bundle::variant_or_default(spec, Quant::Fp32).unwrap();
    let result = bundle::verify_bundle_from(spec, variant, &dir);
    assert!(matches!(result, Err(ModelError::ChecksumMismatch { .. })));
}
