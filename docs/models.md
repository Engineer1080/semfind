# Embedding models and image search

## Choose an embedding model

The CLI and downloader accept `granite-97m`, `granite-311m`, `e5-small`, and `e5-large` with `--model` or `SEMFIND_MODEL`. Granite 97M is the default for new indexes. Compare the models in the [retrieval benchmark](../benchmarks/retrieval/README.md).

```bash
export SEMFIND_MODEL=granite-311m
export SEMFIND_INDEX_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/semfind/index-granite-311m"
python scripts/download_model.py
semfind init
semfind 'How do I find files modified today?'
```

E5 Small also has an INT8 variant for compatible CPUs. Download it with `python scripts/download_model.py --model e5-small --variant int8`. Use a separate model and index directory when you want to keep multiple variants.

Without an explicit model selection, Semfind uses the built-in model recorded for an existing index. Keep the model directory available for the weights and tokenizer used to create the index. A custom model still requires its configured directory.

To replace the current index's model, download the new model and explicitly select it for a full rebuild. For example, to switch an existing E5 index to Granite 97M after installing Semfind through APT:

```bash
semfind-download-model --model granite-97m
semfind init --force --model granite-97m
```

If you set `SEMFIND_MODEL` or `SEMFIND_MODEL_DIR` in your shell, update or unset the old setting before switching. The same rebuild is needed when the weights or tokenizer change. Search and incremental indexing reject a model whose embedding identity differs from the recorded identity. To keep both models in use, give each a separate index directory as shown above.

## Use your own model

Set `--model-dir` or `SEMFIND_MODEL_DIR` to a local directory. Use this setting by itself in place of `--model` and `SEMFIND_MODEL`. Supply `model.onnx`, a Hugging Face `tokenizer.json` with batch padding configured, and a `semfind-model.json` describing the encoder. For an ONNX export with external data, use a single sidecar named `model.onnx_data`.

```json
{
  "pooling": "mean",
  "output": "last_hidden_state",
  "max_tokens": 512,
  "query_prefix": "query: ",
  "passage_prefix": "passage: ",
  "minimum_similarity": 0.80
}
```

`pooling` is required. Use `mean` for attention-masked mean pooling, `cls` for the first token, or `sentence` for an already pooled output of shape `[batch, dimensions]`. The other settings are optional. The default output name is `last_hidden_state`, the token limit is 512, prefixes are empty, and the similarity cutoff is disabled unless specified. The ONNX graph must run on CPU with batched INT64 inputs named `input_ids`, `attention_mask`, or `token_type_ids` and produce one vector of up to 4096 dimensions per input after pooling.

Changes to the weights, tokenizer, or encoding settings require a full rebuild or a separate index. You can adjust the similarity cutoff without reindexing.

## Search images by subject

Once Florence-2 is set up locally, `semfind init` automatically generates English descriptions of supported JPEG, PNG, and WebP files in your home directory. Semfind indexes the captions as text alongside document passages. The same search can return both, while `--kind image` limits it to pictures. Semfind includes the generated description in the result snippet.

From the repository root, add the optional dependencies to the Python environment selected during setup. The [Florence-2 Base FT checkpoint](https://huggingface.co/florence-community/Florence-2-base-ft) download is about 470 MB.

```bash
"$SEMFIND_PYTHON" -m pip install 'torch==2.14.1' 'torchvision==0.29.1' --index-url https://download.pytorch.org/whl/cpu
"$SEMFIND_PYTHON" -m pip install 'transformers==5.23.0' 'Pillow==13.0.0'
"$SEMFIND_PYTHON" scripts/download_model.py --model florence-2
semfind init
semfind "Katze auf dem Sofa" --json
semfind "Katze auf dem Sofa" --kind image --json
```

Florence-2 runs locally and automatically uses a compatible PyTorch GPU when available. Semfind tries dedicated runtimes in `runtime/florence-cuda` and `runtime/florence-rocm` under the Semfind data directory, then checks the configured Python environment. If GPU captioning fails, Semfind retries on the CPU. `--device cpu` forces CPU captioning, and `--python` uses the chosen interpreter for device detection. `semfind init --time` reports the Florence-2 backend separately when it generates captions.

The [GPU setup guide](gpu.md) covers text embeddings. For image captioning on an NVIDIA GPU with a CUDA 13-capable driver, create a dedicated Florence-2 runtime.

```bash
florence_runtime="${XDG_DATA_HOME:-$HOME/.local/share}/semfind/runtime/florence-cuda"
python3 -m venv "$florence_runtime"
"$florence_runtime/bin/python" -m pip install 'torch==2.14.1' 'torchvision==0.29.1' --index-url https://download.pytorch.org/whl/cu130
"$florence_runtime/bin/python" -m pip install 'transformers==5.23.0' 'Pillow==13.0.0'
```

For AMD GPUs, use a [PyTorch ROCm build](https://pytorch.org/get-started/locally/) in `runtime/florence-rocm` and install the same Transformers and Pillow dependencies. The Florence-2 checkpoint is shared between backends.

In a local check with Florence-2 Base FT, the median time to describe one repeated test image after warmup was 3.01 s on a Ryzen 7 5800X3D and 0.15 s on a GeForce RTX 4080 SUPER with CUDA. Both backends produced the same description for that image. The timing excludes model startup and index updates. ROCm still needs hardware verification.

The first run can take longer for large photo collections. Subsequent scans generate captions for new or changed images, or when the caption model changes. If Florence-2 is unavailable during `init`, Semfind keeps the captions it already has and indexes new images by filename and path. Use `semfind init --no-caption-images` to skip captioning for one run.

Use `semfind index ROOT --caption-images` to explicitly caption images in a separate indexed root. Use `SEMFIND_CAPTION_MODEL_DIR` or `--caption-model-dir` for another local checkpoint directory. Deleting an image or unindexing its root removes its caption. Unreadable image contents fall back to filename and path search when the file itself remains accessible. Caption extraction limits images to 16 MiB and 40 million pixels.
