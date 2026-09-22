//! Pure results-panel logic: ranked rows, probability colours, and the
//! low-confidence flag.
//!
//! Kept free of `egui` so the presentation rules are unit-tested without a
//! window. The overlay (`app`) turns [`Results`] into widgets.

use layassist_model::RankedAnswer;
use layassist_model::render::option_label;

/// Confidence below which a decision is flagged as low-confidence (ADR-27 C2).
///
/// The app never refuses to answer; it only warns.
pub const DEFAULT_CONFIDENCE_THRESHOLD: f32 = 0.5;

/// One row of the results panel.
#[derive(Debug, Clone, PartialEq)]
pub struct ResultRow {
    /// The option label (`A`, `B`, …) as rendered to the model.
    pub label: String,
    /// The captured answer text.
    pub text: String,
    /// Calibrated probability in `0.0..=1.0`.
    pub probability: f32,
    /// Display colour for the probability bar.
    pub colour: [u8; 3],
    /// Whether this is the top-ranked answer.
    pub is_top: bool,
}

/// The ranked results panel.
#[derive(Debug, Clone, PartialEq)]
pub struct Results {
    /// Rows, highest probability first.
    pub rows: Vec<ResultRow>,
    /// The decision's Jev-style confidence (`1 - normalized entropy`).
    pub confidence: f32,
    /// Whether [`Self::confidence`] is below the threshold.
    pub low_confidence: bool,
}

/// Build the results panel from ranked answers.
#[must_use]
pub fn results(ranked: &[RankedAnswer], threshold: f32) -> Results {
    let confidence = ranked.first().map_or(0.0, |answer| answer.confidence);
    let rows = ranked
        .iter()
        .enumerate()
        .map(|(rank, answer)| ResultRow {
            label: option_label(answer.index),
            text: answer.text.clone(),
            probability: answer.probability,
            colour: probability_colour(answer.probability),
            is_top: rank == 0,
        })
        .collect();
    Results { rows, confidence, low_confidence: confidence < threshold }
}

/// Map a probability to an RGB colour: red at 0, amber at 0.5, green at 1.
///
/// A purely visual cue; the numeric percentage is always shown alongside.
#[must_use]
pub fn probability_colour(probability: f32) -> [u8; 3] {
    let probability = probability.clamp(0.0, 1.0);
    if probability < 0.5 {
        lerp([220, 60, 50], [230, 180, 40], probability * 2.0)
    } else {
        lerp([230, 180, 40], [60, 180, 90], (probability - 0.5) * 2.0)
    }
}

// Endpoints are u8s, so the interpolation stays within 0..=255 and the round +
// clamp is exact; the truncating cast is safe.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn lerp(from: [u8; 3], to: [u8; 3], t: f32) -> [u8; 3] {
    let channel = |from: u8, to: u8| {
        let value = f32::from(from) + (f32::from(to) - f32::from(from)) * t;
        value.round().clamp(0.0, 255.0) as u8
    };
    [channel(from[0], to[0]), channel(from[1], to[1]), channel(from[2], to[2])]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(index: usize, text: &str, probability: f32, confidence: f32) -> RankedAnswer {
        RankedAnswer { index, text: text.to_string(), probability, confidence }
    }

    #[test]
    fn results_keep_order_and_flag_the_top_answer() {
        let ranked = vec![
            answer(1, "Mars", 0.6, 0.4),
            answer(0, "Venus", 0.25, 0.4),
            answer(2, "Jupiter", 0.15, 0.4),
        ];
        let panel = results(&ranked, DEFAULT_CONFIDENCE_THRESHOLD);
        assert_eq!(panel.rows.len(), 3);
        assert_eq!(panel.rows[0].label, "B");
        assert_eq!(panel.rows[0].text, "Mars");
        assert!(panel.rows[0].is_top);
        assert!(!panel.rows[1].is_top);
        assert_eq!(panel.rows[1].label, "A");
        assert!((panel.confidence - 0.4).abs() < f32::EPSILON);
        assert!(panel.low_confidence);
    }

    #[test]
    fn high_confidence_is_not_flagged() {
        let ranked = vec![answer(0, "yes", 0.9, 0.7)];
        let panel = results(&ranked, DEFAULT_CONFIDENCE_THRESHOLD);
        assert!(!panel.low_confidence);
    }

    #[test]
    fn colour_scale_runs_red_to_green() {
        let low = probability_colour(0.0);
        let high = probability_colour(1.0);
        assert!(low[0] > low[1], "low probability should be red-dominant");
        assert!(high[1] > high[0], "high probability should be green-dominant");
        // The green channel rises monotonically from red through amber to green;
        // red peaks at the amber midpoint rather than falling throughout.
        let samples: Vec<[u8; 3]> =
            [0.0, 0.25, 0.5, 0.75, 1.0].iter().map(|&p| probability_colour(p)).collect();
        assert!(samples.windows(2).all(|pair| pair[0][1] <= pair[1][1]));
        let mid = probability_colour(0.5);
        assert!(mid[0] > 200 && mid[1] > 150, "midpoint should be amber");
    }

    #[test]
    fn colour_clamps_out_of_range_probabilities() {
        assert_eq!(probability_colour(-1.0), probability_colour(0.0));
        assert_eq!(probability_colour(2.0), probability_colour(1.0));
    }
}
