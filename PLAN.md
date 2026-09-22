# PLAN

## Goal

A single Rust desktop applet: the user highlights a **question** and N
**candidate answers** (native text, no OCR), and the applet asks a local **Laya**
decision model which answer is correct, then shows a ranked list with
probability colours.

General-purpose ("any text"), with Linux (Wayland + X11) first.

## Scope

### v1 (Linux)
- Resident **tray applet** (no separate daemon) written in Rust.
- **Laya via ONNX Runtime** (`ort`), **English int8** by default (multilingual
  via settings), **CPU-only**, **hot** resident model (soft budget ≤ 3 GB,
  ADR-11/16).
- Hotkey → full-screen **egui overlay**. The overlay is **click-through** and
  never captures the pointer (ADR-14): the user selects text natively in the
  target app, and each selection becomes an item (first = question, rest =
  answers). `Enter` = decide; results render in the same overlay; a click
  dismisses them (ADR-15).
- Text source: the OS **selection buffer only** via the 3 platform backends
  (ADR-13). Accessibility-tree and OCR resolvers are future.
- Single-answer `choice` only. **Never clobber the clipboard.**

### v2+
- Windows, then macOS.
- Accessibility resolver (clipboard-free reads / region resolution).
- Browser/CDP resolver (deferred).
- Multi-answer; auto-routing across checkpoints.
- OCR resolver.

## Architecture

Rust workspace:

```
layassist/
├── Cargo.toml                # workspace
├── rust-toolchain.toml       # pinned stable
├── crates/
│   ├── layassist-core/       # session model, config, types, error
│   ├── layassist-model/      # ONNX session, tokenizer, rendering, calibration, registry
│   ├── layassist-resolvers/  # TextResolver trait + selection backends
│   ├── layassist-platform/   # cfg-gated: overlay, hotkey, selection backends
│   └── layassist-app/        # egui UI, tray, hotkey wiring, the binary
└── flake.nix
```

Data flow:

```
native selection (click-through overlay) ─▶ TextResolver.resolve_current_selection()
                                        │
                                        ▼
                       ModelRegistry.decide(question, answers)   (ort, int8)
                                        │
                                        ▼
                       ranked probabilities ─▶ egui results panel
```

## Rust quality gate (enforced)

- `rust-toolchain.toml` pins stable; `rustfmt`, `clippy`.
- `Cargo.toml` `[workspace.lints]`: `clippy::all` + selected `pedantic`;
  deny `clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic` in library
  crates.
- `#![forbid(unsafe_code)]` on every crate except `layassist-platform` (FFI),
  where unsafe is isolated and documented.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test`, `cargo doc` with `#![deny(missing_docs)]`.
- `cargo deny` (licenses/advisories/bans) + `cargo audit` in CI.
- Errors: `thiserror` in libraries, `anyhow` only in the binary.
- Golden tests compare tokenizer + rendering output to the reference.
- ADRs for decisions; small PRs.

## Model layer (`layassist-model`)

- **ONNX** graph per checkpoint: inputs `input_ids`, `attention_mask`,
  `marker_pos`, `marker_mask`, `qtype` → outputs `logits`, `act_probs`.
- **Tokenizer**: `tokenizers` crate (same lib as HuggingFace) loading each
  checkpoint's `tokenizer.json`.
- **Rendering**: port of Laya's `build_sequence` / `render_options`
  (`rl_common.py`) — `[CLS] <type> question: <ins> [SEP] [MASK] opt0 … [SEP]
  state [SEP]`, option `[MASK]` positions → `marker_pos`.
- **Calibration**: apply `temperature` / `temperature_by_options` from each
  `laya_config.json`.
- **Registry**: per-checkpoint `{graph, tokenizer, config, quant}`; lazy load +
  LRU; settings toggle (ADR-7). See `LAYA.md`.

## UX (v1)

1. Hotkey → full-screen egui overlay: a dim, **click-through** layer that keeps
   **keyboard interactivity** (ADR-14/25/26).
2. Select text natively in the target app → **auto-captured** as **Question**
   (blue).
3. Select more text → **Answer 1..N** (green).
4. `Enter` → ranked results: `[bar] 72.3%  answer text`, top pick emphasised.
5. A click dismisses the results and returns to the live screen (ADR-15).
   Low confidence flagged.

> No freeze (ADR-25); the pointer is never captured (ADR-14). `Esc` cancels.

## Milestones

| # | Deliverable |
|---|---|
| M0 | Rust workspace + flake + CI quality gate + `ort` smoke test on `receptron/laya-onnx` |
| M1 | **Export spike**: multilingual → ONNX + int8; verify parity vs PyTorch; measure RAM/CPU; settle A5/A7 |
| M2 | `layassist-model`: tokenizer + rendering + calibration port; golden tests |
| M3 | Click-through overlay + item model + results panel (selection resolver stub) |
| M4 | Linux selection backends: Wayland PRIMARY (`wl-clipboard-rs`) + X11 (`x11rb`) |
| M5 | Tray (`tray-icon`) + settings (checkpoint, confidence, colours, hot/unload); `layassist toggle` |
| M6 | Polish: no-clipboard-clobber guarantees, error popup, history, docs |
| M7 | Windows selection backend · M8 macOS selection backend · M9 accessibility resolver · M10 OCR |

## Open questions

All outstanding decisions are tracked in **`OPEN-QUESTIONS.md`**. The v1-blocking
questions are resolved (see `DECISIONS.md` ADR-23…27); remaining items are
distribution/packaging and engineering details, deferred to later milestones.
