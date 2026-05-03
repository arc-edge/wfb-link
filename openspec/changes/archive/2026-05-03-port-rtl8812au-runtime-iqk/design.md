## Context

The current RTL8812AU TX path works by programming EFUSE-derived TXAGC and
captured Linux IQK/RFE constants. That is enough for bench WFB flow, but it
does not adapt IQ imbalance to the actual adapter, channel, temperature, or
antenna load. The standalone IQK diagnostic now proves we can safely collect
the upstream backup surfaces outside live WFB traffic, and the RF-quality runner
now preserves Linux receiver RSSI/SNR/MCS telemetry. Those pieces make it
reasonable to start a guarded runtime IQK port.

The Linux source of truth is aircrack-ng's RTL8812A CE path in
`hal/phydm/halrf/rtl8812a/halrf_8812a_ce.c`: `_phy_iq_calibrate_8812a`,
`_iqk_tx_8812a`, `_iqk_tx_fill_iqc_8812a`, `_iqk_rx_fill_iqc_8812a`, and the
backup/restore helpers. The macOS port already has the low-level primitives:
masked BB writes, RF serial reads/writes, delays, retained init, LCK staging,
and structured calibration reports.

## Goals / Non-Goals

**Goals:**

- Add an opt-in `rtl8812a-runtime-iqk` TX calibration profile that can run after
  retained init and channel setup.
- Port the upstream IQK sequence in bounded stages with explicit restore
  reporting and no live WFB traffic during the calibration itself.
- Record selected TX/RX IQC values, per-path success/failure, retry counts,
  before/after register evidence, and cleanup state.
- Validate runtime IQK with close-range receiver-backed A/B runs before treating
  it as production calibration.

**Non-Goals:**

- Make runtime IQK the default profile in this change.
- Claim long-distance readiness from bench-only IQK success.
- Port firmware-offloaded IQK, MCC IQK restore, DPK, or thermal power tracking.
- Optimize for HT40/VHT80 until the HT20 path is stable.

## Decisions

- Implement runtime IQK as a TX calibration profile, not as part of default
  init. This keeps risky RF writes behind the existing explicit profile and
  authorization flow.
- Stage implementation from least destructive to most destructive: report shape
  and fill helpers first, backup/restore next, one-shot TX IQK, one-shot RX IQK,
  then receiver-backed A/B validation.
- Keep standalone evidence separate from runtime calibration. The standalone
  diagnostic remains useful for debugging, but only the runtime profile can
  classify IQK as completed.
- Use the upstream retry bounds as hard limits and preserve restore attempts
  even if a read, one-shot, or candidate selection fails.
- Treat SNR/RSSI receiver telemetry as diagnostic evidence. WFB payload recovery
  and Linux-baseline loss margin remain the pass/fail signal.

## Risks / Trade-offs

- IQK writes can leave the BB/RF path in a bad state -> save MAC/BB, AFE, RF,
  page-select, and HSSI selector state, restore on all exits, and require a
  post-IQK WFB smoke before marking the profile usable.
- Candidate selection may fail on one path -> report per-path failure and apply
  upstream fallback IQC values only when that matches Linux behavior.
- A close-range pass may hide poor EVM at distance -> keep the production label
  behind receiver-backed or spectrum-backed range evidence.
- Full port size is large -> split into helper/report, calibration execution,
  and validation tasks so each stage can be reviewed and backed out.
