# GPU setup for text embeddings

## Run inference on an NVIDIA GPU

Check that `nvidia-smi` can see the GPU. The command below installs the user-local ONNX Runtime with CUDA and cuDNN libraries, runs an inference check, and then registers it for automatic discovery. It requires an NVIDIA driver that supports CUDA 13. [ONNX Runtime CUDA requirements](https://onnxruntime.ai/docs/execution-providers/CUDA-ExecutionProvider.html#requirements)

```bash
semfind setup-gpu cuda
semfind init --time
```

With the default `--device auto`, search and indexing first try dedicated text embedding runtimes in `runtime/cuda` and `runtime/migraphx` under the Semfind data directory, in that order. You can symlink either directory to an existing virtual environment. If neither dedicated GPU starts, Semfind uses its configured Python environment and tries CUDA, MIGraphX, then CPU as available. A GPU inference error in automatic mode retries on the CPU. `--time` reports the provider selected when the embedding model starts. Florence-2 uses [separate PyTorch runtimes for captioning](models.md#search-images-by-subject).

Before indexing, interactive `semfind init` offers to install a missing GPU runtime if automatic selection falls back to the CPU and Semfind detects a GPU with a bound NVIDIA or AMD driver. Press Enter to decline. A failed installation also continues on the CPU. Non-interactive runs, an explicit `--python`, and direct selections of CPU, CUDA, or MIGraphX skip the prompt.

When a search or indexing command uses the CPU for text embeddings, Semfind prints a performance tip on stderr once per invocation. On Linux it includes the CUDA and MIGraphX setup commands and links to the prerequisites for each backend. Use `--no-gpu-hint` to suppress it for one invocation, or set `SEMFIND_NO_GPU_HINT=true` for your environment.

Use `--python` to select a specific interpreter for one invocation. `SEMFIND_PYTHON` sets the default interpreter, including the fallback in automatic mode. Use `--device cpu` to force CPU inference or `--device cuda` to require CUDA in the selected interpreter. You can also set `SEMFIND_DEVICE`. The same model and weights can reuse an index on another backend. Tokenization, filesystem scanning, SQLite, and search ranking run on the CPU. On the tested RTX 4080 SUPER, CUDA sped up index building and searches with a running model worker. A cold CUDA session took longer to start than a CPU session. Automatic searches can now reuse a running session across invocations. [CPU and CUDA benchmark](../benchmarks/retrieval/README.md#granite-97m-cpu-and-cuda-results)

## AMD GPUs

Semfind supports AMD MIGraphX experimentally. Install compatible ROCm and MIGraphX system libraries for your GPU and distribution first. The default setup uses AMD's ONNX Runtime wheel for ROCm 7.2.1 with Python 3.12 on x86_64. For a different supported ROCm and Python combination, pass a compatible local AMD wheel using `semfind setup-gpu migraphx --wheel /path/to/onnxruntime_migraphx.whl`. [AMD installation and compatibility](https://rocm.docs.amd.com/projects/radeon-ryzen/en/latest/docs/install/installrad/native_linux/install-onnx.html) · [MIGraphX wheel matrix](https://onnxruntime.ai/docs/execution-providers/MIGraphX-ExecutionProvider.html#requirements)

```bash
semfind setup-gpu migraphx
semfind init --time
```

Both setup commands install the Python runtime for text embeddings under the Semfind data directory. Run them as your regular user. The NVIDIA driver and AMD system libraries are managed by your distribution. If a runtime is already registered, the setup command checks it in place. Florence-2 has its own PyTorch setup. The explicit `--device migraphx` setting reports an error if the provider cannot start or inference fails. With the default `--device auto`, Semfind falls back to the CPU if GPU inference fails. Granite 97M execution and performance on AMD hardware still need verification.
