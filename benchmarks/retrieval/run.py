#!/usr/bin/env python3
"""Compare multilingual E5 and Granite R2 models in the semfind FP32 retrieval benchmark."""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import sys
import tempfile
from urllib.request import urlopen


ROOT = Path(__file__).resolve().parents[2]
BENCHMARK = Path(__file__).resolve().parent
GRANITE_REVISION = "c61e626a6255c490879d0af885078b61929d51f6"
GRANITE_URL = f"https://huggingface.co/ibm-granite/granite-embedding-97m-multilingual-r2/resolve/{GRANITE_REVISION}"
E5_SHA256 = {
    "model.onnx": "ca456c06b3a9505ddfd9131408916dd79290368331e7d76bb621f1cba6bc8665",
    "tokenizer.json": "0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39",
}
E5_LARGE_REVISION = "65606222138ddc487ab5c3bb2e772d8ce06b3ff5"
E5_LARGE_URL = f"https://huggingface.co/intfloat/multilingual-e5-large/resolve/{E5_LARGE_REVISION}/onnx"
E5_LARGE_SHA256 = {
    "model.onnx": "bb5a52503a3ef35247f5b5ae6c473aaae60505dd3ffaef56d7b69e2f84683c05",
    "model.onnx_data": "0cf1883fee81c63819a44e2ba0efa51d4043d9759685a4ebebbde97e0623d15c",
    "tokenizer.json": "62c24cdc13d4c9952d63718d6c9fa4c287974249e16b7ade6d5a85e7bbb75626",
}
GRANITE_FILES = {
    "model.onnx": (
        "onnx/model.onnx",
        "68e592b160673d30250824c1116bc6ab33f70efb22b97c9e1d7ce1e69c1c9d70",
    ),
    "tokenizer.json": (
        "tokenizer.json",
        "4f2842d568e2724370aec203652a42ac783c7937f8347a1a2cc7506d71f1582f",
    ),
}
GRANITE_311_REVISION = "dba7b0ee9d789f330fecfb85df57699f9e7d9c42"
GRANITE_311_URL = f"https://huggingface.co/ibm-granite/granite-embedding-311m-multilingual-r2/resolve/{GRANITE_311_REVISION}"
GRANITE_311_FILES = {
    "model.onnx": (
        "onnx/model.onnx",
        "75f9f258bf5013f5fe8a4dad61dd0fd16ac0cbaa7a106e3d3f41c2d04a42d541",
    ),
    "tokenizer.json": (
        "tokenizer.json",
        "0087c868b33bad550a78a08d19798cfd7f713cde4f020803b8f51f405503e15f",
    ),
}


def checksum(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def require_artifact(path, expected, url=None):
    if path.is_file() and checksum(path) == expected:
        return
    if url is None or path.exists():
        raise SystemExit(f"Expected benchmark artifact missing or checksum differs: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as stream:
        temporary = Path(stream.name)
    try:
        print(f"Downloading {path.parent.name}/{path.name} from pinned revision", file=sys.stderr, flush=True)
        with urlopen(url, timeout=90) as response, temporary.open("wb") as stream:
            shutil.copyfileobj(response, stream)
        if checksum(temporary) != expected:
            raise SystemExit(f"Checksum mismatch: {url}")
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def metrics(queries, results):
    positive = [
        (set(case["relevant"]), ranks)
        for case, ranks in zip(queries, results, strict=True)
        if case["relevant"]
    ]
    negative = [
        ranks for case, ranks in zip(queries, results, strict=True) if not case["relevant"]
    ]
    reciprocal_ranks = [
        next((1 / rank for rank, item in enumerate(ranks, 1) if item in expected), 0.0)
        for expected, ranks in positive
    ]
    return {
        "top1": sum(score == 1.0 for score in reciprocal_ranks),
        "positives": len(positive),
        "mrr10": statistics.mean(reciprocal_ranks),
        "recall5": statistics.mean(
            len(expected.intersection(ranks[:5])) / len(expected) for expected, ranks in positive
        ),
        "empty": sum(not ranks for ranks in negative),
        "negatives": len(negative),
    }


def cutoff(value):
    return "off" if value is None else f"{value:.2f}"


def summary(result):
    queries = result["test_queries"]
    positives = sum(bool(case["relevant"]) for case in queries)
    lines = [
        "# semfind retrieval benchmark",
        "",
        f"semfind {result['semfind_version']} on {result['environment']['cpu']} with CPU-only ONNX Runtime.",
        f"{result['document_count']} synthetic documents and the built-in settings and command catalogs.",
        f"{len(queries)} English test queries, including {positives} with a known match and {len(queries) - positives} without one.",
        f"{len(result['validation_queries'])} separate validation queries for choosing cutoffs.",
        "",
        "## Retrieval quality",
        "",
        "| Model and cutoff | Top 1 | Recall@5 | MRR@10 | Empty on no-match queries |",
        "| --- | ---: | ---: | ---: | ---: |",
    ]
    for model in result["models"]:
        profiles = [
            ("evaluated", model["evaluated_cutoff"]),
            ("tuned", model["tuned_cutoff"]),
            ("unfiltered", None),
        ]
        for profile, threshold in profiles:
            score = metrics(queries, model[f"{profile}_ranks"])
            lines.append(
                f"| {model['name']} {profile} ({cutoff(threshold)}) | "
                f"{score['top1']}/{score['positives']} | {score['recall5']:.1%} | "
                f"{score['mrr10']:.3f} | {score['empty']}/{score['negatives']} |"
            )
    lines.extend([
        "",
        "## Query groups",
        "",
        "Top 1 on the evaluated cutoff for each model.",
        "",
        "| Group | " + " | ".join(model["name"] for model in result["models"]) + " |",
        "| --- |" + " ---: |" * len(result["models"]),
    ])
    for group in dict.fromkeys(case["group"] for case in queries if case["relevant"]):
        cases = [
            (i, case) for i, case in enumerate(queries)
            if case["group"] == group and case["relevant"]
        ]
        wins = []
        for model in result["models"]:
            wins.append(sum(
                any(item in case["relevant"] for item in model["evaluated_ranks"][i][:1])
                for i, case in cases
            ))
        lines.append(f"| {group} | " + " | ".join(f"{count}/{len(cases)}" for count in wins) + " |")
    lines.extend([
        "",
        "## Timing",
        "",
        "| Model | Artifacts (MiB) | Index (s) | Worker startup (s) | Cold query (s) | Warm query median (ms) | Warm query p95 (ms) |",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: |",
    ])
    for model in result["models"]:
        timing = model["timing"]
        warm = sorted(timing["warm_query_ms"])
        p95 = warm[math.ceil(len(warm) * 0.95) - 1]
        lines.append(
            f"| {model['name']} | {model['artifacts_bytes'] / (1024 ** 2):.1f} | "
            f"{statistics.median(timing['index_ms']) / 1000:.3f} | "
            f"{statistics.median(timing['startup_ms']) / 1000:.3f} | "
            f"{statistics.median(timing['cold_query_ms']) / 1000:.3f} | "
            f"{statistics.median(warm):.2f} | {p95:.2f} |"
        )
    return "\n".join(lines) + "\n"


def environment(python):
    cpu = platform.processor() or platform.machine()
    info = Path("/proc/cpuinfo")
    if info.exists():
        cpu = next(
            (line.split(":", 1)[1].strip() for line in info.read_text().splitlines()
             if line.startswith("model name")),
            cpu,
        )
    dependency_versions = subprocess.check_output(
        [
            python, "-I", "-c",
            'import importlib.metadata as m, json, platform; '
            'print(json.dumps({"python": platform.python_version(), **'
            '{p: m.version(p) for p in ("onnxruntime", "tokenizers", "numpy")}}))',
        ],
        text=True,
    )
    return {
        **json.loads(dependency_versions),
        "cpu": cpu,
        "os": platform.freedesktop_os_release().get("PRETTY_NAME"),
        "kernel": platform.release(),
        "corpus_sha256": checksum(BENCHMARK / "corpus.json"),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--python", type=Path, default=Path(sys.executable))
    parser.add_argument("--e5", type=Path, default=ROOT / "model")
    parser.add_argument("--granite", type=Path, default=ROOT / "model/granite-97m-r2")
    parser.add_argument("--e5-large", type=Path, default=ROOT / "model/e5-large")
    parser.add_argument("--granite-311", type=Path, default=ROOT / "model/granite-311m-r2")
    parser.add_argument("--download-granite", action="store_true")
    parser.add_argument("--download-e5-large", action="store_true")
    parser.add_argument("--download-granite-311", action="store_true")
    parser.add_argument("--output", type=Path, default=ROOT / "target/retrieval-benchmark-granite-311")
    args = parser.parse_args()
    python = args.python.absolute()
    metadata = environment(python)
    for name, digest in E5_SHA256.items():
        require_artifact(args.e5 / name, digest)
    for name, (upstream, digest) in GRANITE_FILES.items():
        url = f"{GRANITE_URL}/{upstream}" if args.download_granite else None
        require_artifact(args.granite / name, digest, url)
    for name, digest in E5_LARGE_SHA256.items():
        url = f"{E5_LARGE_URL}/{name}" if args.download_e5_large else None
        require_artifact(args.e5_large / name, digest, url)
    for name, (upstream, digest) in GRANITE_311_FILES.items():
        url = f"{GRANITE_311_URL}/{upstream}" if args.download_granite_311 else None
        require_artifact(args.granite_311 / name, digest, url)
    process = subprocess.run(
        [
            "cargo", "run", "--release", "--locked", "-p", "semfind-core", "--example",
            "retrieval_benchmark", "--", BENCHMARK / "corpus.json", args.e5.absolute(),
            args.granite.absolute(), args.e5_large.absolute(), args.granite_311.absolute(), python,
        ],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
    )
    result = json.loads(process.stdout)
    result["environment"] = metadata
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "results.json").write_text(
        json.dumps(result, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    report = summary(result)
    (args.output / "summary.md").write_text(report, encoding="utf-8")
    print(report, end="")


if __name__ == "__main__":
    main()
