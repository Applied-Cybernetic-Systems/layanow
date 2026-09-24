# Open questions

Decisions live in `DECISIONS.md` as ADRs. This file tracks what is still open.

## Resolved

| # | Question | Outcome |
|---|---|---|
| A1 | "3 resolvers" meaning | Text source = **selection only**; 3 = platform backends (ADR-13) |
| A2 | Wayland capture model | **Click-through** overlay + native selection (ADR-14) |
| A3 | Overlay freeze | **No freeze**; dim over live content (ADR-25) |
| A4 | Default checkpoint | **English fp32**; multilingual via settings (ADR-16 superseded by ADR-36) |
| A5 | MCQ → Laya rendering | `ins` = question, `crit` = letter-labelled answers, `state` = empty/context. **Confirmed** on 20 self-made MCQs (context 8/8, empty 5/12, question-in-state 1/12) (ADR-23/28) |
| A6 | Model acquisition | **Download on first run**, cache + checksums (ADR-17) |
| A7 | Quantization | **fp32** default; **fp16** a clean low-precision option for both checkpoints; **English int8** opt-in with a lower-accuracy warning (19/20 agreement); **multilingual int8 rejected** (9/20, near-uniform). Method is weight-only int8 (`MatMulNBits`), not `quantize_dynamic` (ADR-24/28/36/39) |
| A8 | Results display | Same overlay; click dismisses (ADR-15) |
| A9 | Nothing resolved | Applet popup error (ADR-18) |
| A10 | Hotkey | User-managed (ADR-19) |
| A11 | Tray library | **`tray-icon`** (ADR-20) |
| A12 | Overlay windowing | Per-platform; renderer cross-platform (ADR-20) |
| A14 | Capture mechanism | Overlay keyboard interactivity + **`Tab`-commit of the PRIMARY highlight or a typed entry** (ADR-26 revised by ADR-33); auto-capture is future work |
| B3 | X11 support | Wayland + X11 (ADR-21) |
| C1–C7 | Model defaults | See ADR-27 |
| D1 | v1 platform | Linux; order Linux → Windows → macOS |
| E1 | Git | `github.com/Applied-Cybernetic-Systems/layanow` (public) (ADR-22) |
| E3 | Testing fixtures | **Commit generated golden fixtures** for the rendering/calibration port; replay offline (ADR-29) |
| E4 | Logging | **`tracing` + `tracing-subscriber`**; `LAYANOW_LOG` (fallback `RUST_LOG`), default `warn`, `LAYANOW_DEBUG` shorthand |
| E5 | Config | **TOML at `~/.config/layanow/config.toml`**: `checkpoint`, `confidence_threshold`, `unload` (hot/on-demand) in v1 (ADR-38) |
| E8 | MSRV/edition | **Rust 1.85 / edition 2024** (pinned in `Cargo.toml`/`rust-toolchain.toml`) |
| E10 | Localization | UI is **English-only** (ADR-41) |

## Deferred (decide later)

- [x] **D2. Distribution** — Nix flake package for Linux (done); packaging for Windows/macOS remains.
- [ ] **D3. Autostart** — start at login (systemd user unit / xdg autostart)?
- [ ] **D4. Single instance** — enforce one applet instance?
- [ ] **D5. Updates** — update mechanism, or manual? Model version pinning.
- [ ] **E2. CI** — keep the Nix-based GitHub Actions workflow as-is?
- [ ] **E6. History/privacy** — keep a local decision log? Retention?
- [ ] **E7. Lint strictness** — keep `clippy::pedantic` warn + CI `-D warnings`?
- [ ] **E9. Telemetry** — none (assumed).
