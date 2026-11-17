# Dependencies and model provenance

The project uses Apache-2.0, which includes an explicit patent grant. Rust dependencies are resolved in `Cargo.lock`. `cargo deny --locked check` checks their declared licenses, sources, and known RustSec advisories. Bundled SQLite is in the [public domain](https://sqlite.org/copyright.html).

The model is [intfloat/multilingual-e5-small](https://huggingface.co/intfloat/multilingual-e5-small) under MIT. `scripts/download_model.py` downloads ONNX and tokenizer artifacts from a fixed [upstream revision](https://huggingface.co/intfloat/multilingual-e5-small/tree/614241f622f53c4eeff9890bdc4f31cfecc418b3/onnx) and verifies the upstream SHA-256 hashes. The repository and Rust CLI packages ship source code only.

The installed inference runtime uses [ONNX Runtime](https://github.com/microsoft/onnxruntime/blob/main/LICENSE) (MIT), [Hugging Face Tokenizers](https://github.com/huggingface/tokenizers/blob/main/LICENSE) (Apache-2.0), and [NumPy](https://github.com/numpy/numpy/blob/main/LICENSE.txt) (BSD-3-Clause with third-party notices). The Python packages are installed separately with direct versions pinned in `requirements.txt`. Transitive versions resolve at installation and are checked with `pip-audit`; wheel distributions carry their own license notices. For PDF extraction, semfind invokes the system's separately installed [Poppler](https://poppler.freedesktop.org/) program through pipes.

If a release packages a model, Python runtime, or Poppler, review that package's complete set of third-party notices and redistribution obligations. For releases, build from a tagged revision with `Cargo.lock`, record the toolchain and target, and publish SHA-256 sums alongside the artifacts.
