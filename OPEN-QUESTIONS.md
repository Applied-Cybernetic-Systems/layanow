# Open questions

Decisions live in `DECISIONS.md` as ADRs. This file tracks what is still open.

## Resolved

| # | Question | Outcome |
|---|---|---|
| A1 | "3 resolvers" meaning | Text source = **selection only**; 3 = platform backends (ADR-13) |
| A2 | Wayland capture model | **Click-through** overlay + native selection (ADR-14) |
| A3 | Overlay freeze | **No freeze**; dim over live content (ADR-25) |
| A4 | Default checkpoint | **English int8**; multilingual via settings (ADR-16) |
| A5 | MCQ → Laya rendering | `ins` = question, `crit` = letter-labelled answers, `state` = empty/context. **M1 confirmed** on 20 self-made MCQs (context 8/8, empty 5/12, question-in-state 1/12) (ADR-23/28) |
| A6 | Model acquisition | **Download on first run**, cache + checksums (ADR-17) |
| A7 | Quantization | **Dynamic int8** default, fp32 fallback/setting. **M1:** multilingual int8 missed the ≥99% agreement bar (70% overall) → multilingual defaults to fp32; int8 opt-in (ADR-24/28) |
| A8 | Results display | Same overlay; click dismisses (ADR-15) |
| A9 | Nothing resolved | Applet popup error (ADR-18) |
| A10 | Hotkey | User-managed (ADR-19) |
| A11 | Tray library | **`tray-icon`** (ADR-20) |
| A12 | Overlay windowing | Per-platform; renderer cross-platform (ADR-20) |
| A14 | Capture mechanism | **Auto-capture** selection changes + overlay keyboard interactivity (ADR-26) |
| B3 | X11 support | Wayland + X11 (ADR-21) |
| C1–C7 | Model defaults | See ADR-27 |
| D1 | v1 platform | Linux; order Linux → Windows → macOS |
| E1 | Git | `github.com/Uiyx/layassist` (private) (ADR-22) |
| E3 | Testing fixtures | **Commit generated golden fixtures** for the rendering/calibration port; replay offline (ADR-29) |

## Deferred (decide later)

- [ ] **D2. Distribution** — Nix flake package for Linux; packaging for Windows/macOS.
- [ ] **D3. Autostart** — start at login (systemd user unit / xdg autostart)?
- [ ] **D4. Single instance** — enforce one applet instance?
- [ ] **D5. Updates** — update mechanism, or manual? Model version pinning.
- [ ] **E2. CI** — keep the Nix-based GitHub Actions workflow as-is?
- [ ] **E4. Logging** — `tracing` target/location/verbosity.
- [ ] **E5. Config** — TOML at `~/.config/layassist/config.toml`; which keys in v1?
- [ ] **E6. History/privacy** — keep a local decision log? Retention?
- [ ] **E7. Lint strictness** — keep `clippy::pedantic` warn + CI `-D warnings`?
- [ ] **E8. MSRV/edition** — Rust 1.85 / edition 2024.
- [ ] **E9. Telemetry** — none (assumed).
- [ ] **E10. Localization** — UI English-only in v1?
