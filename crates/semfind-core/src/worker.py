"""Local embedding inference over the parent process pipes."""
import hashlib
import json
from pathlib import Path
import sys

import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer

GRANITE_97_TOKENIZER = "4f2842d568e2724370aec203652a42ac783c7937f8347a1a2cc7506d71f1582f"
GRANITE_311_TOKENIZER = "0087c868b33bad550a78a08d19798cfd7f713cde4f020803b8f51f405503e15f"
E5_LARGE_TOKENIZER = "62c24cdc13d4c9952d63718d6c9fa4c287974249e16b7ade6d5a85e7bbb75626"
BUILTIN_CUTOFFS = {
    GRANITE_97_TOKENIZER: 0.86,
    GRANITE_311_TOKENIZER: 0.88,
    E5_LARGE_TOKENIZER: 0.80,
}


def custom_recipe(path):
    config = json.loads(path.read_text(encoding="utf-8"))
    recipe = {
        "output": "last_hidden_state",
        "max_tokens": 512,
        "query_prefix": "",
        "passage_prefix": "",
    }
    allowed = recipe.keys() | {"pooling", "minimum_similarity"}
    if not isinstance(config, dict) or config.keys() - allowed:
        raise ValueError("invalid fields in semfind-model.json")
    recipe.update(config)
    cutoff = recipe.pop("minimum_similarity", None)
    if recipe.get("pooling") not in ("cls", "mean", "sentence"):
        raise ValueError("semfind-model.json pooling must be cls, mean or sentence")
    if type(recipe["max_tokens"]) is not int or not 1 <= recipe["max_tokens"] <= 32768:
        raise ValueError("semfind-model.json max_tokens must be an integer from 1 to 32768")
    text_fields = ("output", "query_prefix", "passage_prefix")
    if not all(isinstance(recipe[key], str) for key in text_fields) or not recipe["output"]:
        raise ValueError("semfind-model.json requires string prefixes and a nonempty output name")
    if cutoff is not None and (type(cutoff) not in (int, float) or not -1 <= cutoff <= 1):
        raise ValueError("semfind-model.json minimum_similarity must be between -1 and 1")
    return recipe, cutoff


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
    granite = hashes[1] in (GRANITE_97_TOKENIZER, GRANITE_311_TOKENIZER)
    recipe = "granite-r2-cls-512-v1" if granite else "e5-mean-pool-v1"
    cutoff = BUILTIN_CUTOFFS.get(hashes[1], 0.78)
    config = directory / "semfind-model.json"
    if config.exists():
        encoding, cutoff = custom_recipe(config)
        recipe = "custom-onnx-v1"
        hashes.append(hashlib.sha256(json.dumps(encoding, sort_keys=True).encode()).hexdigest())
    else:
        encoding = {
            "pooling": "cls" if granite else "mean",
            "output": "last_hidden_state",
            "max_tokens": 512,
            "query_prefix": "" if granite else "query: ",
            "passage_prefix": "" if granite else "passage: ",
        }
    options = ort.SessionOptions()
    options.intra_op_num_threads = 2
    options.inter_op_num_threads = 1
    options.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL
    session = ort.InferenceSession(str(model), options, providers=["CPUExecutionProvider"])
    tokenizer = Tokenizer.from_file(str(tokenizer_file))
    tokenizer.enable_truncation(max_length=encoding["max_tokens"])
    if recipe == "e5-mean-pool-v1":
        tokenizer.enable_padding(pad_id=tokenizer.token_to_id("<pad>"), pad_token="<pad>")
    elif tokenizer.padding is None:
        raise ValueError("tokenizer.json must define batch padding")
    inputs = {node.name for node in session.get_inputs()}
    if inputs - {"input_ids", "attention_mask", "token_type_ids"}:
        raise ValueError(f"unsupported ONNX model inputs: {sorted(inputs)}")
    output = encoding["output"]
    if not any(node.name == output for node in session.get_outputs()):
        raise ValueError(f"ONNX model does not provide output {output!r}")

    print(json.dumps({"identity": recipe + ":" + ":".join(hashes), "minimum_similarity": cutoff}))
    pooling = encoding["pooling"]
    for line in sys.stdin:
        prefix, texts = json.loads(line)
        prefix = encoding["query_prefix" if prefix == "query: " else "passage_prefix"]
        batch = tokenizer.encode_batch([prefix + text for text in texts])
        columns = {
            "input_ids": np.asarray([row.ids for row in batch], dtype=np.int64),
            "attention_mask": np.asarray([row.attention_mask for row in batch], dtype=np.int64),
            "token_type_ids": np.asarray([row.type_ids for row in batch], dtype=np.int64),
        }
        hidden, = session.run([output], {key: columns[key] for key in inputs})
        if pooling != "sentence" and hidden.ndim != 3:
            raise ValueError("cls and mean pooling require an ONNX output shaped [batch, tokens, dimensions]")
        if pooling == "sentence":
            pooled = hidden
        elif pooling == "cls":
            pooled = hidden[:, 0, :]
        else:
            mask = columns["attention_mask"][:, :, None]
            pooled = (hidden * mask).sum(axis=1) / mask.sum(axis=1)
        if pooled.ndim != 2 or pooled.shape[0] != len(texts):
            raise ValueError("ONNX model must produce one embedding vector per input")
        print(json.dumps(pooled.tolist(), allow_nan=False))


if __name__ == "__main__":
    main()
