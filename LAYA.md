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
`rl_agent_config.json`. One checkpoint is resident (ADR-11); choice is a settings
toggle (ADR-7). Each checkpoint offers precision variants (`CheckpointSpec` in
`layanow-model::bundle`): English ships fp32 (`receptron/laya-onnx`), fp16 and
weight-only int8 (`inferenceprince`); multilingual ships fp32 (`soyelmismo`) and
fp16 (`mizchi`). fp32 is the default, fp16 is a clean low-precision option, and
English int8 is opt-in **with a lower-accuracy warning** (ADR-28/36/39).

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

The v1 export **prunes the `act_probs` head** (`tools/export/prune_act_head.py`,
ADR-27 C3): it is unused, and its `value_info` breaks ONNX shape inference. The
shipped graph therefore emits `logits` only; `layanow-model` reads just
`logits`.

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
- Special tokens are read from the checkpoint's `tokenizer_config.json`
  (`cls_token`/`sep_token`/`mask_token`/`pad_token`), **not** hardcoded:
  ModernBERT spells them `[CLS]`/`[SEP]`/`[MASK]`/`[PAD]`, mmBERT `<bos>`/
  `<eos>`/`<mask>`/`<pad>` (`tokenizers` does not parse that file, so
  `layanow-model` reads it and falls back to the `[CLS]` spellings).

For our MCQ use: `state` is the optional context the user supplies (the overlay
Context box, ADR-40), serialized as `{"context": <text>}` (empty → `{}`);
`q["ins"] = question`, `crit = {answer_label: answer_text}`.

### Rendering mapping (A5 — decided)

Chosen (ADR-23), closest to Laya's Jev-style training distribution:

- `q["t"] = "choice"`.
- `q["ins"]` = the question text.
- `q["crit"]` = letter-labelled answers: `{"A": ans0, "B": ans1, …}` →
  `"A: <answer>"`.
- `state` = the optional context (empty `{}` by default, ADR-23); the overlay
  Context box supplies it as `{"context": <text>}` (ADR-40). Do not duplicate
  the question into `state`.

**Evaluation (multilingual, 20 neutral self-made MCQs):** with a short
passage in `state` the fp32 model was 8/8 correct; with empty `state`
(state-less trivia) 5/12; with the question duplicated into `state` 1/12.
So keep `state = {}` as the default and put any selected context into `state`
— do not duplicate the question. Alternatives considered: text-as-label
`{ans0: "", …}`, and duplicating the question into `state`.

### Golden tests

`render.rs` is a port, not a re-derivation: it must match `rl_common.py`
byte-for-byte. `tools/golden/gen_render_fixtures.py` (dev-shell only) runs the
reference `build_sequence` / `render_options` / `confidence_from_probs` against a
checkpoint's real tokenizer and writes
`crates/layanow-model/tests/fixtures/render_golden.json`. The committed fixture
records the exact tokenizer inputs, assembled `input_ids`, `marker_pos`, and the
calibration/confidence values; `tests/render_golden.rs` replays it offline
(ADR-29). Regenerate after an intentional change:

```sh
nix develop
source target/m1-venv/bin/activate   # or any env with transformers
tools/golden/gen_render_fixtures.py \
  --tokenizer target/m1-export/onnx-multilingual/tokenizer \
  --rl-common target/m1-export/multilingual/rl_common.py
```

## Quantization policy (A7 — decided)

**Dynamic int8** (`onnxruntime.quantization.quantize_dynamic`) was the intended
default: int8 weights, runtime-quantized activations, no calibration data,
standard for transformer encoders. Keep the fp32 graph as a settings option and
as the fallback if int8 regresses — **it does; see below**.

Acceptance: int8 vs fp32 top-1 agreement ≥ 99% and small JS/KL divergence on a
sample of real inputs; if a labelled sample exists, top-1 accuracy within
~1–2 points of fp32.

**Result (multilingual).** fp32↔PyTorch parity: `max |dlogits| = 8.6e-6`.
Dynamic int8 (default settings) vs fp32 top-1 agreement was **100% on
context-bearing inputs but 50% on state-less inputs (70% overall)** — below the
≥ 99% bar. `per_channel=True` was much worse (27%); MatMul-only 73%. Peak RSS
(ORT session, debug process): fp32 ≈ 2.1 GB, int8 ≈ 0.7 GB. **Therefore the
multilingual checkpoint defaults to fp32** (ADR-28), with int8 an opt-in
setting. The shipped English bundle (`receptron/laya-onnx`) is also fp32 and is
the default; ADR-36 makes int8 opt-in there too.

**Result (shipped graphs, ADR-39).** Re-evaluated the actual community
exports against fp32 on a 20-case self-made MCQ set
(`layanow-model --example precision_parity`): fp16 is effectively exact for both
checkpoints (20/20; max `|p-q| ≤ 1.3e-3`). Weight-only int8
(`inferenceprince/laya-onnx-int8`, `MatMulNBits`) reaches 19/20 for English —
below the ≥ 99% bar, so it is offered only behind a lower-accuracy warning. The
`soyelmismo` multilingual `model-int8.onnx` collapses to near-uniform
probabilities (9/20) in every language tested and is **not registered**.

## Calibration & confidence

- Apply temperature scaling from `laya_config.json` (`temperature` per qtype,
  and `temperature_by_options` keyed by cardinality bucket) before reporting.
  The multilingual checkpoint ships `temperature = [1, 1, 1]` and no buckets,
  i.e. no calibration; English has fitted values (e.g. `choice:3-5` = 1.76).
- Confidence = Jev-style `1 - normalized_entropy(p)`, matching the reference
  `confidence_from_probs`.
- The `act` head (`act_probs`) is available but unused for v1.

## Getting the ONNX files

The known exports are described by the `CheckpointSpec` registry in
`layanow-model` (`bundle.rs`); each spec records the repo and the bundle layout
(ADR-37). The app downloads and digest-verifies only the selected one (ADR-17).

### Ready-made (English, default)
`receptron/laya-onnx` — `laya.onnx` + `laya.onnx.data` (+ `laya_config.json`,
`tokenizer/`), fp32 (default). Lower precision variants:
`inferenceprince/laya-onnx` (fp16) and `inferenceprince/laya-onnx-int8`
(weight-only `MatMulNBits` int8, offered with a lower-accuracy warning).

### Ready-made (multilingual)
`soyelmismo/laya-multilingual-onnx` — `model-fp32.onnx` (mmBERT-base, fp32) +
`rl_agent_config.json` + `tokenizer/` (default). Verified end-to-end
(download → manifest digest → `ort` load → 3-option `choice`). The repo also
ships a selective `model-int8.onnx`, which is **not** in the registry: it was
evaluated and collapses to near-uniform probabilities (ADR-39).
`mizchi/laya-multilingual-onnx` (fp16) is the registered low-precision option;
it is numerically clean but slower on CPUs without native FP16
(~3900 ms/decision).

### Exporting a checkpoint yourself (multilingual / typed-decisions)
Build-time only; Python + torch never shipped. Driven by
`tools/export/run.sh` inside the dev shell (which provides Python 3.12 + `uv`):

```sh
nix develop
tools/export/run.sh                                # multilingual
LAYANOW_M1_SUBFOLDER=typed-decisions tools/export/run.sh
```

The script:

1. fetches the checkpoint subfolder (`multilingual/` or `typed-decisions/`),
   the repo-root `rl_common.py`, and the MIT `export/export_onnx.py`;
2. creates a venv (`requirements.txt`: `transformers>=5`, `safetensors`, `onnx`,
   `onnxruntime`, `onnxscript`; torch CPU wheels) and runs the export script,
   which writes `laya.onnx`, copies `tokenizer/`, writes `laya_config.json`, and
   prints the parity check (`max |dlogits|`);
3. prunes the unused `act_probs` head (`prune_act_head.py`, above);
4. quantizes to dynamic int8 (`quantize.py`, `per_channel=False`);
5. verifies int8 vs fp32 (`verify.py`).

Artifacts land in `$LAYANOW_M1_DIR` (default `target/m1-export/`, gitignored).
Load them in Rust with
`layanow_model::bundle::checkpoint_from_dir("laya-multilingual", dir, Quant::Fp32)`
and `examples/multilingual_spike.rs`.

Feasibility: **confirmed** — the head is encoder-agnostic and mmBERT is a
`modernbert`-family model. Multilingual exports cleanly (parity 8.6e-6). The
only friction is the unused act branch, handled by pruning. See `FEASIBILITY.md`
for numbers and `DECISIONS.md` ADR-28.

## Model registry (`layanow-model`)

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

No SkillsBuild/answer-bank coupling (ADR-12). For the export spike, evaluate on a
small neutral, self-made MCQ sample (and/or the model's own reference parity
check) to confirm int8 accuracy before offering it as an opt-in setting.
