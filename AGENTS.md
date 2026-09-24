# AGENTS.md

Guidance for agents (and humans) working in this repository.

## What this is

`layanow` is a cross-platform desktop **tray applet** (Rust). The user
highlights a question and its candidate answers as **native text** (no OCR), and
the applet asks a local **Laya** typed-decision model (via **ONNX Runtime**)
which answer is correct, then shows a ranked list with probability colours.

Read `README.md` first, then `PLAN.md` and `DECISIONS.md`. The ADRs are binding.

## Repository layout

```
crates/
  layanow-core/       # platform-agnostic types, config, errors (no I/O)
  layanow-model/      # ONNX session, tokenizer, rendering, calibration, registry
  layanow-resolvers/  # TextResolver trait + selection/accessibility impls
  layanow-platform/   # cfg-gated OS backends; the ONLY crate allowed unsafe
  layanow-app/        # egui UI, tray, hotkey wiring; the `layanow` binary
flake.nix               # dev shell (Rust toolchain, onnxruntime, a11y, Wayland)
```

Docs: `PLAN.md`, `DECISIONS.md` (ADRs), `FEASIBILITY.md`, `RESOLVERS.md`,
`LAYA.md`, `OPEN-QUESTIONS.md`, and the GitHub issue tracker (see below).

## Non-negotiable invariants

1. **Never clobber the user's clipboard** (ADR-5). Read PRIMARY / the a11y tree.
   If a copy is ever unavoidable it must be opt-in and save+restore.
2. **Native text only, no OCR** in v1 (ADR-3). v1 text source is the OS
   **selection buffer only** (ADR-13); the accessibility resolver and OCR are
   reserved future slots.
3. **No Python at runtime** (ADR-9). Python is build-time only, for ONNX
   export/quantization.
4. **No network at runtime** except downloading model files on first use.
5. **Single-answer `choice` only** in v1 (ADR-8). Keep the data model
   list-shaped so multi-answer can be added.
6. **No SkillsBuild coupling** and no bundled answer bank (ADR-12).
7. Default resource profile: **hot, fp32 (English), CPU-only**, documented soft
   budget **≤ 3 GB** (ADR-11/36). No runtime enforcement. Dynamic int8 is an
   opt-in setting (ADR-36, T-114).
8. `unsafe` code only in `layanow-platform`, isolated and documented.

## Work tracking (GitHub Issues)

Work is tracked in the **GitHub issue tracker**
(<https://github.com/Applied-Cybernetic-Systems/layanow/issues>), not in a file:

- **Labels** are `area/<core|model|resolve|overlay|app|platform|ui|docs|ci|build|pkg>`
  plus `deferred`; **milestones** are `M0`–`M10` (see `PLAN.md`).
- The historical `T-###` ids are kept in issue titles (e.g. `T-130: …`) for
  traceability; the issue number is the tracker id. Ids are never reused.
- When you start an issue, assign yourself / comment; when you finish it, close
  it with a short outcome and the commit, as `**Closed:**` used to.
- Record anything you notice as a new issue instead of acting silently.
- Completed work (the old closed log) is archived in `CHANGELOG.md`.

## Quality gate (must pass before "done")

Run inside the dev shell (`nix develop`):

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
cargo deny check
```

`cargo lint` is an alias for the clippy line (see `.cargo/config.toml`).

CI (`.github/workflows/ci.yml`) runs exactly these via `nix develop`.

## Coding conventions

- **Errors:** `thiserror` in libraries, `anyhow` only at the binary boundary.
  Never `unwrap`/`expect`/`panic` in library code (clippy denies them; tests are
  exempted per-crate via `#![cfg_attr(test, allow(...))]`).
- **Docs:** public items are documented (`missing_docs` is denied in CI).
- **Lints:** workspace lints in `Cargo.toml` are authoritative; do not
  `#[allow]` a lint without a comment explaining why.
- **Platform code:** everything OS-specific goes in `layanow-platform` behind
  the traits defined in `layanow-resolvers` / `layanow-model`, so the rest
  stays portable and safe.
- **Concurrency:** inference runs on a dedicated worker thread; communicate via
  channels. Prefer message passing over shared mutable state.
- **Formatting:** `rustfmt` (see `rustfmt.toml`); edition 2024.

## Working with the model

- The ONNX graph contract and per-checkpoint export steps are in `LAYA.md`.
- `ort` loads the system `libonnxruntime` via `ORT_DYLIB_PATH` (set by the dev
  shell). Use the `load-dynamic` feature.
- Rendering must match Laya's `rl_common.py` (`build_sequence` /
  `render_options`). Add golden tests when you implement it; do not "fix" the
  layout by eye.
- Calibration (`temperature`, `temperature_by_options`) comes from each
  checkpoint's `laya_config.json`.

## Adding a resolver or a checkpoint

- **Resolver:** implement `TextResolver` (in `layanow-resolvers`) with the
  platform call in `layanow-platform`; register it in the resolution chain.
  Update `RESOLVERS.md`.
- **Checkpoint:** export + quantize per `LAYA.md`, add a `Checkpoint` entry to
  the registry, and document it. Selection stays a settings toggle.

## When unsure

- Design decisions belong in `DECISIONS.md` as a new ADR (short: decision, why,
  consequence). Add one rather than silently changing behaviour.
- If a requirement is ambiguous, stop and ask; do not guess at UX or data
  handling. The open questions are tracked in `OPEN-QUESTIONS.md` (and summarised
  in `PLAN.md`).
