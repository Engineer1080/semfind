"""Generate local Florence-2 captions from length-prefixed image bytes."""
import io
import json
import os
import sys
import warnings
from contextlib import redirect_stdout

os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["HF_HUB_DISABLE_TELEMETRY"] = "1"

import torch
from PIL import Image, ImageOps
from transformers import Florence2ForConditionalGeneration, Florence2Processor

TASK = "<MORE_DETAILED_CAPTION>"
MAX_IMAGE_BYTES = 16 * 1024 * 1024
MAX_IMAGE_PIXELS = 40_000_000
MAX_BATCH_SIZE = 8


def decode_image(data):
    try:
        with warnings.catch_warnings():
            warnings.simplefilter("error", Image.DecompressionBombWarning)
            with Image.open(io.BytesIO(data), formats=("JPEG", "PNG", "WEBP")) as original:
                return ImageOps.exif_transpose(original).convert("RGB")
    except (OSError, ValueError, Image.DecompressionBombError, Image.DecompressionBombWarning):
        return None


def generate(images, processor, model):
    inputs = processor(text=[TASK] * len(images), images=images, return_tensors="pt").to(model.device)
    output = model.generate(**inputs, max_new_tokens=80, num_beams=1, do_sample=False)
    pad_id = model.generation_config.pad_token_id
    captions = []
    for tokens, image in zip(output.cpu().tolist(), images, strict=True):
        while tokens and tokens[-1] == pad_id:
            tokens.pop()
        generated = processor.decode(tokens, skip_special_tokens=False)
        result = processor.post_process_generation(generated, task=TASK, image_size=image.size)
        text = " ".join(result[TASK].replace("\x00", " ").split())
        captions.append(text[:512].strip() or None)
    return captions


def generate_with_retry(images, processor, model):
    try:
        return generate(images, processor, model)
    except torch.cuda.OutOfMemoryError:
        if len(images) == 1:
            raise
    # Split only after the failed generation's tensors have been released.
    torch.cuda.empty_cache()
    middle = len(images) // 2
    return (generate_with_retry(images[:middle], processor, model)
            + generate_with_retry(images[middle:], processor, model))


def captions(items, count, processor, model, backend):
    results = [None] * count
    pending = []
    pixels = 0

    def flush():
        images = [image for _, image in pending]
        texts = generate_with_retry(images, processor, model)
        for (index, _), text in zip(pending, texts, strict=True):
            results[index] = text
        pending.clear()

    for index, data in enumerate(items):
        image = decode_image(data)
        if image is None:
            continue
        size = image.width * image.height
        if pending and (backend == "cpu" or pixels + size > MAX_IMAGE_PIXELS):
            flush()
            pixels = 0
        pending.append((index, image))
        pixels += size
    if pending:
        flush()
    return results


def read_images(stream, count):
    for _ in range(count):
        size = stream.read(4)
        if len(size) != 4:
            raise ValueError("incomplete image length")
        length = int.from_bytes(size, "little")
        if length > MAX_IMAGE_BYTES:
            raise ValueError("image exceeds the 16 MiB input limit")
        data = stream.read(length)
        if len(data) != length:
            raise ValueError("incomplete image")
        yield data


def select_backend(requested):
    if requested == "cpu":
        return "cpu"
    backend = "rocm" if torch.version.hip else "cuda"
    if torch.cuda.is_available() and requested in ("auto", backend):
        return backend
    if requested == "auto":
        return "cpu"
    raise RuntimeError(f"Florence-2 could not access a PyTorch {requested.upper()} device")


def main():
    directory, requested = sys.argv[1:3]
    torch.set_num_threads(2)
    torch.set_num_interop_threads(1)
    Image.MAX_IMAGE_PIXELS = MAX_IMAGE_PIXELS
    with redirect_stdout(sys.stderr):
        backend = select_backend(requested)
        processor = Florence2Processor.from_pretrained(directory, local_files_only=True)
        model = Florence2ForConditionalGeneration.from_pretrained(
            directory, local_files_only=True, use_safetensors=True, dtype=torch.float32,
        ).to("cpu" if backend == "cpu" else "cuda").eval()
    print(json.dumps(backend))
    with torch.inference_mode():
        stream = sys.stdin.buffer
        while header := stream.read(1):
            count = header[0]
            if not 1 <= count <= MAX_BATCH_SIZE:
                raise ValueError("invalid image batch size")
            with redirect_stdout(sys.stderr):
                result = captions(read_images(stream, count), count, processor, model, backend)
            print(json.dumps(result))


if __name__ == "__main__":
    main()
