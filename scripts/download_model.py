#!/usr/bin/env python3
"""Explicit download of the pinned multilingual E5 artifacts."""
import argparse
import hashlib
import os
from pathlib import Path
import tempfile
from urllib.request import urlopen

REVISION = "614241f622f53c4eeff9890bdc4f31cfecc418b3"
BASE = f"https://huggingface.co/intfloat/multilingual-e5-small/resolve/{REVISION}/onnx"
VOCABULARY = (
    "tokenizer.json",
    "0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39",
)
MODELS = {
    "fp32": ("model.onnx", "ca456c06b3a9505ddfd9131408916dd79290368331e7d76bb621f1cba6bc8665"),
    "int8": (
        "model_qint8_avx512_vnni.onnx",
        "dd476dd0c2514e9b9be83aeb3853fac0763e0bdf4a71645407587d77c48a2d88",
    ),
}


def checksum(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def download(upstream, target, expected):
    if target.is_file():
        if checksum(target) == expected:
            return
        raise SystemExit(f"{target} already exists with a different checksum. Use a new model directory.")
    print(f"Downloading {upstream}", flush=True)
    with tempfile.NamedTemporaryFile(dir=target.parent, delete=False) as stream:
        temporary = Path(stream.name)
    try:
        with urlopen(f"{BASE}/{upstream}", timeout=60) as response, temporary.open("wb") as stream:
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
    parser.add_argument("--directory", type=Path,
                        default=Path(os.getenv("SEMFIND_MODEL_DIR", default_data / "semfind" / "model")))
    parser.add_argument("--variant", choices=MODELS, default="fp32")
    args = parser.parse_args()
    args.directory.mkdir(parents=True, exist_ok=True)
    upstream, digest = VOCABULARY
    download(upstream, args.directory / upstream, digest)
    upstream, digest = MODELS[args.variant]
    download(upstream, args.directory / "model.onnx", digest)
    print(f"Model ready in {args.directory}")


if __name__ == "__main__":
    main()
