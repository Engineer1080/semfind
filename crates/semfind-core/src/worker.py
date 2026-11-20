"""Local embedding inference over the parent process pipes."""
import hashlib
import json
from pathlib import Path
import sys

import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer

GRANITE_97_TOKENIZER = "4f2842d568e2724370aec203652a42ac783c7937f8347a1a2cc7506d71f1582f"


def main():
    sys.stdin.reconfigure(encoding="utf-8")
    ort.disable_telemetry_events()
    directory = Path(sys.argv[1])
    model, tokenizer_file = (directory / name for name in ("model.onnx", "tokenizer.json"))
    artifacts = [model, tokenizer_file]
    external_weights = directory / "model.onnx_data"
    if external_weights.is_file():
        artifacts.append(external_weights)
    hashes = []
    for path in artifacts:
        with path.open("rb") as stream:
            hashes.append(hashlib.file_digest(stream, "sha256").hexdigest())
    granite = hashes[1] == GRANITE_97_TOKENIZER
    recipe = "granite-r2-cls-512-v1" if granite else "e5-mean-pool-v1"
    options = ort.SessionOptions()
    options.intra_op_num_threads = 2
    options.inter_op_num_threads = 1
    options.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL
    session = ort.InferenceSession(str(model), options, providers=["CPUExecutionProvider"])
    tokenizer = Tokenizer.from_file(str(tokenizer_file))
    tokenizer.enable_truncation(max_length=512)
    if not granite:
        tokenizer.enable_padding(pad_id=tokenizer.token_to_id("<pad>"), pad_token="<pad>")
    inputs = {node.name for node in session.get_inputs()}

    print(json.dumps(recipe + ":" + ":".join(hashes)))
    for line in sys.stdin:
        prefix, texts = json.loads(line)
        batch = tokenizer.encode_batch(texts if granite else [prefix + text for text in texts])
        columns = {
            "input_ids": np.asarray([row.ids for row in batch], dtype=np.int64),
            "attention_mask": np.asarray([row.attention_mask for row in batch], dtype=np.int64),
            "token_type_ids": np.asarray([row.type_ids for row in batch], dtype=np.int64),
        }
        hidden, = session.run(["last_hidden_state"], {key: columns[key] for key in inputs})
        if granite:
            pooled = hidden[:, 0, :]
        else:
            mask = columns["attention_mask"][:, :, None]
            pooled = (hidden * mask).sum(axis=1) / mask.sum(axis=1)
        print(json.dumps(pooled.tolist(), allow_nan=False))


if __name__ == "__main__":
    main()
