# FEASIBILITY

## Runtime language: Rust

Chosen in ADR-2. Summary of the analysis:

| Concern | Rust | Go | Python + Qt |
|---|---|---|---|
| Model inference | `ort` (ONNX Runtime) | `gonnx`/purego | `onnxruntime` |
| Tokenizer | `tokenizers` (official HF lib) | purego/pure-Go libs | `tokenizers` |
| Tray | `tray-icon` / `ksni` | `systray` | Qt |
| Transparent Wayland overlay | `egui` + `smithay-client-toolkit` | Gio (hand-rolled) | Qt |
| a11y (AT-SPI / UIA / AX) | `atspi` / `uiautomation` / `axuielement-rs` | godbus + hand-rolled | `pyatspi`/`pywinauto` |
| Global hotkey | `global-hotkey` | OS-specific | OS-specific |
| Idle shell RAM (est.) | ~20–60 MB | ~30–80 MB | ~200–450 MB |
| Single binary | good (cross-compile harder) | best (no cgo) | n/a |

**Why Rust wins here:** the model runtime is identical across languages, so the
deciding factor is the resident shell (overlay + a11y + tray). Rust has mature,
maintained crates for all of it, plus the hardest correctness gate (compiler +
clippy), which matters for agent-authored code. Go is near-tied on resources and
simpler to cross-compile, but would need more hand-rolling for the overlay/a11y.
Python is the easiest to write and the heaviest to run.

**Quality gate:** see `PLAN.md` — `rustfmt`, `clippy -D warnings`, denied
`unwrap`/`expect`/`panic` in libraries, `forbid(unsafe_code)` outside the FFI
crate, `cargo test`, `cargo deny`/`audit`, golden tests, ADRs.

## Model runtime: ONNX (no Python at runtime)

- ONNX exports exist: `receptron/laya-onnx` (English, with config + tokenizer),
  `Mattepiu/laya-onnx` (incl. int8). Export code is MIT (`receptron/laya`,
  `export/export_onnx.py`).
- Documented graph: `input_ids`, `attention_mask`, `marker_pos`, `marker_mask`,
  `qtype` → `logits`, `act_probs`. Parity to PyTorch ≈ 1e-5.
- `ort` loads it in Rust; `ort`'s `load-dynamic` feature uses the system
  `libonnxruntime` (provided by the nix devshell via `ORT_DYLIB_PATH`).

### Exporting other checkpoints (multilingual, typed-decisions) — low difficulty

Evidence:
- The export script is parameterised: `export_onnx.py [model_dir] [out_dir]`,
  and reads `rl_agent_config.json`, `encoder/`, `model.safetensors`,
  `tokenizer/` from `model_dir`.
- `rl_common.build_model(cfg, encoder_dir=…)` uses
  `AutoConfig`/`AutoModel.from_config` and a from-scratch head that depends only
  on `encoder.config.hidden_size` — **encoder-agnostic**.
- The multilingual encoder config is `model_type: "modernbert"`
  (`jhu-clsp/mmBERT-base`, 22 layers, hidden 768, vocab 256 000, RoPE, sliding +
  full attention) — i.e. the **same architecture family** the English export
  already targets.

So exporting multilingual is essentially "run the provided script with a
different `model_dir`", plus an int8 quantization pass and a parity check.
**Risks:** `transformers` v5 is required by the configs; the `torch.export`/
dynamo path and `reference_compile` flag are ModernBERT-specific and must be
verified; int8 must be accuracy-checked. All are covered by the M1 spike.

## Text capture without OCR

The needed primitive is *point/region → accessible node → text*. Per OS:

| Platform | API | Rust crate |
|---|---|---|
| Linux (AT-SPI2) | `Component.getAccessibleAtPoint`, `Text.getSelectionText` | `atspi` (zbus) |
| Windows | `IUIAutomation.ElementFromPoint`, `TextPattern.GetSelection` | `uiautomation` |
| macOS | `AXUIElementCopyElementAtPosition`, `AXSelectedText` | `axuielement-rs` / `accessibility` |

Wayland caveats: no global-hotkey protocol (compositor bind); clipboard is
focus-gated (use `wlr-data-control`/`ext-data-control-v1` via `wl-clipboard-rs`,
or PRIMARY); AT-SPI must be running (devshell starts it).

## UI toolkit: egui over iced

Both are fully cross-platform. **egui/eframe** chosen (ADR-10): more widely
adopted and lighter; immediate-mode custom drawing fits the drag-to-mark overlay;
transparent, undecorated, always-on-top windows are straightforward; idle CPU is
near zero when not repainting. `iced` (retained/Elm-style) is more suited to
complex forms and would be heavier for a transparent overlay.

## Resource budget

- int8 checkpoint runtime ≈ 0.4–0.6 GB; + ORT runtime/tokenizer/shell ≈
  0.6–0.8 GB total → comfortably under the documented 3 GB (ADR-11).
- fp32 also fits (≈1.3–2.1 GB) but with less headroom.
- CPU-only avoids a CUDA context/VRAM. ORT `intra_op_num_threads` is capped in
  config to keep idle/low-load CPU in check.
- Model load is seconds; "hot" keeps it resident, "on-demand" unloads after idle.

## Risks

| Risk | Mitigation |
|---|---|
| Port of Laya rendering/head diverges from reference | Golden tests vs `rl_common.py` / ONNX parity; M0/M2 |
| int8 accuracy regression | Accuracy check in M1; fall back to fp32 multilingual |
| Multilingual ONNX export quirks | M1 spike; configs are modernbert family |
| AT-SPI point mapping under Wayland | Spike; PRIMARY-selection fallback |
| `axuielement-rs`/`uiautomation` maturity | Pin versions, wrap behind `TextResolver` trait |
| `ort`/system libonnxruntime mismatch | `load-dynamic` + `ORT_DYLIB_PATH` from nix; pin ORT version |
