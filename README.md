# layassist

A cross-platform Rust desktop **applet**: highlight a question and its candidate
answers as native text (no OCR), and it asks a local **Laya** decision model
which answer is correct, then shows a ranked list with probability colours.

General-purpose ("any text"), Linux/Wayland first.

```
hotkey ─▶ egui overlay ─▶ drag over the question, then each answer ─▶ Enter
                                                                    │
                                          Laya (ONNX Runtime, int8) ◀┘
                                                                    │
                         coloured probability panel + top pick ◀─────┘
```

## Status

Planning / scaffolding. See `PLAN.md`.

## Documentation

| File | Contents |
|---|---|
| `AGENTS.md` | Repo guide for agents: invariants, quality gate, conventions |
| `PLAN.md` | Scope, UX, Rust workspace, quality gate, milestones |
| `DECISIONS.md` | Why Rust, why ONNX/int8, egui, resource policy, etc. |
| `FEASIBILITY.md` | Language analysis, ONNX export feasibility, crates, risks |
| `RESOLVERS.md` | `TextResolver` trait and per-platform backends |
| `LAYA.md` | ONNX contract, checkpoint export, rendering port, calibration |
| `OPEN-QUESTIONS.md` | Every decision still needing clarification |

## Development environment (Nix)

`flake.nix` pins the same nixpkgs revision as `~/projects/nix` and provides:

- `cargo`, `rustc`, `clippy`, `rustfmt`, `rust-analyzer`
- `onnxruntime` (used by `ort` via `ORT_DYLIB_PATH`)
- `at-spi2-core` + `dbus` (accessibility), `wl-clipboard`, `wtype`
- `wayland`, `wayland-protocols`, `libxkbcommon`, `vulkan-loader`, `mesa`, `libGL`
- `pkg-config`, `cmake`, `openssl`
- the AT-SPI bus started on shell entry + the `layassist-atspi` helper

```sh
cd ~/projects/layassist
nix develop
cargo build
```

`.envrc` is provided for direnv.

## Quickstart (planned)

```sh
nix develop
cargo run -p layassist-app          # starts the tray applet, loads the model
# hotkey opens the overlay
```

Hotkey: on Wayland/MangoWC a compositor bind runs `layassist toggle` (Wayland
has no app-level global hotkeys). On Windows/macOS the applet registers it.

## Non-goals (v1)

- OCR / vision (reserved as a future resolver)
- Accessibility-tree resolver (future; v1 uses the native selection buffer only)
- Browser/CDP integration (deferred)
- Multi-answer (checkbox) quizzes (deferred)
- Automatic answering of anything (this only *advises*)
