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

TASK = "<DETAILED_CAPTION>"
MAX_IMAGE_BYTES = 16 * 1024 * 1024
MAX_IMAGE_PIXELS = 40_000_000


def caption(data, processor, model):
    try:
        with warnings.catch_warnings():
            warnings.simplefilter("error", Image.DecompressionBombWarning)
            with Image.open(io.BytesIO(data), formats=("JPEG", "PNG", "WEBP")) as original:
                image = ImageOps.exif_transpose(original).convert("RGB")
    except (OSError, ValueError, Image.DecompressionBombError, Image.DecompressionBombWarning):
        return None

    inputs = processor(text=TASK, images=image, return_tensors="pt")
    output = model.generate(**inputs, max_new_tokens=80, num_beams=1, do_sample=False)
    generated = processor.batch_decode(output, skip_special_tokens=False)[0]
    result = processor.post_process_generation(generated, task=TASK, image_size=image.size)
    text = " ".join(result[TASK].replace("\x00", " ").split())
    return text[:512].strip() or None


def main():
    directory = sys.argv[1]
    torch.set_num_threads(2)
    torch.set_num_interop_threads(1)
    Image.MAX_IMAGE_PIXELS = MAX_IMAGE_PIXELS
    with redirect_stdout(sys.stderr):
        processor = Florence2Processor.from_pretrained(directory, local_files_only=True)
        model = Florence2ForConditionalGeneration.from_pretrained(
            directory, local_files_only=True, use_safetensors=True, dtype=torch.float32,
        ).eval()
    print(json.dumps(True))
    with torch.inference_mode():
        while size := sys.stdin.buffer.read(4):
            if len(size) != 4:
                raise ValueError("incomplete image length")
            length = int.from_bytes(size, "little")
            if length > MAX_IMAGE_BYTES:
                raise ValueError("image exceeds the 16 MiB input limit")
            data = sys.stdin.buffer.read(length)
            if len(data) != length:
                raise ValueError("incomplete image")
            with redirect_stdout(sys.stderr):
                result = caption(data, processor, model)
            print(json.dumps(result))


if __name__ == "__main__":
    main()
