#!/usr/bin/env python3
"""Download the pinned embedding artifacts. Defaults to Granite 97M Multilingual R2."""
import argparse
import errno
import hashlib
import json
import os
from pathlib import Path
import sys
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
FLORENCE_REVISION = "0b03b6f15a4a211370fb204aee4e7dd48887ea37"
FLORENCE_URL = f"https://huggingface.co/florence-community/Florence-2-base-ft/resolve/{FLORENCE_REVISION}"
FLORENCE_SHA256 = {
    "added_tokens.json": "1d75deda84dfa81fb6c09301f3fed00f9695059568bbb1403a6bf299cd84fc37",
    "config.json": "2a4dfe9885d183da121f239ada734bd700ee6908b650a7a25ab32eb104fe1d7a",
    "generation_config.json": "0251459c49cc358ac033b5d4b8569e61ac22bde5d76be404b97a44cfb33fb12e",
    "merges.txt": "1ce1664773c50f3e0cc8842619a93edc4624525b728b188a9e0be33b7726adc5",
    "model.safetensors": "ab06dea66b16d5e54513256d64854be2194443452fd0d84353b40a278bf87d42",
    "preprocessor_config.json": "1396ec5a0a7adfe1c04fb777b09e8ba753be6dbb5868212ab3c3ef39d91fe031",
    "processor_config.json": "cd0e3bf41a39b1276503fbd273bc03b9afc70d7a15ea681a92a1b4b77f858ee6",
    "special_tokens_map.json": "72ff172dc769bc1551b1b4211628ce3271643bc60379e4da45d85a9be9332c39",
    "tokenizer.json": "3ad7001f773409abe6bba33eac92662611a73d72f459bda2f00d2a221dd31ce4",
    "tokenizer_config.json": "cb5f80bd9afa767bb1bb798ee5f0a79eae45239b8e72506166b4218175a16723",
    "vocab.json": "ed19656ea1707df69134c4af35c8ceda2cc9860bf2c3495026153a133670ab5e",
}


EXTRA_MODELS = {
    "e5-large": (E5_LARGE_URL, "e5-large", {
        name: (name, digest) for name, digest in E5_LARGE_SHA256.items()
    }),
    "granite-311m": (GRANITE_311_URL, "granite-311m-r2", GRANITE_311_FILES),
    "florence-2": (FLORENCE_URL, "florence-2-base-ft", {
        name: (name, digest) for name, digest in FLORENCE_SHA256.items()
    }),
}


def model_artifacts(model, variant):
    if model == "e5-small":
        return E5_BASE, "", {"tokenizer.json": E5_TOKENIZER, "model.onnx": E5_WEIGHTS[variant]}
    if model == "granite-97m":
        return GRANITE_BASE, "granite-97m-r2", {
            "tokenizer.json": GRANITE_TOKENIZER, "model.onnx": GRANITE_WEIGHTS,
        }
    return EXTRA_MODELS[model]


def model_for_index(identity):
    candidates = (
        ("e5-small", "fp32"), ("e5-small", "int8"), ("e5-large", "fp32"),
        ("granite-97m", "fp32"), ("granite-311m", "fp32"),
    )
    for model, variant in candidates:
        _, _, artifacts = model_artifacts(model, variant)
        recipe = "granite-r2-cls-512-v1" if model.startswith("granite-") else "e5-mean-pool-v1"
        files = ("model.onnx", "tokenizer.json", "model.onnx_data")
        if identity == ":".join([recipe, *(artifacts[name][1] for name in files if name in artifacts)]):
            return model, variant
    raise SystemExit(
        "This index requires a model outside the pinned downloads. "
        "Use its original model directory or choose a separate index for a built-in model."
    )


def checksum(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def download(base, upstream, target, expected, progress):
    if target.is_file():
        if checksum(target) == expected:
            return False
        raise SystemExit(f"{target} already exists with a different checksum. Use a new model directory.")
    print(f"Downloading {upstream}", file=progress, flush=True)
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
        try:
            os.link(temporary, target)
        except FileExistsError:
            if target.is_file() and checksum(target) == expected:
                return False
            raise SystemExit(f"{target} already exists with different contents. Use a new model directory.") from None
        except OSError as error:
            if error.errno not in (errno.EOPNOTSUPP, errno.EPERM) or target.exists():
                raise
            os.replace(temporary, target)
        return True
    finally:
        temporary.unlink(missing_ok=True)


def managed_directory(root, subdirectory, model, variant, artifacts):
    directory = root / subdirectory
    if not has_conflicting_artifacts(directory, artifacts):
        return directory
    revision = hashlib.sha256("".join(digest for _, digest in artifacts.values()).encode()).hexdigest()[:16]
    directory = root / "managed" / f"{model}-{variant}-{revision}"
    if has_conflicting_artifacts(directory, artifacts):
        raise SystemExit(f"{directory} contains conflicting model files.")
    return directory


def has_conflicting_artifacts(directory, artifacts):
    for filename, (_, digest) in artifacts.items():
        path = directory / filename
        if (path.exists() or path.is_symlink()) and (not path.is_file() or checksum(path) != digest):
            return True
    if "model.onnx" in artifacts:
        extra_files = {"semfind-model.json", "model.onnx_data"} - artifacts.keys()
        return any((directory / name).exists() for name in extra_files)
    return False


def main():
    default_data = Path(os.getenv("XDG_DATA_HOME", ""))
    if not default_data.is_absolute():
        default_data = Path.home() / ".local" / "share"
    parser = argparse.ArgumentParser(description=__doc__)
    destination = parser.add_mutually_exclusive_group()
    destination.add_argument("--directory", type=Path)
    destination.add_argument("--managed", action="store_true", help="Use Semfind's model storage and keep conflicting artifacts.")
    parser.add_argument("--json", action="store_true", help="Report the model directory and whether files were downloaded as JSON.")
    models = ("granite-97m", "e5-small", *EXTRA_MODELS)
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument("--model", choices=models)
    selection.add_argument("--index-model", help="Download the exact built-in model used by this index identity.")
    parser.add_argument("--variant", choices=E5_WEIGHTS)
    args = parser.parse_args()
    configured_model = os.getenv("SEMFIND_MODEL")
    variant = args.variant or "fp32"
    if args.index_model:
        model, variant = model_for_index(args.index_model)
        if configured_model and configured_model != model:
            parser.error(f"This index uses {model}, but SEMFIND_MODEL selects {configured_model}.")
        if args.variant and args.variant != variant:
            parser.error(f"This index requires the {variant} variant.")
    else:
        model = args.model or configured_model or ("e5-small" if variant == "int8" else "granite-97m")
    if model not in models:
        parser.error(f"Unknown SEMFIND_MODEL: {model}")
    if variant != "fp32" and model != "e5-small":
        parser.error("Only E5 Small is available as INT8 in this downloader")
    base, subdirectory, artifacts = model_artifacts(model, variant)
    root = default_data / "semfind" / "model"
    variable = "SEMFIND_CAPTION_MODEL_DIR" if model == "florence-2" else "SEMFIND_MODEL_DIR"
    if args.managed:
        directory = managed_directory(root, subdirectory, model, variant, artifacts)
    else:
        directory = args.directory or Path(os.getenv(variable) or root / subdirectory)
    directory.mkdir(parents=True, exist_ok=True)
    progress = sys.stderr if args.json else sys.stdout
    downloaded = False
    for filename, (upstream, digest) in artifacts.items():
        downloaded |= download(base, upstream, directory / filename, digest, progress)
    if args.json:
        print(json.dumps({"directory": str(directory.absolute()), "downloaded": downloaded}))
    else:
        print(f"Model ready in {directory}")


if __name__ == "__main__":
    main()
