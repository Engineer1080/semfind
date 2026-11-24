#!/usr/bin/env python3
"""Download the pinned embedding artifacts. Defaults to Granite 97M Multilingual R2."""
import argparse
import errno
import hashlib
import json
import os
import subprocess
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
EMBEDDINGGEMMA_REVISION = "5090578d9565bb06545b4552f76e6bc2c93e4a66"
EMBEDDINGGEMMA_URL = f"https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX/resolve/{EMBEDDINGGEMMA_REVISION}"
EMBEDDINGGEMMA_FILES = {
    "model.onnx": (
        "onnx/model.onnx",
        "ea91fd315a7c152d427d231746f0f811a1ac93beaba656abfdf2b24e091265e4",
    ),
    "model.onnx_data": (
        "onnx/model.onnx_data",
        "ef835ae565d8695236652475903078e8ed794c7c35faf1164d78ec3238e8a88d",
    ),
    "tokenizer.json": (
        "tokenizer.json",
        "4dda02faaf32bc91031dc8c88457ac272b00c1016cc679757d1c441b248b9c47",
    ),
}
EMBEDDINGGEMMA_INT8_FILES = {
    "model.onnx": (
        "onnx/model_quantized.onnx",
        "172efde319fe1542dc41f31be6154910b05b78f7a861c265c4600eec906bd6d8",
    ),
    "model_quantized.onnx_data": (
        "onnx/model_quantized.onnx_data",
        "705626e28e4c23c82ade34566b4197d97f534c12275fa406dfb71e9937d388c0",
    ),
    "tokenizer.json": EMBEDDINGGEMMA_FILES["tokenizer.json"],
}
GTE_URL = "https://huggingface.co/onnx-community/gte-multilingual-base/resolve/2edbf5e672aab465f9ed4c154a8b61791c082c69"
GTE_FILES = {
    "model.onnx": ("onnx/model.onnx", "5b9f03fdc40350a78fa064b4cfb6bf9a229a7c40aa87736f537e3ebd00aa2b86"),
    "tokenizer.json": ("tokenizer.json", "3a56def25aa40facc030ea8b0b87f3688e4b3c39eb8b45d5702b3a1300fe2a20"),
}
QWEN_URL = "https://huggingface.co/Qdrant/Qwen3-Embedding-0.6B-onnx/resolve/af95f2c416ffe9379369ad64f9113e865db6112c"
QWEN_FILES = {
    "model.onnx": (
        "onnx/model_quantized.onnx",
        "b39d0623bfc7d2dfc2dcc0d32152c5c5b8d61e9e74679e8222dcce5cf32486b9",
    ),
    "tokenizer.json": ("tokenizer.json", "def76fb086971c7867b829c23a26261e38d9d74e02139253b38aeb9df8b4b50a"),
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


FLORENCE_LARGE_URL = "https://huggingface.co/florence-community/Florence-2-large-ft/resolve/26b734a54fdfbf9c398351eedfabb7f27fc470b7"
FLORENCE_LARGE_SHA256 = FLORENCE_SHA256 | {
    "config.json": "4f6607fc3fd52dcba9fa6535abcd06c5182df4afd8e3de6f7055cc0d56a38ff7",
    "model.safetensors": "58a76f4be257b0dd03ddd6b16c2bb05f4e1910c550047c8fbc4d85de638c619e",
}
GEMMA2_URL = "https://huggingface.co/google/embeddinggemma-2/resolve/f9972ac22e696a105c482202646979e33e4b5a1e"
GEMMA2_SHA256 = {
    "tokenizer.json": "4d777ef5bdc1aa36227abdfb77c3e49e7b9c892d16e1b6bda41c393504828be4",
    "config.json": "b8f1e9931b57fbc054acdb445c41765d55b0074c58d145fa82839941ad1b5bb3",
    "processor_config.json": "168f6a08522f3ce5dea596d94d003af2fd691742d4f41fe1f9d8cce76bfbf69c",
    "preprocessor_config.json": "ea2ae257e901064abdd98dceb19f2b0da06af600bed15e0f99f5c85c37ee9d78",
    "tokenizer_config.json": "17bd5d6e9364ca49a534e1502076593317c298d4a663623091ed45388f004874",
    "tokenizer.model": "e594c8a90eb08d8bda498ff4747977dc827ae0c3c56b5c0d41a605a22d02ef03",
    "chat_template.jinja": "4b852efc0b9960283e735363331e6f325b33bc74bdbaa076f595bc4e9b94d85e",
    "model.safetensors": "197a32965d4b1105faf060417baa899e193fb73cd401f42ec9295234d5553d79",
}
CAPTION_MODELS = ("florence-2", "florence-2-large")
PYTORCH_MODELS = (*CAPTION_MODELS, "embeddinggemma-2")

EXTRA_MODELS = {
    "e5-large": (E5_LARGE_URL, "e5-large", {
        name: (name, digest) for name, digest in E5_LARGE_SHA256.items()
    }),
    "granite-311m": (GRANITE_311_URL, "granite-311m-r2", GRANITE_311_FILES),
    "embeddinggemma-300m": (EMBEDDINGGEMMA_URL, "embeddinggemma-300m", EMBEDDINGGEMMA_FILES),
    "embeddinggemma-300m-int8": (
        EMBEDDINGGEMMA_URL, "embeddinggemma-300m-int8", EMBEDDINGGEMMA_INT8_FILES
    ),
    "gte-multilingual-base": (GTE_URL, "gte-multilingual-base", GTE_FILES),
    "qwen3-embedding-0.6b-int8": (QWEN_URL, "qwen3-embedding-0.6b-int8", QWEN_FILES),
    "florence-2": (FLORENCE_URL, "florence-2-base-ft", {
        name: (name, digest) for name, digest in FLORENCE_SHA256.items()
    }),
    "florence-2-large": (FLORENCE_LARGE_URL, "florence-2-large-ft", {
        name: (name, digest) for name, digest in FLORENCE_LARGE_SHA256.items()
    }),
    "embeddinggemma-2": (GEMMA2_URL, "embeddinggemma-2", {
        name: (name, digest) for name, digest in GEMMA2_SHA256.items()
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
    gemma2_hashes = (GEMMA2_SHA256["model.safetensors"], *(
        digest for name, digest in GEMMA2_SHA256.items() if name != "model.safetensors"
    ))
    if identity == ":".join(("embeddinggemma2-multimodal-v1", *gemma2_hashes)):
        return "embeddinggemma-2", "fp32"
    candidates = (
        ("e5-small", "fp32", "e5-mean-pool-v1"),
        ("e5-small", "int8", "e5-mean-pool-v1"),
        ("e5-large", "fp32", "e5-mean-pool-v1"),
        ("granite-97m", "fp32", "granite-r2-cls-512-v1"),
        ("granite-311m", "fp32", "granite-r2-cls-512-v1"),
        ("embeddinggemma-300m", "fp32", "embeddinggemma-retrieval-v1"),
        ("embeddinggemma-300m-int8", "fp32", "embeddinggemma-retrieval-v1"),
        ("gte-multilingual-base", "fp32", "gte-cls-512-v1"),
        ("qwen3-embedding-0.6b-int8", "fp32", "qwen3-last-token-512-v1"),
    )
    for model, variant, recipe in candidates:
        _, _, artifacts = model_artifacts(model, variant)
        files = ("model.onnx", "tokenizer.json", "model.onnx_data", "model_quantized.onnx_data")
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
        extra_files = {"semfind-model.json", "model.onnx_data", "model_quantized.onnx_data"} - artifacts.keys()
        return any((directory / name).exists() for name in extra_files)
    return False


def setup_vision_runtime(runtime, progress):
    # Serialize setup so two downloads can share the same environment.
    import fcntl

    runtime.mkdir(parents=True, exist_ok=True)
    ready = (
        "import torch, torchvision, PIL; "
        "from transformers import AutoProcessor, EmbeddingGemma2Model, Florence2ForConditionalGeneration"
    )

    def usable(python):
        return python.is_file() and subprocess.run(
            [python, "-I", "-B", "-c", ready], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
        ).returncode == 0

    with (runtime / "florence-cpu.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        for name in ("florence-cuda", "florence-rocm", "florence-cpu"):
            if usable(runtime / name / "bin/python"):
                return
        if usable(Path(os.getenv("SEMFIND_PYTHON", sys.executable))):
            return
        directory = runtime / "florence-cpu"
        interpreter = directory / "bin/python"
        print("Setting up the local PyTorch CPU runtime", file=progress, flush=True)
        subprocess.run([sys.executable, "-m", "venv", directory], stdout=progress, check=True)
        for packages in (
            ("--index-url", "https://download.pytorch.org/whl/cpu", "torch==2.14.1", "torchvision==0.29.1"),
            ("transformers==5.23.0", "Pillow==13.0.0"),
        ):
            subprocess.run([interpreter, "-m", "pip", "install", *packages], stdout=progress, check=True)
        subprocess.run([interpreter, "-I", "-B", "-c", ready], stdout=progress, check=True)


def main():
    default_data = Path(os.getenv("XDG_DATA_HOME", ""))
    if not default_data.is_absolute():
        default_data = Path.home() / ".local" / "share"
    parser = argparse.ArgumentParser(description=__doc__)
    destination = parser.add_mutually_exclusive_group()
    destination.add_argument("--directory", type=Path)
    destination.add_argument("--managed", action="store_true", help="Use Semfind's model storage and keep conflicting artifacts.")
    parser.add_argument("--json", action="store_true", help="Report the model directory and whether files were downloaded as JSON.")
    parser.add_argument("--setup-runtime", action="store_true", help="Install a user-local PyTorch CPU runtime if needed for vision models.")
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
        parser.error("--variant int8 is for E5 Small. Select other quantized variants with --model.")
    if args.setup_runtime and model not in PYTORCH_MODELS:
        parser.error("--setup-runtime is for Florence-2 and EmbeddingGemma 2")
    base, subdirectory, artifacts = model_artifacts(model, variant)
    root = default_data / "semfind" / "model"
    variable = "SEMFIND_CAPTION_MODEL_DIR" if model in CAPTION_MODELS else "SEMFIND_MODEL_DIR"
    if args.managed:
        directory = managed_directory(root, subdirectory, model, variant, artifacts)
    else:
        directory = args.directory or Path(os.getenv(variable) or root / subdirectory)
    directory.mkdir(parents=True, exist_ok=True)
    progress = sys.stderr if args.json else sys.stdout
    if model in ("embeddinggemma-300m", "embeddinggemma-300m-int8", "embeddinggemma-2"):
        print("EmbeddingGemma is subject to the Gemma Terms of Use: https://ai.google.dev/gemma/terms",
              file=progress, flush=True)
    if args.setup_runtime:
        try:
            setup_vision_runtime(root.parent / "runtime", progress)
        except subprocess.CalledProcessError:
            raise SystemExit("Could not prepare PyTorch. Install Python venv support and retry, or see docs/models.md.") from None
    downloaded = False
    for filename, (upstream, digest) in artifacts.items():
        downloaded |= download(base, upstream, directory / filename, digest, progress)
    if args.json:
        print(json.dumps({"directory": str(directory.absolute()), "downloaded": downloaded}))
    else:
        print(f"Model ready in {directory}")


if __name__ == "__main__":
    main()
