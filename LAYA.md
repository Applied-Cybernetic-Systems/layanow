# LAYA integration (ONNX Runtime, Rust)

[Laya](https://github.com/NandhaKishorM/laya) is a non-autoregressive,
**typed-decision** model: a bidirectional encoder plus a from-scratch decision
head. Given a *state* and typed *questions*, it returns calibrated
probabilities. It never generates text.

We run it via **ONNX Runtime** in Rust (`ort`), not the Python `laya` package
(ADR-9). Python is used only at build time to export/quantize.

## Checkpoints

| Name | Encoder | Params | Context | `max_len` / `head_max_len` |
|---|---|---|---|---|
| `laya` (root) | ModernBERT-large | 421M | 512 | 512 / 192 |
| `laya-multilingual` | mmBERT-base (`jhu-clsp/mmBERT-base`) | 322M | 1024 (up to 8192) | 1024 / 256 |
| `laya-typed-decisions` | ModernBERT-large | 421M | 1024 | 1024 / 256 |

Each checkpoint ships its own `encoder/`, `tokenizer/`, `model.safetensors`, and
`rl_agent_config.json`. Default: **int8**, one checkpoint resident (ADR-11);
choice is a settings toggle (ADR-7).

## ONNX graph contract

Inputs:
- `input_ids` `[B,L]` int64
- `attention_mask` `[B,L]` int64
- `marker_pos` `[B,K]` int64 — positions of the per-option `[MASK]` markers
- `marker_mask` `[B,K]` bool — which option slots are valid
- `qtype` `[B]` int64 — `choice=0`, `score=1`, `noul=2`

Outputs:
- `logits` `[B,K]` float32 — **uncalibrated**; masked slots `-1e4`
- `act_probs` `[B,2]` float32

`laya_config.json` (next to the graph) holds `max_len`, `head_max_len`,
`temperature`, `temperature_by_options` for post-hoc calibration.

## Input rendering (port of `rl_common.py`)

Sequence layout:

```
[CLS] <type> question: <ins> [SEP] [MASK] opt0 [MASK] opt1 … [SEP] state [SEP]
```

- `render_options` for `choice`: `"label: description"` per option.
- `noul` is always `["false: …", "true: …"]`, so `p[1] == P(true)`.
- `score` renders `"level N: <criterion>"`.
- `head_max_len` budgets the head + options; the remainder (`max_len`) is the
  state, truncated (right by default; left for conversation prefixes).
- The `[MASK]` token positions become `marker_pos`; valid slots become
  `marker_mask`.

For our MCQ use: `state = {question text}` (or extra context),
`q["ins"] = question`, `crit = {answer_label: answer_text}`.

### Rendering mapping (A5 — decided)

Chosen (ADR-23), closest to Laya's Jev-style training distribution:

- `q["t"] = "choice"`.
- `q["ins"]` = the question text.
- `q["crit"]` = letter-labelled answers: `{"A": ans0, "B": ans1, …}` →
  `"A: <answer>"`.
- `state` = empty `{}` by default; or the question / any extra context the user
  selected.

Keep this behind config and confirm by evaluation in M1. Alternatives considered:
text-as-label `{ans0: "", …}`, and duplicating the question into `state`.

## Quantization policy (A7 — decided)

**Dynamic int8** (`onnxruntime.quantization.quantize_dynamic`) is the default:
int8 weights, runtime-quantized activations, no calibration data, standard for
transformer encoders. Keep the fp32 graph as a settings option and as the
fallback if int8 regresses.

Acceptance: int8 vs fp32 top-1 agreement ≥ 99% and small JS/KL divergence on a
sample of real inputs; if a labelled sample exists, top-1 accuracy within
~1–2 points of fp32.

## Calibration & confidence

- Apply temperature scaling from `laya_config.json` (`temperature` per qtype,
  and `temperature_by_options` keyed by cardinality bucket) before reporting.
- Confidence = Jev-style `1 - normalized_entropy(p)`, matching the reference
  `confidence_from_probs`.
- The `act` head (`act_probs`) is available but unused for v1.

## Getting the ONNX files

### Ready-made (English)
`receptron/laya-onnx` — `laya.onnx` (+ `laya_config.json`, `tokenizer/`).
Alternative: `Mattepiu/laya-onnx` (includes `laya_int8.onnx`).

### Exporting a checkpoint yourself (multilingual / typed-decisions)
Build-time only; Python + torch never shipped.

1. Fetch the checkpoint subfolder (`multilingual/` or `typed-decisions/`) plus
   the repo-root `rl_common.py` (the export script imports it).
2. Environment with `torch`, `transformers>=5`, `safetensors`, `onnx`,
   `onnxruntime`.
3. Run the MIT export script:
   ```sh
   python export_onnx.py <checkpoint_dir> <out_dir>
   ```
   It writes `laya.onnx`, copies `tokenizer/`, and writes `laya_config.json`,
   then prints a parity check (`max |dlogits|`).
4. Quantize to int8 (`onnxruntime.quantization.quantize_dynamic`), then verify
   accuracy did not regress.
5. Drop the result into the model registry directory.

Feasibility: **low difficulty** — the head is encoder-agnostic and mmBERT is a
`modernbert`-family model, the same architecture the English export already
targets. See `FEASIBILITY.md` for evidence and risks.

## Model registry (`layassist-model`)

```rust
struct Checkpoint {
    graph: PathBuf,          // laya.onnx
    tokenizer: PathBuf,      // tokenizer/tokenizer.json
    config: LayaConfig,      // max_len, head_max_len, temperatures
    quant: Quant,            // Int8 | Fp32
}
```

- Lazy load + LRU eviction; "hot" keeps one session resident.
- ORT session options: CPU EP, `intra_op_num_threads` capped, arena tuned.
- `decide(question, answers) -> RankedAnswers` applies calibration + confidence.

## Evaluation

No SkillsBuild/answer-bank coupling (ADR-12). For the M1 spike, evaluate on a
small neutral, self-made MCQ sample (and/or the model's own reference parity
check) to confirm int8 accuracy before defaulting to it.
