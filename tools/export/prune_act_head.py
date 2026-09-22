"""Drop the unused ``act_probs`` head from an exported Laya graph (v1, ADR-27 C3).

The dynamo-exported act branch carries an inconsistent ``value_info`` that makes
``onnx.shape_inference`` (and therefore ``onnxruntime.quantization``) fail, even
though ONNX Runtime executes the graph. v1 ignores ``act_probs``, so keep only
the nodes needed to produce ``logits``.

Usage: prune_act_head.py <in.onnx> <out.onnx>

Build-time only (ADR-9).
"""

import sys

import onnx


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: prune_act_head.py <in.onnx> <out.onnx>", file=sys.stderr)
        return 2
    source, destination = sys.argv[1], sys.argv[2]
    model = onnx.load(source)

    needed: set[str] = set()

    def visit(name: str) -> None:
        if name in needed:
            return
        needed.add(name)
        for node in model.graph.node:
            if name in node.output:
                for producer_input in node.input:
                    visit(producer_input)

    visit("logits")
    keep = [node for node in model.graph.node if any(out in needed for out in node.output)]

    model.graph.ClearField("node")
    model.graph.node.extend(keep)
    outputs = [output for output in model.graph.output if output.name == "logits"]
    model.graph.ClearField("output")
    model.graph.output.extend(outputs)
    value_info = [value for value in model.graph.value_info if value.name in needed]
    model.graph.ClearField("value_info")
    model.graph.value_info.extend(value_info)

    onnx.save(model, destination)
    print(f"wrote {destination}: {len(keep)} nodes, outputs={[o.name for o in outputs]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
