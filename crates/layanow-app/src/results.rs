//! Pure results-panel logic: ranked rows, probability colours, and the
//! low-confidence flag.
//!
//! Kept free of `egui` so the presentation rules are unit-tested without a
//! window. The overlay turns [`Results`](crate::results::Results) into widgets.

use layanow_model::RankedAnswer;
use layanow_model::render::option_label;
use serde::{Deserialize, Serialize};

/// Confidence below which a decision is flagged as low-confidence (ADR-27 C2).
///
/// The app never refuses to answer; it only warns.
pub const DEFAULT_CONFIDENCE_THRESHOLD: f32 = 0.5;

/// The three anchor colours of the probability ramp.
///
/// A bar's colour lerps `low -> mid -> high` as its probability goes
/// `0 -> 0.5 -> 1`. The default is the gruvbox red/orange/green accents
/// (ADR-32); the settings window lets the user pick their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Palette {
    /// Colour at probability 0 (default gruvbox red).
    pub low: [u8; 3],
    /// Colour at probability 0.5 (default gruvbox orange).
    pub mid: [u8; 3],
    /// Colour at probability 1 (default gruvbox green).
    pub high: [u8; 3],
}

impl Default for Palette {
    fn default() -> Self {
        Self { low: [0xfb, 0x49, 0x34], mid: [0xfe, 0x80, 0x19], high: [0xb8, 0xbb, 0x26] }
    }
}

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

/// Build the results panel from ranked answers, colouring each row with
/// `palette`.
#[must_use]
pub fn results(ranked: &[RankedAnswer], threshold: f32, palette: &Palette) -> Results {
    let confidence = ranked.first().map_or(0.0, |answer| answer.confidence);
    let rows = ranked
        .iter()
        .enumerate()
        .map(|(rank, answer)| ResultRow {
            label: option_label(answer.index),
            text: answer.text.clone(),
            probability: answer.probability,
            colour: probability_colour(answer.probability, palette),
            is_top: rank == 0,
        })
        .collect();
    Results { rows, confidence, low_confidence: confidence < threshold }
}

/// Map a probability to an RGB colour by lerping through `palette`'s three
/// anchors (`low` at 0, `mid` at 0.5, `high` at 1).
///
/// A purely visual cue; the numeric percentage is always shown alongside.
#[must_use]
pub fn probability_colour(probability: f32, palette: &Palette) -> [u8; 3] {
    let probability = probability.clamp(0.0, 1.0);
    if probability < 0.5 {
        lerp(palette.low, palette.mid, probability * 2.0)
    } else {
        lerp(palette.mid, palette.high, (probability - 0.5) * 2.0)
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
        let panel = results(&ranked, DEFAULT_CONFIDENCE_THRESHOLD, &Palette::default());
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
        let panel = results(&ranked, DEFAULT_CONFIDENCE_THRESHOLD, &Palette::default());
        assert!(!panel.low_confidence);
    }

    #[test]
    fn colour_scale_runs_red_to_green() {
        let palette = Palette::default();
        let low = probability_colour(0.0, &palette);
        let high = probability_colour(1.0, &palette);
        assert!(low[0] > low[1], "low probability should be red-dominant");
        assert!(high[1] > high[0], "high probability should be green-dominant");
        // The green channel rises monotonically from red through amber to green;
        // red peaks at the amber midpoint rather than falling throughout.
        let samples: Vec<[u8; 3]> =
            [0.0, 0.25, 0.5, 0.75, 1.0].iter().map(|&p| probability_colour(p, &palette)).collect();
        assert!(samples.windows(2).all(|pair| pair[0][1] <= pair[1][1]));
        let mid = probability_colour(0.5, &palette);
        assert!(mid[0] > 200 && mid[1] > 100, "midpoint should be orange");
    }

    #[test]
    fn colour_clamps_out_of_range_probabilities() {
        let palette = Palette::default();
        assert_eq!(probability_colour(-1.0, &palette), probability_colour(0.0, &palette));
        assert_eq!(probability_colour(2.0, &palette), probability_colour(1.0, &palette));
    }

    #[test]
    fn a_custom_palette_replaces_the_anchors() {
        let palette = Palette { low: [0, 0, 0], mid: [10, 20, 30], high: [255, 255, 255] };
        assert_eq!(probability_colour(0.0, &palette), [0, 0, 0]);
        assert_eq!(probability_colour(0.5, &palette), [10, 20, 30]);
        assert_eq!(probability_colour(1.0, &palette), [255, 255, 255]);
    }
}
