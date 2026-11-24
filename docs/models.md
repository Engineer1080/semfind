# Embedding models and image search

## Choose an embedding model

The CLI and downloader accept these models with `--model` or `SEMFIND_MODEL`. Granite 97M is the default for new indexes.

| Model | `--model` | Default minimum similarity |
| --- | --- | ---: |
| Granite 97M Multilingual R2 | `granite-97m` | 0.80 |
| Granite 311M Multilingual R2 | `granite-311m` | 0.82 |
| E5 Small | `e5-small` | 0.80 |
| E5 Large | `e5-large` | 0.78 |
| EmbeddingGemma 300M FP32 | `embeddinggemma-300m` | 0.30 |
| EmbeddingGemma 300M INT8 | `embeddinggemma-300m-int8` | 0.30 |
| EmbeddingGemma 2 (text and images) | `embeddinggemma-2` | 0.68 for text |
| GTE Multilingual Base FP32 | `gte-multilingual-base` | 0.58 |
| Qwen3 Embedding 0.6B INT8 | `qwen3-embedding-0.6b-int8` | 0.40 |

```bash
export SEMFIND_MODEL=granite-311m
export SEMFIND_INDEX_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/semfind/index-granite-311m"
python scripts/download_model.py
semfind init
semfind 'How do I find files modified today?'
```

E5 Small also has an INT8 variant for compatible CPUs. Download it with `python scripts/download_model.py --model e5-small --variant int8`. Use a separate model and index directory when you want to keep multiple variants.

Select a model in **Manage index → Switch Model** or download it with `semfind-download-model --model NAME` after installing the Debian package. EmbeddingGemma's verified ONNX downloads take about 1.25 GB for FP32 or 330 MB for INT8. Use is subject to the [Gemma Terms of Use](https://ai.google.dev/gemma/terms). [ONNX model and tokenizer](https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX/tree/5090578d9565bb06545b4552f76e6bc2c93e4a66)

GTE uses the [ONNX Community FP32 export](https://huggingface.co/onnx-community/gte-multilingual-base). Qwen3 uses the [Qdrant INT8 export](https://huggingface.co/Qdrant/Qwen3-Embedding-0.6B-onnx), which requires ONNX Runtime 1.23 or later.

Without an explicit model selection, Semfind uses the built-in model recorded for an existing index. Keep the model directory available for the weights and tokenizer used to create the index. A custom model still requires its configured directory.

To replace the current index's model, download the new model and use `reindex`. For example, to switch an existing E5 index to Granite 97M after installing Semfind through APT:

```bash
semfind-download-model --model granite-97m
semfind reindex --model granite-97m
```

`reindex` replaces the vectors for saved passages and records the selected model directory in the same transaction. Switching to or from EmbeddingGemma 2 also rereads images in registered folders. Connect any offline indexed folders before switching. Searches and later scans use that directory by default. Update or unset any old `SEMFIND_MODEL` or `SEMFIND_MODEL_DIR` override in your shell. Use `semfind init --force` to rebuild with a full scan of the home directory and registered folders. Search and incremental indexing reject a model whose embedding identity differs from the recorded identity. To keep both models in use, give each a separate index directory as shown above.

## Change the similarity threshold

Save a different minimum cosine similarity for a built-in model. The setting applies to future searches that use that model. Omit `--model` to select the model recorded for the current index, or Granite 97M for a new index.

```bash
semfind config minimum-similarity 0.82 --model granite-97m
semfind config minimum-similarity --model granite-97m
semfind config minimum-similarity off --model granite-97m
semfind config minimum-similarity --reset --model granite-97m
```

Use a value between `-1` and `1`, or `off` to disable the semantic cutoff. The command prints the saved override or `default` when the model's default applies. Override the saved value for one search with `semfind "query" --minimum-similarity 0.75`. The same flag works with `search --stdin` and custom models. Threshold changes take effect without reindexing. They filter semantic matches, while keyword matches stay eligible.

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

With the default Granite 97M setup, Florence-2 Base FT describes supported JPEG, PNG, and WebP images in English. Semfind uses Florence's `<MORE_DETAILED_CAPTION>` prompt and indexes the descriptions alongside documents with the active embedding model. Download Base (about 470 MB) once to enable captioning in `semfind init`. You can use `--kind image` to limit a search to pictures.

```bash
semfind-download-model --model florence-2 --setup-runtime
semfind init
semfind "Katze auf dem Sofa" --kind image --json
```

From a source checkout, replace `semfind-download-model` with `python3 scripts/download_model.py`. On an existing index, the next scan with captioning regenerates descriptions when the checkpoint or caption recipe changes. To explicitly refresh images in all registered folders, run `semfind reindex --refresh-images`.

Florence-2 Large FT (about 1.54 GB) is optional. It uses the same prompt and can improve photo retrieval at higher indexing cost. Select **Manage index → Image model** in the desktop app, or use these commands.

```bash
semfind-download-model --model florence-2-large --setup-runtime
semfind config caption-model florence-2-large
semfind reindex --refresh-images
```

Use `semfind config caption-model florence-2` to restore Base. Use `--caption-model florence-2-large` for one invocation, `--caption-model-dir` for a local checkpoint, or `semfind index ROOT --caption-images` to caption images in one folder. With a Florence setup, `semfind init --no-caption-images` skips new captions for that run and keeps existing ones. If Florence is unavailable during regular `init`, new images are indexed by filename and path.

## Use EmbeddingGemma 2 for text and images

EmbeddingGemma 2 (about 1.53 GB) embeds documents, app and command descriptions, desktop settings, images, and search queries in one shared space. It loads one encoder for a mixed search. Images have vectors and searchable filenames. Florence is only needed for caption-based indexing with a text model. EmbeddingGemma 2 is subject to the [Gemma Terms of Use](https://ai.google.dev/gemma/terms).

```bash
semfind-download-model --model embeddinggemma-2 --setup-runtime
semfind --model embeddinggemma-2 reindex
semfind "Katze auf dem Sofa" --json
```

On a fresh index, run `semfind --model embeddinggemma-2 init` instead. Direct image indexing is also active for `semfind index ROOT`. To switch back, run `semfind --model granite-97m reindex`. If Florence is set up, the switch restores captions for supported images.

The built-in 0.68 similarity cutoff was calibrated for text. Direct image matches use cosine ranking without a default cutoff. A configured or per-search `--minimum-similarity` applies to both. Both image pipelines accept JPEG, PNG, and WebP up to 16 MiB and 40 million pixels. Other image types and unreadable image contents can be found by filename and path.

## PyTorch runtime

`--setup-runtime` reuses a compatible local PyTorch environment or sets up a CPU runtime in Semfind's user data directory. It requires Python with venv support. The desktop app can run this setup when you choose a vision model.

On a GPU, Florence-2 captions new and changed images in batches of up to eight. Large images can reduce the batch size. A GPU memory shortage splits a batch into smaller groups. The CPU processes one image at a time, including after a GPU failure.

Both Florence-2 and EmbeddingGemma 2 can use PyTorch CUDA or ROCm. Semfind discovers `runtime/florence-cuda` and `runtime/florence-rocm` under its data directory and can fall back to CPU. `--device cpu` forces CPU inference. For EmbeddingGemma 2, `--device migraphx` selects PyTorch ROCm, and `--runtime-info` reports the actual provider. The [ONNX GPU setup guide](gpu.md) applies to the ONNX models.

For NVIDIA with a CUDA 13-capable driver, create a dedicated PyTorch environment.

```bash
torch_runtime="${XDG_DATA_HOME:-$HOME/.local/share}/semfind/runtime/florence-cuda"
python3 -m venv "$torch_runtime"
"$torch_runtime/bin/python" -m pip install 'torch==2.14.1' 'torchvision==0.29.1' --index-url https://download.pytorch.org/whl/cu130
"$torch_runtime/bin/python" -m pip install 'transformers==5.23.0' 'Pillow==13.0.0'
```

For AMD, use a [PyTorch ROCm build](https://pytorch.org/get-started/locally/) in `runtime/florence-rocm` with the same Transformers and Pillow dependencies.
