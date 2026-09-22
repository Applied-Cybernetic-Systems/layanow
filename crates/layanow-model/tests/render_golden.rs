//! Golden tests for the `rl_common.py` port (M2).
//!
//! The fixture (`tests/fixtures/render_golden.json`) is generated once by
//! `tools/golden/gen_render_fixtures.py`, which runs the *reference*
//! `rl_common.build_sequence` / `render_options` against a checkpoint's real
//! tokenizer. This test replays it offline: the recorded tokenizer calls stand
//! in for the checkpoint, so no weights or tokenizer file are needed. It pins
//! the exact head/option/state strings the port builds, the assembled ids, the
//! `[MASK]` marker positions, and the calibration/confidence arithmetic.
//!
//! Regenerate the fixture after an intentional rendering change; see the
//! generator's doc comment.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::collections::HashMap;

use layanow_model::LayaConfig;
use layanow_model::render::{self, CHOICE_QTYPE_INDEX, SpecialIds};
use serde::Deserialize;

/// The committed golden fixture, embedded so the test is hermetic.
const FIXTURE: &str = include_str!("fixtures/render_golden.json");

#[derive(Deserialize)]
struct Fixture {
    special: Special,
    cases: Vec<RenderCase>,
    calibration: Calibration,
}

#[derive(Deserialize)]
struct Special {
    cls: i64,
    sep: i64,
    mask: i64,
    pad: i64,
    mask_tok: String,
}

#[derive(Deserialize)]
struct RenderCase {
    name: String,
    max_len: usize,
    head_max_len: usize,
    state: String,
    instructions: String,
    criteria: Vec<(String, String)>,
    options: Vec<String>,
    tokens: HashMap<String, Vec<i64>>,
    expected_ids: Vec<i64>,
    expected_markers: Vec<i64>,
}

#[derive(Deserialize)]
struct Calibration {
    #[serde(rename = "temperature_cases")]
    temperature: Vec<TemperatureCase>,
    #[serde(rename = "confidence_cases")]
    confidence: Vec<ConfidenceCase>,
    #[serde(rename = "bucket_cases")]
    buckets: Vec<BucketCase>,
}

#[derive(Deserialize)]
struct TemperatureCase {
    name: String,
    temperature: f32,
    logits: Vec<f32>,
    expected_probs: Vec<f32>,
}

#[derive(Deserialize)]
struct ConfidenceCase {
    name: String,
    probs: Vec<f32>,
    expected: f32,
}

#[derive(Deserialize)]
struct BucketCase {
    qtype: String,
    options: usize,
    expected: String,
}

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("parse render_golden.json")
}

#[test]
fn rendering_matches_reference() {
    let fixture = fixture();
    let ids = SpecialIds {
        cls: fixture.special.cls,
        sep: fixture.special.sep,
        mask: fixture.special.mask,
        pad: fixture.special.pad,
        mask_tok: fixture.special.mask_tok.clone(),
    };

    for case in &fixture.cases {
        let options = render::render_choice_options(&case.criteria);
        assert_eq!(options, case.options, "{}: rendered options", case.name);

        // The recorded calls are the reference tokenizer; any string the port
        // builds differently is reported rather than silently tokenized.
        let tokens = case.tokens.clone();
        let encode = move |text: &str| -> Result<Vec<i64>, String> {
            tokens
                .get(text)
                .cloned()
                .ok_or_else(|| format!("port tokenized an unexpected string: {text:?}"))
        };

        let rendered = render::build_sequence(
            encode,
            &ids,
            &case.state,
            &case.instructions,
            &case.options,
            case.max_len,
            case.head_max_len,
        )
        .unwrap_or_else(|error| panic!("{}: {error}", case.name));

        assert_eq!(rendered.ids, case.expected_ids, "{}: input_ids", case.name);
        assert_eq!(rendered.marker_pos, case.expected_markers, "{}: marker_pos", case.name);
    }
}

#[test]
fn calibration_matches_reference() {
    let fixture = fixture();

    for case in &fixture.calibration.temperature {
        let config = LayaConfig {
            max_len: 1024,
            head_max_len: 256,
            temperature: [case.temperature, 1.0, 1.0],
            temperature_by_options: Vec::new(),
        };
        let probabilities = render::calibrated_softmax(&config, CHOICE_QTYPE_INDEX, &case.logits);
        assert_close(&probabilities, &case.expected_probs, 1e-5, &case.name);
    }

    for case in &fixture.calibration.confidence {
        let confidence = render::confidence_from_probs(&case.probs);
        assert!(
            (confidence - case.expected).abs() < 1e-5,
            "{}: confidence {confidence} != {}",
            case.name,
            case.expected
        );
    }

    for case in &fixture.calibration.buckets {
        assert_eq!(
            render::temp_bucket(&case.qtype, case.options),
            case.expected,
            "bucket for {} with {} options",
            case.qtype,
            case.options
        );
    }
}

fn assert_close(actual: &[f32], expected: &[f32], tolerance: f32, name: &str) {
    assert_eq!(actual.len(), expected.len(), "{name}: length");
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert!((actual - expected).abs() < tolerance, "{name}[{index}]: {actual} != {expected}");
    }
}
