#!/usr/bin/env python3
"""Summarize production duplex radio-run evidence directories."""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path
from typing import Any


def load_json(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    try:
        data = json.loads(path.read_text())
    except Exception as exc:  # pragma: no cover - defensive CLI path
        return {"_load_error": str(exc), "_path": str(path)}
    return data if isinstance(data, dict) else {"_value": data}


def as_int(value: Any, default: int = 0) -> int:
    if value is None:
        return default
    try:
        return int(value)
    except (TypeError, ValueError):
        return default


def as_float(value: Any, default: float | None = None) -> float | None:
    if value is None:
        return default
    try:
        return float(value)
    except (TypeError, ValueError):
        return default


def truthy(value: Any, default: bool = True) -> bool:
    if value is None:
        return default
    if isinstance(value, bool):
        return value
    return str(value).lower() not in {"0", "false", "no", "off", "disabled"}


def is_run_dir(path: Path) -> bool:
    return (
        (path / "summary.json").exists()
        or (path / "radio-run.json").exists()
        or (path / "peer" / "counter-m2l.json").exists()
        or (path / "peer" / "counter-l2m.json").exists()
    )


def discover_run_dirs(paths: list[Path], recursive: bool) -> list[Path]:
    run_dirs: list[Path] = []
    seen: set[Path] = set()

    def add(path: Path) -> None:
        resolved = path.resolve()
        if resolved not in seen and is_run_dir(path):
            seen.add(resolved)
            run_dirs.append(path)

    for input_path in paths:
        path = input_path.expanduser()
        if path.is_file():
            path = path.parent
        if not path.exists():
            continue
        add(path)
        runs_dir = path / "runs"
        if runs_dir.is_dir():
            for child in sorted(runs_dir.iterdir()):
                if child.is_dir():
                    add(child)
        if recursive:
            for summary_path in sorted(path.rglob("summary.json")):
                add(summary_path.parent)
    return run_dirs


def int_list(values: Any) -> list[int]:
    if not isinstance(values, list):
        return []
    result = []
    for value in values:
        try:
            result.append(int(value))
        except (TypeError, ValueError):
            continue
    return sorted(set(result))


def sequence_counts(counter: dict[str, Any]) -> dict[int, int]:
    counts = counter.get("sequence_counts")
    if not isinstance(counts, dict):
        return {}
    parsed = {}
    for key, value in counts.items():
        try:
            parsed[int(key)] = int(value)
        except (TypeError, ValueError):
            continue
    return parsed


def missing_sequences(counter: dict[str, Any], expected: int) -> list[int]:
    missing = int_list(counter.get("missing_sequences"))
    if missing or expected <= 0:
        return missing
    counts = sequence_counts(counter)
    if counts:
        return [seq for seq in range(expected) if seq not in counts]
    return []


def clusters(values: list[int]) -> list[dict[str, int]]:
    if not values:
        return []
    ranges = []
    start = values[0]
    prev = values[0]
    for value in values[1:]:
        if value == prev + 1:
            prev = value
            continue
        ranges.append({"start": start, "end": prev, "count": prev - start + 1})
        start = value
        prev = value
    ranges.append({"start": start, "end": prev, "count": prev - start + 1})
    return ranges


def periodic_delta(values: list[int]) -> int | None:
    if len(values) < 3:
        return None
    diffs = [right - left for left, right in zip(values, values[1:])]
    if not diffs:
        return None
    [(delta, count)] = Counter(diffs).most_common(1)
    if count >= max(2, len(diffs) - 1):
        return delta
    return None


def format_clusters(cluster_values: list[dict[str, int]], limit: int = 5) -> str:
    if not cluster_values:
        return "-"
    parts = []
    for item in cluster_values[:limit]:
        if item["start"] == item["end"]:
            parts.append(str(item["start"]))
        else:
            parts.append(f"{item['start']}-{item['end']}")
    if len(cluster_values) > limit:
        parts.append(f"+{len(cluster_values) - limit} more")
    return ",".join(parts)


def get_source_summary(run_dir: Path, summary: dict[str, Any]) -> dict[str, Any]:
    source = load_json(run_dir / "peer" / "source-summary.json")
    if source:
        return source
    nested = summary.get("source_summary")
    return nested if isinstance(nested, dict) else {}


def direction_enabled(summary: dict[str, Any], direction: str, counter: dict[str, Any]) -> bool:
    if counter.get("disabled"):
        return False
    directions = summary.get("directions")
    key = f"{direction}_enabled"
    if isinstance(directions, dict) and key in directions:
        return truthy(directions.get(key))
    return as_int(counter.get("expected"), 0) > 0 or bool(counter)


def direction_source_count(source_summary: dict[str, Any], direction: str) -> int | None:
    counts = source_summary.get("direction_counts")
    if not isinstance(counts, dict):
        return None
    for key in (direction, direction.upper()):
        if key in counts:
            return as_int(counts.get(key), 0)
    return None


def direction_source_lateness(source_summary: dict[str, Any], direction: str) -> float | None:
    values = source_summary.get("max_lateness_sec")
    if not isinstance(values, dict):
        return None
    for key in (direction, direction.upper()):
        if key in values:
            return as_float(values.get(key))
    return None


def summarize_direction(
    run_dir: Path,
    summary: dict[str, Any],
    source_summary: dict[str, Any],
    direction: str,
    lateness_warn_sec: float,
) -> dict[str, Any]:
    peer_counter = load_json(run_dir / "peer" / f"counter-{direction}.json")
    summary_counter = summary.get(f"{direction}_counter")
    counter = peer_counter or (summary_counter if isinstance(summary_counter, dict) else {})
    enabled = direction_enabled(summary, direction, counter)
    source_expected = as_int(source_summary.get("expected_payloads"), 0)
    expected = as_int(counter.get("expected"), source_expected)
    unique = as_int(counter.get("unique_sequences"), as_int(counter.get("recovered_payloads"), 0))
    missing = missing_sequences(counter, expected)
    missing_count = len(missing)
    if expected > 0 and unique < expected and not missing:
        missing_count = expected - unique
    duplicate_count = as_int(counter.get("duplicate_sequence_count"), 0)
    max_sequence_count = as_int(counter.get("max_sequence_count"), 0)
    min_seen_sequence_count = as_int(counter.get("min_seen_sequence_count"), 0)
    source_count = direction_source_count(source_summary, direction)
    max_lateness = direction_source_lateness(source_summary, direction)
    peer = summary.get("peer_wfb_rx") if isinstance(summary.get("peer_wfb_rx"), dict) else {}
    decrypt_after = as_int(peer.get(f"{direction}_decrypt_failures_after_session"), as_int(peer.get(f"{direction}_decrypt_failures"), 0))
    decrypt_total = as_int(peer.get(f"{direction}_decrypt_failures_total"), decrypt_after)
    tx = summary.get("tx") if isinstance(summary.get("tx"), dict) else {}
    tx_failed = as_int(tx.get("failed_submissions"), 0)
    tx_dropped = as_int(tx.get("dropped_datagrams"), 0)

    if not enabled:
        assessment = "disabled"
    elif expected <= 0:
        assessment = "no_expected_count"
    elif source_count is not None and source_count < expected:
        assessment = "source_incomplete"
    elif tx_failed or tx_dropped:
        assessment = "tx_path_issue"
    elif decrypt_after:
        assessment = "decrypt_or_corruption"
    elif missing_count and max_lateness is not None and max_lateness > lateness_warn_sec:
        assessment = "source_late_or_rf_loss"
    elif missing_count:
        assessment = "rf_or_receiver_loss"
    elif duplicate_count:
        assessment = "clean_with_duplicates"
    else:
        assessment = "clean"

    recovery = None
    if enabled and expected > 0:
        recovery = unique / expected
    missing_clusters = clusters(missing)
    return {
        "enabled": enabled,
        "expected": expected,
        "unique_sequences": unique,
        "recovery": recovery,
        "missing_count": missing_count,
        "missing_sequences": missing,
        "missing_clusters": missing_clusters,
        "missing_periodic_delta": periodic_delta(missing),
        "duplicate_sequence_count": duplicate_count,
        "max_sequence_count": max_sequence_count,
        "min_seen_sequence_count": min_seen_sequence_count,
        "source_count": source_count,
        "source_max_lateness_sec": max_lateness,
        "decrypt_failures_after_session": decrypt_after,
        "decrypt_failures_total": decrypt_total,
        "assessment": assessment,
    }


def signal_summary(summary: dict[str, Any]) -> dict[str, Any]:
    rx = summary.get("rx") if isinstance(summary.get("rx"), dict) else {}
    signal = rx.get("signal") if isinstance(rx.get("signal"), dict) else {}
    result: dict[str, Any] = {}
    for key in ("rssi_dbm", "snr_db", "noise_dbm"):
        value = signal.get(key)
        if isinstance(value, dict):
            item = {
                field: value.get(field)
                for field in ("avg", "average", "min", "max", "samples", "sample_count")
                if field in value
            }
            if "avg" not in item and "average" in item:
                item["avg"] = item["average"]
            if "samples" not in item and "sample_count" in item:
                item["samples"] = item["sample_count"]
            result[key] = item
    return result


def summarize_run(run_dir: Path, lateness_warn_sec: float) -> dict[str, Any]:
    summary = load_json(run_dir / "summary.json")
    meta = load_json(run_dir / "matrix-run-meta.json")
    source_summary = get_source_summary(run_dir, summary)
    tx = summary.get("tx") if isinstance(summary.get("tx"), dict) else {}
    radio = load_json(run_dir / "radio-run.json")
    radio_result = summary.get("radio_result") or radio.get("result")
    m2l = summarize_direction(run_dir, summary, source_summary, "m2l", lateness_warn_sec)
    l2m = summarize_direction(run_dir, summary, source_summary, "l2m", lateness_warn_sec)
    direction_assessments = [
        item["assessment"]
        for item in (m2l, l2m)
        if item["enabled"] and item["assessment"] != "disabled"
    ]
    tx_failed = as_int(tx.get("failed_submissions"), 0)
    tx_dropped = as_int(tx.get("dropped_datagrams"), 0)

    if not summary:
        assessment = "missing_summary"
    elif tx_failed or tx_dropped:
        assessment = "tx_path_issue"
    elif any(item == "source_incomplete" for item in direction_assessments):
        assessment = "source_incomplete"
    elif any(item == "decrypt_or_corruption" for item in direction_assessments):
        assessment = "decrypt_or_corruption"
    elif any(item == "source_late_or_rf_loss" for item in direction_assessments):
        assessment = "source_late_or_rf_loss"
    elif any(item == "rf_or_receiver_loss" for item in direction_assessments):
        assessment = "rf_or_receiver_loss"
    elif all(item in {"clean", "clean_with_duplicates"} for item in direction_assessments):
        assessment = "clean"
    else:
        assessment = "incomplete_evidence"

    return {
        "run_dir": str(run_dir),
        "profile": meta.get("profile_name") or run_dir.name,
        "radio_command": summary.get("radio_command") or meta.get("radio_command"),
        "smoke_result": summary.get("smoke_result"),
        "radio_result": radio_result,
        "failures": summary.get("failures") or [],
        "assessment": assessment,
        "source_summary": {
            "expected_payloads": source_summary.get("expected_payloads"),
            "direction_counts": source_summary.get("direction_counts"),
            "max_lateness_sec": source_summary.get("max_lateness_sec"),
            "payload_interval_sec": source_summary.get("payload_interval_sec"),
            "source_phase_sec": source_summary.get("source_phase_sec"),
        },
        "m2l": m2l,
        "l2m": l2m,
        "tx": {
            "submitted_frames": as_int(tx.get("submitted_frames"), 0),
            "failed_submissions": tx_failed,
            "dropped_datagrams": tx_dropped,
        },
        "signal": signal_summary(summary),
    }


def ratio_text(direction: dict[str, Any]) -> str:
    if not direction["enabled"]:
        return "disabled"
    expected = direction["expected"]
    unique = direction["unique_sequences"]
    missing = direction["missing_count"]
    if expected <= 0:
        return "n/a"
    clusters_text = format_clusters(direction["missing_clusters"])
    periodic = direction["missing_periodic_delta"]
    periodic_text = f" d={periodic}" if periodic else ""
    return f"{unique}/{expected} miss={missing} {clusters_text}{periodic_text}"


def source_text(run: dict[str, Any]) -> str:
    source = run["source_summary"]
    counts = source.get("direction_counts") or {}
    late = source.get("max_lateness_sec") or {}
    parts = []
    for direction in ("m2l", "l2m"):
        count = counts.get(direction, counts.get(direction.upper()))
        max_late = late.get(direction, late.get(direction.upper())) if isinstance(late, dict) else None
        if count is not None:
            if max_late is None:
                parts.append(f"{direction}={count}")
            else:
                parts.append(f"{direction}={count} late={float(max_late):.3f}s")
    return " ".join(parts) if parts else "-"


def signal_text(run: dict[str, Any]) -> str:
    signal = run.get("signal") or {}
    snr = signal.get("snr_db") or {}
    rssi = signal.get("rssi_dbm") or {}
    snr_avg = snr.get("avg")
    rssi_avg = rssi.get("avg")
    if snr_avg is None and rssi_avg is None:
        return "-"
    parts = []
    if snr_avg is not None:
        parts.append(f"snr={snr_avg}dB")
    if rssi_avg is not None:
        parts.append(f"rssi={rssi_avg}dBm")
    return " ".join(parts)


def print_text(runs: list[dict[str, Any]]) -> None:
    if not runs:
        print("No run evidence found.")
        return
    print("| Run | Command | Result | M2L | L2M | Source | TX | Signal | Assessment |")
    print("|---|---|---|---:|---:|---|---|---|---|")
    for run in runs:
        result = run.get("smoke_result") or run.get("radio_result") or "-"
        tx = run["tx"]
        tx_text = "submitted={submitted_frames} fail={failed_submissions} drop={dropped_datagrams}".format(**tx)
        print(
            "| `{run}` | `{command}` | `{result}` | {m2l} | {l2m} | {source} | {tx} | {signal} | `{assessment}` |".format(
                run=Path(run["run_dir"]).name,
                command=run.get("radio_command") or "-",
                result=result,
                m2l=ratio_text(run["m2l"]),
                l2m=ratio_text(run["l2m"]),
                source=source_text(run),
                tx=tx_text,
                signal=signal_text(run),
                assessment=run["assessment"],
            )
        )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Summarize radio-run smoke artifacts and classify payload loss evidence."
    )
    parser.add_argument("paths", nargs="+", type=Path, help="Run directory, matrix root, or summary file path")
    parser.add_argument("--json", action="store_true", help="emit machine-readable JSON")
    parser.add_argument("--recursive", action="store_true", help="scan recursively for summary.json files")
    parser.add_argument(
        "--lateness-warn-sec",
        type=float,
        default=0.020,
        help="source max-lateness threshold used in loss classification",
    )
    args = parser.parse_args()

    run_dirs = discover_run_dirs(args.paths, args.recursive)
    runs = [summarize_run(run_dir, args.lateness_warn_sec) for run_dir in run_dirs]
    result = {
        "run_count": len(runs),
        "clean_count": sum(1 for run in runs if run["assessment"] == "clean"),
        "issue_count": sum(1 for run in runs if run["assessment"] != "clean"),
        "runs": runs,
    }
    if args.json:
        print(json.dumps(result, indent=2, sort_keys=True))
    else:
        print_text(runs)
    return 0 if runs else 1


if __name__ == "__main__":
    sys.exit(main())
