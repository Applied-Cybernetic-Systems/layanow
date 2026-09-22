//! Pure port of Laya's input rendering and calibration helpers (ADR-23).
//!
//! This module mirrors the checkpoint's `rl_common.py` (as re-implemented in
//! `receptron/laya`'s `src/sequence.ts`): the sequence layout, per-option
//! `[MASK]` marker positions, the per-cardinality temperature buckets, and the
//! Jev-style confidence. It performs no I/O and holds no session, so it is
//! unit-tested without the weights.

use crate::LayaConfig;

/// The `qtype` value passed to the graph for a `choice` decision.
pub const QTYPE_CHOICE: i64 = 0;
/// The `qtype` value for a `score` decision (not used in v1).
pub const QTYPE_SCORE: i64 = 1;
/// The `qtype` value for a `noul` (yes/no) decision (not used in v1).
pub const QTYPE_NOUL: i64 = 2;

/// Index of `choice` within [`LayaConfig::temperature`].
pub const CHOICE_QTYPE_INDEX: usize = 0;

/// Tokenizer ids the renderer needs, looked up once per checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecialIds {
    /// `[CLS]`, the leading sequence token.
    pub cls: i64,
    /// `[SEP]`, the segment separator.
    pub sep: i64,
    /// `[MASK]`, placed once per option to mark its decision slot.
    pub mask: i64,
    /// `[PAD]`, used when batching sequences of different lengths.
    pub pad: i64,
    /// The literal mask token text, scrubbed from user text so it cannot inject
    /// an extra marker.
    pub mask_tok: String,
}

/// A rendered sequence and the positions of its option markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedSequence {
    /// `input_ids` for a single example, in layout order.
    pub ids: Vec<i64>,
    /// Positions within [`Self::ids`] of each option's `[MASK]` marker.
    pub marker_pos: Vec<i64>,
}

/// Render letter-labelled `choice` options the way Laya expects.
///
/// Each `(label, text)` becomes `"<label>: <text>"`, or just `"<label>"` when
/// the description is empty. This is `render_options` for `choice`.
#[must_use]
pub fn render_choice_options(criteria: &[(String, String)]) -> Vec<String> {
    criteria
        .iter()
        .map(
            |(label, text)| {
                if text.is_empty() { label.clone() } else { format!("{label}: {text}") }
            },
        )
        .collect()
}

/// Build the model input sequence for one typed question.
///
/// Layout (matching `rl_common.build_sequence`):
///
/// ```text
/// [CLS] choice question: <instructions> [SEP] [MASK] opt0 [MASK] opt1 … [SEP] state [SEP]
/// ```
///
/// `encode` is a fallible tokenizer (`false` special tokens); it is generic over
/// its error type so the renderer stays independent of the model error type.
/// `state` is the already-serialized state text (`"{}"` for an empty state).
pub fn build_sequence<F, E>(
    encode: F,
    ids: &SpecialIds,
    state: &str,
    instructions: &str,
    options: &[String],
    max_len: usize,
    head_max_len: usize,
) -> Result<RenderedSequence, E>
where
    F: Fn(&str) -> Result<Vec<i64>, E>,
{
    let scrub = |text: &str| text.replace(ids.mask_tok.as_str(), " ");

    let head = format!("choice question: {}", scrub(instructions));
    let mut head_ids = encode(&head)?;

    let mut opt_ids: Vec<Vec<i64>> = options
        .iter()
        .map(|option| {
            let mut ids_for_option = vec![ids.mask];
            let mut text_ids = encode(&format!(" {}", scrub(option)))?;
            text_ids.truncate(48);
            ids_for_option.extend(text_ids);
            Ok(ids_for_option)
        })
        .collect::<Result<_, E>>()?;

    let total = |opts: &[Vec<i64>]| opts.iter().map(Vec::len).sum::<usize>();
    let mut opt_budget = head_max_len.saturating_sub(total(&opt_ids));
    if opt_budget < 16 {
        let per = (head_max_len.saturating_sub(16) / opt_ids.len().max(1)).max(4);
        for option in &mut opt_ids {
            option.truncate(per);
        }
        opt_budget = head_max_len.saturating_sub(total(&opt_ids));
    }
    head_ids.truncate(opt_budget.max(8));

    let mut sequence = Vec::with_capacity(max_len);
    sequence.push(ids.cls);
    sequence.extend_from_slice(&head_ids);
    sequence.push(ids.sep);

    let mut marker_pos = Vec::with_capacity(opt_ids.len());
    for option in &opt_ids {
        marker_pos.push(i64::try_from(sequence.len()).unwrap_or(i64::MAX));
        sequence.extend_from_slice(option);
    }
    sequence.push(ids.sep);

    let room = max_len.saturating_sub(sequence.len()).saturating_sub(1);
    let mut state_ids = encode(&scrub(state))?;
    state_ids.truncate(room);
    sequence.extend_from_slice(&state_ids);
    sequence.push(ids.sep);
    sequence.truncate(max_len);

    let marker_pos = marker_pos
        .into_iter()
        .filter(|&pos| usize::try_from(pos).is_ok_and(|pos| pos < max_len))
        .collect();

    Ok(RenderedSequence { ids: sequence, marker_pos })
}

/// Numerically stable softmax.
#[must_use]
pub fn softmax(logits: &[f32]) -> Vec<f32> {
    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut exps: Vec<f32> = logits.iter().map(|&value| (value - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    if sum > 0.0 {
        for value in &mut exps {
            *value /= sum;
        }
    }
    exps
}

/// Jev-style confidence: `1 - normalized_entropy(p)`.
#[must_use]
pub fn confidence_from_probs(probabilities: &[f32]) -> f32 {
    if probabilities.len() < 2 {
        return 1.0;
    }
    let entropy: f32 = probabilities.iter().map(|&p| -p * p.max(1e-12).ln()).sum();
    // Option counts are tiny, so the u16 path is exact and avoids a lossy cast.
    let ln_k = f32::from(u16::try_from(probabilities.len()).unwrap_or(u16::MAX)).ln();
    1.0 - entropy / ln_k
}

/// The per-cardinality temperature bucket key, e.g. `"choice:3-5"`.
#[must_use]
pub fn temp_bucket(qtype_name: &str, options: usize) -> String {
    let size = match options {
        0..=2 => "2",
        3..=5 => "3-5",
        6..=10 => "6-10",
        _ => "11+",
    };
    format!("{qtype_name}:{size}")
}

/// Look up the temperature for a `choice` decision with `options` options,
/// falling back to the per-qtype temperature from the checkpoint config.
#[must_use]
pub fn temperature_for(config: &LayaConfig, qtype_index: usize, options: usize) -> f32 {
    let qtype_name = match qtype_index {
        1 => "score",
        2 => "noul",
        _ => "choice",
    };
    let bucket = temp_bucket(qtype_name, options);
    config.temperature_by_options.iter().find(|(name, _)| name == &bucket).map_or_else(
        || config.temperature.get(qtype_index).copied().unwrap_or(1.0),
        |(_, temperature)| *temperature,
    )
}

/// Apply temperature scaling and softmax to the raw logits of one question.
#[must_use]
pub fn calibrated_softmax(config: &LayaConfig, qtype_index: usize, logits: &[f32]) -> Vec<f32> {
    let temperature = temperature_for(config, qtype_index, logits.len());
    let temperature = if temperature > 0.0 { temperature } else { 1.0 };
    let scaled: Vec<f32> = logits.iter().map(|&value| value / temperature).collect();
    softmax(&scaled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModelError;

    /// A stand-in tokenizer: one id per whitespace-separated word, so sequence
    /// lengths are easy to reason about. Fallible to mirror the real encoder.
    fn encoder() -> impl Fn(&str) -> Result<Vec<i64>, ModelError> {
        |text| {
            text.split_whitespace()
                .map(|word| {
                    i64::try_from(100 + word.len())
                        .map_err(|_| ModelError::UnexpectedOutput("test word too long"))
                })
                .collect()
        }
    }

    fn ids() -> SpecialIds {
        SpecialIds { cls: 1, sep: 2, mask: 3, pad: 0, mask_tok: "[MASK]".to_string() }
    }

    #[test]
    fn render_choice_options_matches_reference() {
        let criteria =
            vec![("A".to_string(), "first".to_string()), ("B".to_string(), String::new())];
        assert_eq!(render_choice_options(&criteria), ["A: first", "B"]);
    }

    #[test]
    fn build_sequence_layout() {
        let options = vec!["a".to_string(), "bb".to_string()];
        let rendered =
            build_sequence(encoder(), &ids(), "hello world", "which one", &options, 512, 192)
                .unwrap();
        assert_eq!(rendered.ids[0], 1);
        assert_eq!(rendered.ids[5], 2);
        assert_eq!(rendered.marker_pos, [6, 8]);
        assert_eq!(rendered.ids[6], 3);
        assert_eq!(rendered.ids[8], 3);
        assert_eq!(&rendered.ids[rendered.ids.len() - 4..], [2, 105, 105, 2]);
    }

    #[test]
    fn build_sequence_truncates_state_to_max_len() {
        let long_state = "w ".repeat(1000);
        let options = vec!["yes".to_string(), "no".to_string()];
        let rendered =
            build_sequence(encoder(), &ids(), &long_state, "is it", &options, 64, 32).unwrap();
        assert_eq!(rendered.ids.len(), 64);
        assert_eq!(rendered.ids[63], 2);
        assert_eq!(rendered.marker_pos.len(), 2);
    }

    #[test]
    fn build_sequence_scrubs_the_mask_token() {
        let options = vec!["yes".to_string(), "no".to_string()];
        let rendered =
            build_sequence(encoder(), &ids(), "state [MASK] here", "x [MASK] y", &options, 128, 64)
                .unwrap();
        let masks = rendered.ids.iter().filter(|&&id| id == 3).count();
        assert_eq!(masks, 2);
    }

    #[test]
    fn temp_bucket_boundaries() {
        for (options, size) in
            [(0, "2"), (2, "2"), (3, "3-5"), (5, "3-5"), (6, "6-10"), (10, "6-10"), (11, "11+")]
        {
            assert_eq!(temp_bucket("choice", options), format!("choice:{size}"));
        }
        assert_eq!(temp_bucket("score", 2), "score:2");
        assert_eq!(temp_bucket("noul", 2), "noul:2");
    }

    #[test]
    fn softmax_and_confidence() {
        let probabilities = softmax(&[1.0, 1.0, 1.0]);
        let sum: f32 = probabilities.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6);
        assert!(confidence_from_probs(&probabilities).abs() < 1e-6);
        assert!((confidence_from_probs(&[1.0, 0.0]) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn calibrated_softmax_uses_the_choice_bucket() {
        let config = LayaConfig {
            max_len: 512,
            head_max_len: 192,
            temperature: [1.0, 1.0, 1.0],
            temperature_by_options: vec![("choice:3-5".to_string(), 2.0)],
        };
        let logits = [2.0, 0.0, 0.0];
        let scaled = calibrated_softmax(&config, CHOICE_QTYPE_INDEX, &logits);
        let unscaled = softmax(&logits);
        // A temperature above 1 flattens the distribution.
        assert!(scaled[0] < unscaled[0]);
        let sum: f32 = scaled.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6);
    }
}
