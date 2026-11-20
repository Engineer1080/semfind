#!/usr/bin/env python3
"""Download the pinned embedding artifacts. Defaults to Granite 97M Multilingual R2."""
import argparse
import hashlib
import os
from pathlib import Path
import tempfile
from urllib.request import urlopen

E5_BASE = "https://huggingface.co/intfloat/multilingual-e5-small/resolve/614241f622f53c4eeff9890bdc4f31cfecc418b3/onnx"
E5_TOKENIZER = (
    "tokenizer.json",
    "0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39",
)
E5_WEIGHTS = {
    "fp32": ("model.onnx", "ca456c06b3a9505ddfd9131408916dd79290368331e7d76bb621f1cba6bc8665"),
    "int8": (
        "model_qint8_avx512_vnni.onnx",
        "dd476dd0c2514e9b9be83aeb3853fac0763e0bdf4a71645407587d77c48a2d88",
    ),
}
GRANITE_BASE = "https://huggingface.co/ibm-granite/granite-embedding-97m-multilingual-r2/resolve/c61e626a6255c490879d0af885078b61929d51f6"
GRANITE_TOKENIZER = (
    "tokenizer.json", "4f2842d568e2724370aec203652a42ac783c7937f8347a1a2cc7506d71f1582f"
)
GRANITE_WEIGHTS = (
    "onnx/model.onnx", "68e592b160673d30250824c1116bc6ab33f70efb22b97c9e1d7ce1e69c1c9d70"
)

E5_LARGE_REVISION = "65606222138ddc487ab5c3bb2e772d8ce06b3ff5"
E5_LARGE_URL = f"https://huggingface.co/intfloat/multilingual-e5-large/resolve/{E5_LARGE_REVISION}/onnx"
E5_LARGE_SHA256 = {
    "model.onnx": "bb5a52503a3ef35247f5b5ae6c473aaae60505dd3ffaef56d7b69e2f84683c05",
    "model.onnx_data": "0cf1883fee81c63819a44e2ba0efa51d4043d9759685a4ebebbde97e0623d15c",
    "tokenizer.json": "62c24cdc13d4c9952d63718d6c9fa4c287974249e16b7ade6d5a85e7bbb75626",
}
GRANITE_311_REVISION = "dba7b0ee9d789f330fecfb85df57699f9e7d9c42"
GRANITE_311_URL = f"https://huggingface.co/ibm-granite/granite-embedding-311m-multilingual-r2/resolve/{GRANITE_311_REVISION}"
GRANITE_311_FILES = {
    "model.onnx": (
        "onnx/model.onnx",
        "75f9f258bf5013f5fe8a4dad61dd0fd16ac0cbaa7a106e3d3f41c2d04a42d541",
    ),
    "tokenizer.json": (
        "tokenizer.json",
        "0087c868b33bad550a78a08d19798cfd7f713cde4f020803b8f51f405503e15f",
    ),
}


EXTRA_MODELS = {
    "e5-large": (E5_LARGE_URL, "e5-large", {
        name: (name, digest) for name, digest in E5_LARGE_SHA256.items()
    }),
    "granite-311m": (GRANITE_311_URL, "granite-311m-r2", GRANITE_311_FILES),
}


def checksum(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def download(base, upstream, target, expected):
    if target.is_file():
        if checksum(target) == expected:
            return
        raise SystemExit(f"{target} already exists with a different checksum. Use a new model directory.")
    print(f"Downloading {upstream}", flush=True)
    with tempfile.NamedTemporaryFile(dir=target.parent, delete=False) as stream:
        temporary = Path(stream.name)
    try:
        with urlopen(f"{base}/{upstream}", timeout=60) as response, temporary.open("wb") as stream:
            digest = hashlib.sha256()
            while block := response.read(1024 * 1024):
                digest.update(block)
                stream.write(block)
        if digest.hexdigest() != expected:
            raise SystemExit(f"SHA-256 mismatch for {upstream}")
        os.replace(temporary, target)
    finally:
        temporary.unlink(missing_ok=True)


def main():
    default_data = Path(os.getenv("XDG_DATA_HOME", ""))
    if not default_data.is_absolute():
        default_data = Path.home() / ".local" / "share"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, default=os.getenv("SEMFIND_MODEL_DIR"))
    models = ("granite-97m", "e5-small", *EXTRA_MODELS)
    parser.add_argument("--model", choices=models)
    parser.add_argument("--variant", choices=E5_WEIGHTS, default="fp32")
    args = parser.parse_args()
    model = args.model or os.getenv("SEMFIND_MODEL") or ("e5-small" if args.variant == "int8" else "granite-97m")
    if model not in models:
        parser.error(f"Unknown SEMFIND_MODEL: {model}")
    if args.variant != "fp32" and model != "e5-small":
        parser.error("Only E5 Small is available as INT8 in this downloader")
    default_directory = default_data / "semfind" / "model"
    if model == "granite-97m":
        default_directory /= "granite-97m-r2"
        base = GRANITE_BASE
        artifacts = {"tokenizer.json": GRANITE_TOKENIZER, "model.onnx": GRANITE_WEIGHTS}
    elif model == "e5-small":
        base = E5_BASE
        artifacts = {"tokenizer.json": E5_TOKENIZER, "model.onnx": E5_WEIGHTS[args.variant]}
    else:
        base, subdirectory, artifacts = EXTRA_MODELS[model]
        default_directory /= subdirectory
    directory = args.directory if args.directory is not None else default_directory
    directory.mkdir(parents=True, exist_ok=True)
    for filename, (upstream, digest) in artifacts.items():
        download(base, upstream, directory / filename, digest)
    print(f"Model ready in {directory}")


if __name__ == "__main__":
    main()
