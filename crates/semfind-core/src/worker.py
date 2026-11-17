"""Local E5 inference. The embedding protocol runs over the parent process pipes."""
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
    model, tokenizer_file = (directory / name for name in ("model.onnx", "tokenizer.json"))
    hashes = []
    for path in (model, tokenizer_file):
        with path.open("rb") as stream:
            hashes.append(hashlib.file_digest(stream, "sha256").hexdigest())
    options = ort.SessionOptions()
    options.intra_op_num_threads = 2
    options.inter_op_num_threads = 1
    options.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL
    session = ort.InferenceSession(str(model), options, providers=["CPUExecutionProvider"])
    tokenizer = Tokenizer.from_file(str(tokenizer_file))
    tokenizer.enable_truncation(max_length=512)
    tokenizer.enable_padding(pad_id=tokenizer.token_to_id("<pad>"), pad_token="<pad>")
    inputs = {node.name for node in session.get_inputs()}

    print(json.dumps("e5-mean-pool-v1:" + ":".join(hashes)))
    for line in sys.stdin:
        prefix, texts = json.loads(line)
        batch = tokenizer.encode_batch([prefix + text for text in texts])
        columns = {
            "input_ids": np.asarray([row.ids for row in batch], dtype=np.int64),
            "attention_mask": np.asarray([row.attention_mask for row in batch], dtype=np.int64),
            "token_type_ids": np.asarray([row.type_ids for row in batch], dtype=np.int64),
        }
        hidden, = session.run(["last_hidden_state"], {key: columns[key] for key in inputs})
        mask = columns["attention_mask"][:, :, None]
        pooled = (hidden * mask).sum(axis=1) / mask.sum(axis=1)
        print(json.dumps(pooled.tolist(), allow_nan=False))


if __name__ == "__main__":
    main()
