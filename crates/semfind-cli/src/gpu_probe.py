"""Verify a real GPU inference before registering an ONNX Runtime environment."""

import base64
import sys

import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer  # Check the runtime's embedding dependency.

provider = sys.argv[1]
ort.disable_telemetry_events()
if provider not in ort.get_available_providers():
    sys.exit(f"{provider} is unavailable. Check your GPU driver and runtime libraries.")
if provider == "CUDAExecutionProvider":
    ort.preload_dlls()

# ONNX IR 8 / opset 13, two float32 2×2 inputs and a single MatMul node.
model = base64.b64decode(
    "CAg6ZQoRCgFBCgFCEgFZIgZNYXRNdWwSEXNlbWZpbmQtZ3B1LWNoZWNrWhMKAUESDgoM"
    "CAESCAoCCAIKAggCWhMKAUISDgoMCAESCAoCCAIKAggCYhMKAVkSDgoMCAESCAoCCAIK"
    "AggCQgIQDQ=="
)
options = ort.SessionOptions()
options.intra_op_num_threads = 1
options.inter_op_num_threads = 1
options.add_session_config_entry("session.disable_cpu_ep_fallback", "1")
try:
    session = ort.InferenceSession(model, options, providers=[provider], enable_fallback=False)
    session.disable_fallback()
    if session.get_providers()[0] != provider:
        raise RuntimeError("ONNX Runtime did not activate the requested provider")
    matrix = np.array([[1, 2], [3, 4]], dtype=np.float32)
    output, = session.run(None, {"A": matrix, "B": np.eye(2, dtype=np.float32)})
    if not np.array_equal(matrix, output):
        raise RuntimeError("inference returned an incorrect result")
except Exception as error:
    cause = error.__cause__ or error
    sys.exit(f"{provider} failed the GPU check: {cause}")
