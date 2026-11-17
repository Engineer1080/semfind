# Verification

| Check | Result |
| --- | --- |
| Rust unit and integration tests, Windows x86-64 | Passed. Covers persistence, ranking, literal matches, filters, saved exclusions, rescans, failed inference rollback, CLI JSON and exit codes. |
| Real E5 inference and PDF extraction, Windows x86-64 | Passed with the upstream FP32 and INT8 artifacts. The smoke test checks a German query against English text, an English paraphrase, a PDF error code, and a COSMIC setting. |
| Quick-start examples | Passed with the checked-in synthetic files. A German question retrieved `data-retention.md`, the brightness query returned `display.brightness`, and `E-0042` returned an exact match. |
| Linux x86-64 | Cross-compiled a release build for glibc 2.39. Rust 1.93 formatting and strict Clippy checks passed for the Linux code and test targets. |
| Dependencies and tracked-source preparation | cargo-deny passed after updating bundled SQLite. pip-audit found no known vulnerabilities in the resolved Python environment. A source-tree Gitleaks scan found no credentials. |
| GitHub Actions | The workflow passed actionlint. The hosted jobs are ready for the first GitHub push. |

Runtime validation on Pop!_OS 24.04 remains pending. This includes Linux filesystem-permission and symlink tests, installation through the documented Pop!_OS commands, and any interaction with COSMIC. The Linux workflow runs the filesystem and real-model tests on Ubuntu 24.04.

`semfind` remains a working name. The [GitHub name check](https://github.com/search?q=semfind+in%3Aname&type=repositories) found several projects with the same name, including [PaperBoardOfficial/semfind](https://github.com/PaperBoardOfficial/semfind), a semantic terminal file search.
