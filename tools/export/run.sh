#!/usr/bin/env bash
#
# Build-time ONNX export + dynamic int8 quantization for a Laya checkpoint.
#
#   nix develop
#   tools/export/run.sh                 # multilingual (default)
#   LAYASSIST_M1_SUBFOLDER=typed-decisions tools/export/run.sh
#
# Steps: fetch the PyTorch checkpoint + rl_common.py + the MIT export script,
# export fp32 ONNX, prune the unused act head (v1), quantize to dynamic int8,
# then compare int8 against fp32 (ADR-24). Python is build-time only (ADR-9).
#
# Artifacts land in $LAYASSIST_M1_DIR (default target/m1-export), which is
# gitignored.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORK="${LAYASSIST_M1_DIR:-$REPO_ROOT/target/m1-export}"
VENV="$REPO_ROOT/target/m1-venv"
SUBFOLDER="${LAYASSIST_M1_SUBFOLDER:-multilingual}"
MODEL_DIR="$WORK/$SUBFOLDER"
ONNX_DIR="$WORK/onnx-$SUBFOLDER"
HF="https://huggingface.co/convaiinnovations/laya/resolve/main"
EXPORT_SRC="https://raw.githubusercontent.com/receptron/laya/main/export/export_onnx.py"

fetch() { # url destination
    if [ -s "$2" ]; then
        echo "cached  $2"
        return
    fi
    echo "fetch   $1"
    mkdir -p "$(dirname "$2")"
    curl -fSL --retry 3 -o "$2.part" "$1"
    mv "$2.part" "$2"
}

mkdir -p "$MODEL_DIR/encoder" "$MODEL_DIR/tokenizer"
fetch "$HF/$SUBFOLDER/rl_agent_config.json" "$MODEL_DIR/rl_agent_config.json"
fetch "$HF/$SUBFOLDER/encoder/config.json" "$MODEL_DIR/encoder/config.json"
fetch "$HF/$SUBFOLDER/tokenizer/tokenizer_config.json" "$MODEL_DIR/tokenizer/tokenizer_config.json"
fetch "$HF/rl_common.py" "$MODEL_DIR/rl_common.py"
fetch "$EXPORT_SRC" "$WORK/export_onnx.py"
fetch "$HF/$SUBFOLDER/tokenizer/tokenizer.json" "$MODEL_DIR/tokenizer/tokenizer.json"
fetch "$HF/$SUBFOLDER/model.safetensors" "$MODEL_DIR/model.safetensors"

echo "venv    $VENV"
if [ ! -d "$VENV" ]; then
    uv venv --python "$(command -v python3.12)" "$VENV"
fi
set +u
# shellcheck disable=SC1091
source "$VENV/bin/activate"
set -u
export UV_LINK_MODE=copy
uv pip install --quiet --index-url https://download.pytorch.org/whl/cpu torch
uv pip install --quiet -r "$REPO_ROOT/tools/export/requirements.txt"

if [ ! -s "$ONNX_DIR/laya.onnx" ]; then
    echo "export  $SUBFOLDER -> $ONNX_DIR"
    (cd "$WORK" && python export_onnx.py "$SUBFOLDER" "onnx-$SUBFOLDER")
else
    echo "export  cached $ONNX_DIR/laya.onnx"
fi

python "$REPO_ROOT/tools/export/prune_act_head.py" "$ONNX_DIR/laya.onnx" "$ONNX_DIR/laya_logits.onnx"
python "$REPO_ROOT/tools/export/quantize.py" "$ONNX_DIR/laya_logits.onnx" "$ONNX_DIR/laya_int8.onnx"

# A nonzero exit here is the ADR-24 verdict (int8 missed the agreement bar for
# this checkpoint), not a pipeline failure; see DECISIONS.md ADR-28.
echo "verify  int8 vs fp32 (nonzero exit = int8 below the ADR-24 bar)"
python "$REPO_ROOT/tools/export/verify.py" "$MODEL_DIR" "$ONNX_DIR"
