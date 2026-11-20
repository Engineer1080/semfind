"""Benchmark adapter for Granite's official FP32 ONNX model."""
import hashlib
import json
from pathlib import Path
import sys

import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer


def main():
    sys.stdin.reconfigure(encoding="utf-8")
    ort.disable_telemetry_events()
    directory = Path(sys.argv[1])
    weights = directory / "model.onnx"
    vocabulary = directory / "tokenizer.json"
    hashes = []
    for path in (weights, vocabulary):
        with path.open("rb") as stream:
            hashes.append(hashlib.file_digest(stream, "sha256").hexdigest())

    options = ort.SessionOptions()
    options.intra_op_num_threads = 2
    options.inter_op_num_threads = 1
    options.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL
    session = ort.InferenceSession(str(weights), options, providers=["CPUExecutionProvider"])
    tokenizer = Tokenizer.from_file(str(vocabulary))
    tokenizer.enable_truncation(max_length=512)

    print(json.dumps("granite-r2-cls-512-v1:" + ":".join(hashes)))
    for line in sys.stdin:
        _, texts = json.loads(line)
        batch = tokenizer.encode_batch(texts)
        inputs = {
            "input_ids": np.asarray([row.ids for row in batch], dtype=np.int64),
            "attention_mask": np.asarray([row.attention_mask for row in batch], dtype=np.int64),
        }
        hidden, = session.run(["last_hidden_state"], inputs)
        print(json.dumps(hidden[:, 0, :].tolist(), allow_nan=False))


if __name__ == "__main__":
    main()
