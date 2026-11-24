"""Encode text and images in the same local EmbeddingGemma 2 space."""
import hashlib
import io
import json
import os
from pathlib import Path
import sys
import warnings
from contextlib import redirect_stdout

os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["HF_HUB_DISABLE_TELEMETRY"] = "1"
os.environ["HF_HUB_DISABLE_PROGRESS_BARS"] = "1"

import torch
from PIL import Image, ImageOps
from transformers import AutoConfig, AutoModel, AutoProcessor

MAX_IMAGE_BYTES = 16 * 1024 * 1024
Image.MAX_IMAGE_PIXELS = 40_000_000


def format_text(kind, text):
    if kind == "query: ":
        return "task: search result | query: " + text
    title, separator, body = text.partition("\n")
    return f"title: {title or 'none'} | text: {body}" if separator else f"title: none | text: {title}"


def model_identity(directory):
    names = ("model.safetensors", "tokenizer.json", "config.json", "processor_config.json",
             "preprocessor_config.json", "tokenizer_config.json", "tokenizer.model", "chat_template.jinja")
    hashes = []
    for name in names:
        with (directory / name).open("rb") as stream:
            hashes.append(hashlib.file_digest(stream, "sha256").hexdigest())
    return ":".join(("embeddinggemma2-multimodal-v1", *hashes))


def select_backend(requested):
    gpu = "rocm" if torch.version.hip else "cuda"
    available = torch.cuda.is_available()
    if requested == "auto":
        return gpu if available else "cpu"
    if requested == "cpu" or (requested == gpu and available):
        return requested
    raise RuntimeError(f"EmbeddingGemma 2 could not access a PyTorch {requested.upper()} device")


def read_image(stream, length):
    if not isinstance(length, int) or not 0 <= length <= MAX_IMAGE_BYTES:
        raise ValueError("invalid image length")
    data = stream.read(length)
    if len(data) != length:
        raise ValueError("incomplete image")
    try:
        with warnings.catch_warnings():
            warnings.simplefilter("error", Image.DecompressionBombWarning)
            with Image.open(io.BytesIO(data), formats=("JPEG", "PNG", "WEBP")) as image:
                return ImageOps.exif_transpose(image).convert("RGB")
    except (OSError, ValueError, Image.DecompressionBombError, Image.DecompressionBombWarning):
        return None


def main():
    directory = Path(sys.argv[1])
    requested, _, mode = sys.argv[2].partition("-")
    identity = model_identity(directory)
    torch.set_num_threads(2)
    torch.set_num_interop_threads(1)
    providers = {"cpu": "TorchCPUProvider", "cuda": "TorchCUDAProvider", "rocm": "TorchROCmProvider"}
    backend = select_backend(requested)
    device = "cpu" if backend == "cpu" else "cuda"
    dtype = torch.bfloat16 if backend != "cpu" and torch.cuda.is_bf16_supported() else torch.float32
    with redirect_stdout(sys.stderr):
        config = AutoConfig.from_pretrained(directory, local_files_only=True)
        config.audio_config = None
        processor = AutoProcessor.from_pretrained(directory, local_files_only=True)
        model = AutoModel.from_pretrained(
            directory, config=config, local_files_only=True, use_safetensors=True, dtype=dtype,
        ).to(device).eval()
    print(json.dumps({"identity": identity, "minimum_similarity": 0.68, "provider": providers[backend]}))

    def encode(kind, value):
        if kind == "image":
            if value is None:
                return []
            inputs = processor(text=[processor.image_token], images=[value], return_tensors="pt").to(device)
            inputs["pixel_values"] = inputs["pixel_values"].to(dtype)
        elif kind in ("query: ", "passage: "):
            inputs = processor.tokenizer(
                [format_text(kind, text) for text in value], padding=True, truncation=True,
                max_length=512, return_tensors="pt",
            ).to(device)
        else:
            raise ValueError(f"unknown embedding input: {kind}")
        tokens = model(**inputs).last_hidden_state.float()
        mask = inputs["attention_mask"].unsqueeze(-1)
        return ((tokens * mask).sum(1) / mask.sum(1)).cpu().tolist()

    stream = sys.stdin.buffer
    with torch.inference_mode():
        while line := stream.readline():
            kind, value = json.loads(line)
            if kind == "image":
                value = read_image(stream, value)
            try:
                with redirect_stdout(sys.stderr):
                    vectors = encode(kind, value)
            except RuntimeError:
                if device == "cpu" or (requested != "auto" and mode != "auto"):
                    raise
                print(f"semfind: {providers[backend]} inference failed. Retrying on the CPU.", file=sys.stderr)
                device, dtype = "cpu", torch.float32
                model.to(device=device, dtype=dtype)
                with redirect_stdout(sys.stderr):
                    vectors = encode(kind, value)
            print(json.dumps({"vectors": vectors, "uses_cpu": device == "cpu"}, allow_nan=False))


if __name__ == "__main__":
    main()
