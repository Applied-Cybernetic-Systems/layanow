//! Running a Laya checkpoint through ONNX Runtime.

use std::path::Path;

use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::Tensor;
use serde_json::Value;
use tokenizers::Tokenizer;

use crate::error::ModelError;
use crate::render::{self, CHOICE_QTYPE_INDEX, QTYPE_CHOICE, RenderedSequence, SpecialIds};
use crate::{Checkpoint, LayaConfig, RankedAnswer};

/// Default CPU thread cap (ADR-27, C4).
const DEFAULT_INTRA_THREADS: usize = 4;

/// A loaded Laya checkpoint: ONNX session, tokenizer, and calibration config.
pub struct Decider {
    session: Session,
    tokenizer: Tokenizer,
    ids: SpecialIds,
    config: LayaConfig,
}

impl Decider {
    /// Load the graph and tokenizer described by `checkpoint`.
    pub fn load(checkpoint: &Checkpoint) -> Result<Self, ModelError> {
        let tokenizer = Tokenizer::from_file(&checkpoint.tokenizer)?;
        let ids = special_ids(&tokenizer, &tokenizer_config_path(&checkpoint.tokenizer))?;
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(Box::new)?
            .with_intra_threads(intra_threads())
            .map_err(Box::new)?
            .commit_from_file(&checkpoint.graph)?;
        Ok(Self { session, tokenizer, ids, config: checkpoint.config.clone() })
    }

    /// Run one `choice` decision and return the answers ranked by probability.
    ///
    /// `state` is the already-serialized state text; pass `"{}"` for the empty
    /// state chosen in ADR-23.
    pub fn decide_choice(
        &mut self,
        state: &str,
        instructions: &str,
        answers: &[String],
    ) -> Result<Vec<RankedAnswer>, ModelError> {
        if answers.is_empty() {
            return Err(ModelError::NoAnswers);
        }

        let criteria: Vec<(String, String)> = answers
            .iter()
            .enumerate()
            .map(|(index, answer)| (render::option_label(index), answer.clone()))
            .collect();
        let options = render::render_choice_options(&criteria);

        let rendered = render::build_sequence(
            |text| self.tokenize(text),
            &self.ids,
            state,
            instructions,
            &options,
            usize::try_from(self.config.max_len).unwrap_or(usize::MAX),
            usize::try_from(self.config.head_max_len).unwrap_or(usize::MAX),
        )?;

        let probabilities = self.run_choice(&rendered)?;
        let confidence = render::confidence_from_probs(&probabilities);
        let mut ranked: Vec<RankedAnswer> = answers
            .iter()
            .enumerate()
            .map(|(index, text)| RankedAnswer {
                index,
                text: text.clone(),
                probability: probabilities.get(index).copied().unwrap_or(0.0),
                confidence,
            })
            .collect();
        ranked.sort_by(|left, right| right.probability.total_cmp(&left.probability));
        Ok(ranked)
    }

    fn tokenize(&self, text: &str) -> Result<Vec<i64>, ModelError> {
        let encoding = self.tokenizer.encode(text, false)?;
        Ok(encoding.get_ids().iter().map(|&id| i64::from(id)).collect())
    }

    fn run_choice(&mut self, rendered: &RenderedSequence) -> Result<Vec<f32>, ModelError> {
        let length = rendered.ids.len();
        let markers = rendered.marker_pos.len();

        let input_ids = Tensor::from_array((vec![1usize, length], rendered.ids.clone()))?;
        let attention_mask = Tensor::from_array((vec![1usize, length], vec![1_i64; length]))?;
        let marker_pos = Tensor::from_array((vec![1usize, markers], rendered.marker_pos.clone()))?;
        let marker_mask = Tensor::from_array((vec![1usize, markers], vec![true; markers]))?;
        let qtype = Tensor::from_array((vec![1usize], vec![QTYPE_CHOICE]))?;

        let outputs = self.session.run(ort::inputs![
            "input_ids" => input_ids,
            "attention_mask" => attention_mask,
            "marker_pos" => marker_pos,
            "marker_mask" => marker_mask,
            "qtype" => qtype,
        ])?;

        let Some(value) = outputs.get("logits") else {
            return Err(ModelError::UnexpectedOutput("missing `logits` output"));
        };
        let (_, logits) = value.try_extract_tensor::<f32>()?;
        if logits.len() < markers {
            return Err(ModelError::UnexpectedOutput(
                "`logits` has fewer slots than the number of options",
            ));
        }

        Ok(render::calibrated_softmax(&self.config, CHOICE_QTYPE_INDEX, &logits[..markers]))
    }
}

/// The sibling `tokenizer_config.json` of a `tokenizer.json`.
fn tokenizer_config_path(tokenizer: &Path) -> std::path::PathBuf {
    tokenizer.with_file_name("tokenizer_config.json")
}

/// Resolve the special tokens the renderer needs.
///
/// The role names come from the checkpoint's `tokenizer_config.json`
/// (`cls_token`, `sep_token`, `mask_token`, `pad_token`), because different
/// encoders use different spellings — e.g. ModernBERT's `[CLS]`/`[MASK]` versus
/// mmBERT's `<bos>`/`<mask>`. Falls back to the ModernBERT spellings when no
/// config file is present.
fn special_ids(tokenizer: &Tokenizer, config_path: &Path) -> Result<SpecialIds, ModelError> {
    let config = read_tokenizer_config(config_path)?;
    let config = config.as_ref();
    let cls = token_string(config, "cls_token", "[CLS]");
    let sep = token_string(config, "sep_token", "[SEP]");
    let mask = token_string(config, "mask_token", "[MASK]");
    let pad = token_string(config, "pad_token", "[PAD]");
    Ok(SpecialIds {
        cls: require_id(tokenizer, &cls)?,
        sep: require_id(tokenizer, &sep)?,
        mask: require_id(tokenizer, &mask)?,
        pad: require_id(tokenizer, &pad)?,
        mask_tok: mask,
    })
}

fn read_tokenizer_config(path: &Path) -> Result<Option<Value>, ModelError> {
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    let value = serde_json::from_str(&text)
        .map_err(|source| ModelError::Config { path: path.to_path_buf(), source })?;
    Ok(Some(value))
}

/// A config value may be a bare string or an added-token object with `content`.
fn token_string(config: Option<&Value>, field: &str, fallback: &str) -> String {
    config
        .and_then(|value| value.get(field))
        .and_then(|entry| {
            entry
                .as_str()
                .map(str::to_string)
                .or_else(|| entry.get("content").and_then(Value::as_str).map(str::to_string))
        })
        .unwrap_or_else(|| fallback.to_string())
}

fn require_id(tokenizer: &Tokenizer, token: &str) -> Result<i64, ModelError> {
    tokenizer
        .token_to_id(token)
        .map(i64::from)
        .ok_or_else(|| ModelError::MissingSpecialToken(token.to_string()))
}

fn intra_threads() -> usize {
    non_empty_env("LAYANOW_INTRA_OP_THREADS")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|&threads| threads > 0)
        .unwrap_or(DEFAULT_INTRA_THREADS)
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn token_string_reads_plain_strings_and_objects() {
        let config = json!({
            "cls_token": "[CLS]",
            "mask_token": { "content": "<mask>", "special": true }
        });
        let config = Some(&config);
        assert_eq!(token_string(config, "cls_token", "?"), "[CLS]");
        assert_eq!(token_string(config, "mask_token", "?"), "<mask>");
        assert_eq!(token_string(config, "pad_token", "[PAD]"), "[PAD]");
        assert_eq!(token_string(None, "cls_token", "[CLS]"), "[CLS]");
    }
}
