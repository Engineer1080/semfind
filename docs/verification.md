# Verification

| Check | Result |
| --- | --- |
| Rust unit and integration tests, Windows x86-64 | Passed. Covers persistence, ranking, literal matches, filters, saved exclusions, rescans, failed inference rollback, CLI JSON and exit codes. |
| Real E5 inference and PDF extraction, Windows x86-64 | Passed with the upstream FP32 and INT8 artifacts. The smoke test checks a German query against English text, an English paraphrase, a PDF error code, and a COSMIC setting. |
| Quick-start examples | Passed with the checked-in synthetic files. A German question retrieved `data-retention.md`, the brightness query returned `display.brightness`, and `E-0042` returned an exact match. |
| Pop!_OS 24.04, Linux x86-64 | Passed Rust and CLI tests, including real Granite 97M inference, PDF extraction, home and desktop entry indexing, local manuals, and multilingual search. |
| COSMIC plugin, Pop!_OS 24.04 with Pop Launcher 1.2.6 | The packaged service discovered the plugin in a temporary home. A German query returned a Florence-2-captioned photo and a text file. The prefix preserved CLI order. Activation dispatched the expected file, app, and settings arguments to test helpers. |
| NVIDIA CUDA, Pop!_OS 24.04 | Verified automatic selection on an RTX 4080 SUPER and CPU fallback with the GPU hidden. The [CPU and CUDA benchmark](../benchmarks/retrieval/README.md#granite-97m-cpu-and-cuda-results) returned identical ranked results on 44 queries. |
| AMD MIGraphX | Verified provider selection, explicit errors, and CPU retry with a simulated MIGraphX provider. Granite 97M execution and performance on AMD hardware remain unverified. |
| Dependencies and tracked-source preparation | cargo-deny passed after updating bundled SQLite. pip-audit found no known vulnerabilities in the resolved Python environment. A source-tree Gitleaks scan found no credentials. |
| GitHub Actions | The workflow runs Rust checks and real-model tests on Ubuntu 24.04. |

Setup from scratch using the documented package installation commands and activation through the graphical COSMIC window remain pending.

`semfind` remains a working name. The [GitHub name check](https://github.com/search?q=semfind+in%3Aname&type=repositories) found several projects with the same name, including [PaperBoardOfficial/semfind](https://github.com/PaperBoardOfficial/semfind), a semantic terminal file search.
