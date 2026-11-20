# English retrieval benchmark

This benchmark runs the semfind search core with multilingual E5 Small and Large and Granite Embedding 97M and 311M Multilingual R2. All four use official FP32 ONNX weights, the CPU provider, two inference threads, and a 512-token limit. Both E5 models use semfind's normal prefixes and mean pooling. Both Granite models use the publisher's CLS pooling and unprefixed text. All models use the production worker. [E5 Small model card](https://huggingface.co/intfloat/multilingual-e5-small) · [Granite 97M model card](https://huggingface.co/ibm-granite/granite-embedding-97m-multilingual-r2) · [E5 Large model card](https://huggingface.co/intfloat/multilingual-e5-large) · [Granite 311M model card](https://huggingface.co/ibm-granite/granite-embedding-311m-multilingual-r2)

## Run on Linux

Use the Python dependencies from the project setup and put Cargo on `PATH`. From the repository root:

```bash
.venv/bin/python scripts/download_model.py --model e5-small --directory model
.venv/bin/python benchmarks/retrieval/run.py --download-granite --download-e5-large --download-granite-311
```

The runner verifies model files against pinned SHA-256 checksums. The E5 Large download is about 2.26 GB including external weights. Granite 311M requires about 1.28 GB. With no download flags, the runner uses the local model files. It builds the Rust example in release mode and writes `target/retrieval-benchmark-granite-311/summary.md` and `target/retrieval-benchmark-granite-311/results.json`. The JSON includes the raw timings, cutoff sweep, expected results, returned ranks, model identities, and environment details. Each index is built in a temporary directory from the synthetic fixture. See `run.py --help` for alternative paths.

## Method

- The fixture contains synthetic documents and English queries with expected result IDs. The test includes same-language documents, English questions about German, French and Spanish documents, exact filenames and error codes, built-in Linux commands and COSMIC settings, and queries with no relevant result. All models index the same files and built-in catalogs. The benchmark uses Debian-family command examples and omits host-specific application and manpage catalogs.
- The primary comparison uses the original E5 Small cutoff of 0.78 and cutoffs for Granite and E5 Large selected on separate validation queries. The same validation procedure also tunes E5 Small as a control. The sweep tries no cutoff and then 0.20 through 1.00 in increments of 0.02. Each validation query contributes the reciprocal rank of its first correct result or, for a query without a match, one point for returning an empty list. Ties keep the lower cutoff. Query embeddings are reused only during calibration. The control with the cutoff off retains both text and semantic retrieval.
- Relevance uses first-result accuracy, Recall@5, and mean reciprocal rank through rank 10 for queries with a relevant item. The no-match column counts how often semfind returns an empty list when the fixture has no relevant result.
- Index time covers opening and building a fresh index after the model is loaded. Cold query time covers opening an index, starting a fresh Python worker, and searching. It includes startup and model checksum time with warm filesystem caches. Warm query time uses an open index and a running worker. The harness rotates model order, repeats fresh builds three times, measures one cold query per positive group, and runs five warm repetitions of the entire test set.

The corpus and no-match judgments are small and synthetic. Results describe semfind on this workload and hardware. They leave open how the models compare on a full home index or on queries in other languages.

## Measured run on Pop!_OS 24.04

semfind 0.1.5 ran on an AMD Ryzen 7 5800X3D with ONNX Runtime 1.32.0. Each model indexed 75 sources and 255 passages. The test covered 36 English queries with a known match and eight without one.

| Metric | E5 Small at 0.78 | Granite 97M at 0.86 | E5 Large at 0.80 | Granite 311M at 0.88 |
| --- | ---: | ---: | ---: | ---: |
| Correct at rank 1 | 26/36 | 30/36 | 33/36 | 30/36 |
| Recall@5 | 88.9% | 88.9% | 97.2% | 91.7% |
| MRR@10 | 0.804 | 0.856 | 0.944 | 0.866 |
| Empty on no-match queries | 0/8 | 4/8 | 4/8 | 4/8 |
| Fresh index after model load, median | 2.399 s | 2.755 s | 28.758 s | 9.463 s |
| Cold search, median | 1.697 s | 1.724 s | 2.759 s | 4.544 s |
| Warm search, median / p95 | 7.22 / 8.88 ms | 7.88 / 9.71 ms | 68.63 / 82.41 ms | 22.98 / 25.95 ms |
| Model and tokenizer | 464.8 MiB | 396.1 MiB | 2,148.6 MiB | 1,221.2 MiB |

Granite 311M put all five cross-language matches first. Granite 97M and E5 Large each had four. Granite 97M led on Linux commands with eight of eight first-place matches, followed by Granite 311M with seven. E5 Large led on English document queries with 16 of 16 first-place matches, compared with 11 for each Granite model. All models put the expected result first for every literal and desktop settings query.

Granite 311M took about 2.9 times as long as Granite 97M and about a third as long as E5 Large for warm queries. Its cold searches took the longest in this run. Cold search time includes starting a fresh model worker and checksumming its files.

Tuning E5 Small on the same validation split selected 0.82, giving 27/36 at rank 1, 83.3% Recall@5, 0.787 MRR@10, and four empty lists on the eight no-match queries. With the cosine cutoff disabled, Granite 311M put 26/36 correct items first and the other models each put 25/36 first. All four returned results on every no-match query in that control.
