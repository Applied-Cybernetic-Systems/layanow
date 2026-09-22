"""Dynamic int8 quantization of an exported Laya ONNX graph (ADR-24).

Usage: quantize.py <in.onnx> <out.onnx>

Expects the v1 graph produced by ``prune_act_head.py`` (logits only), so ONNX
shape inference succeeds. Build-time only (ADR-9); Python never runs in the app.
"""

import sys

from onnxruntime.quantization import QuantType, quantize_dynamic


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: quantize.py <in.onnx> <out.onnx>", file=sys.stderr)
        return 2
    source, destination = sys.argv[1], sys.argv[2]
    # per_channel=False is the default and, empirically, the most accurate for
    # this graph (per-channel dropped top-1 agreement sharply; see LAYA.md).
    quantize_dynamic(source, destination, weight_type=QuantType.QInt8, per_channel=False)
    print(f"wrote {destination}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
