## Context

The previous backend work proved that macOS can own an RTL8812AU/AWUS036ACH through IOUSBHost, initialize it, send and receive WFB-ng traffic, run a bounded/full bridge loop, and interoperate with a Linux WFB peer. That is enough to prove feasibility, but not enough for long-distance operation.

Long-distance WFB depends on RF quality: correct per-path/per-rate power indexes, stable RF frontend state, IQ imbalance control, LO/channel calibration, antenna/path behavior, and conservative rate/bandwidth choices. The current path still contains expedient calibration behavior from Linux captures and planted constants. It can deliver packets on the bench, but those shortcuts can hide poor EVM, asymmetric paths, unstable output power, and distance-limited performance.

The Linux RTL8812AU driver and Linux WFB-ng remain the baseline. The macOS path should be compared against Linux using the same physical adapters, antennas, channel, bandwidth, rate, keys, WFB settings, payload sizes, and test geometry.

## Goals / Non-Goals

**Goals:**

- Produce repeatable RF-quality reports for Mac-vs-Linux WFB runs.
- Map EFUSE TX-power data into explicit RTL8812AU per-path/per-rate TXAGC writes.
- Report the TX power, RFE, IQK, LCK, thermal, rate, bandwidth, queue, and descriptor state that was active during each run.
- Replace planted/captured calibration constants where practical, or clearly label remaining stop-gaps in reports.
- Establish close-range, attenuated/stepped, and outdoor/long-distance acceptance runs.
- Keep 20 MHz as the first range target; only promote 40/80 MHz after evidence proves actual wide-PPDU value.

**Non-Goals:**

- General Wi-Fi association, AP/STA behavior, or kernel networking integration.
- Automatic rate adaptation comparable to the Linux firmware/driver stack.
- USBDriverKit migration unless IOUSBHost becomes unstable during long-running RF tests.
- Regulatory-domain automation beyond explicit operator-selected channel, bandwidth, and guarded power controls.
- Claiming wide-bandwidth range improvement without spectrum/SDR or equivalent evidence of actual wide PPDU occupancy.

## Decisions

### Decision: Use Linux as the RF baseline, not abstract register parity

The comparison target is a working Linux WFB-ng/RTL8812AU run on the same hardware. Reports must capture Linux receiver results and, when possible, Linux USB/register evidence, but success is measured by RF/WFB outcomes: recovered payloads, loss, RSSI/noise metadata, throughput, and stability.

Alternative considered: continue diffing final register maps until they match. That has already been useful for bring-up, but final register parity is not sufficient for RF quality because calibration routines are sequence-sensitive and environment-sensitive.

### Decision: Split TX power work from IQ/LCK calibration work

EFUSE-derived TX power programming is smaller and easier to test than full IQK/LCK. It should be implemented first, with explicit reporting of before/write/after TXAGC values. IQK/LCK should start as instrumentation and controlled replay/approximation, then move toward proper ports only where the measured RF outcome justifies the complexity.

Alternative considered: port the full Linux calibration stack first. That risks spending a lot of time on opaque runtime code before we have a measurement harness that can prove it improved range.

### Decision: Treat every RF-quality run as a structured experiment

Each run should record adapter identity, MAC, EFUSE summary, RFE type, channel, bandwidth, rate/MCS, TX descriptor profile, power indexes, calibration state, antenna/path settings, WFB FEC settings, payload size, runtime, CPU, receiver counters, and artifact paths. A run without this context is not useful for range decisions.

Alternative considered: keep using ad hoc shell notes and pcap names. That was fine for discovery but does not support long-distance comparisons or regression tracking.

### Decision: Keep range acceptance conservative and staged

The first target is a stable 20 MHz link that approaches the Linux baseline under identical settings. Wider modes remain secondary until actual wide-PPDU behavior is proven.

Alternative considered: focus immediately on HT40/VHT because higher bandwidth sounds like better range. For WFB at distance, robust modulation, clean power, and FEC recovery matter more than wide channel occupancy.

## Risks / Trade-offs

- Linux receiver metadata may be incomplete or inconsistent -> keep WFB payload recovery and controlled packet counts as primary metrics, and use RSSI/noise only where the receiver reports it consistently.
- EFUSE-to-TXAGC mapping may differ by channel group, path, rate, board type, or regulatory cap -> implement the mapping as reportable steps with override hooks and Linux comparison fixtures.
- Full IQK/LCK ports may be large and brittle -> use measured outcome gates before porting deeper routines, and retain explicit stop-gap labels until real calibration is in place.
- Outdoor range tests are noisy and hard to reproduce -> require a close-range baseline, fixed test geometry notes, and repeated runs before accepting a conclusion.
- Higher TX power can reduce quality if calibration is wrong -> optimize for receiver recovery and error/loss behavior, not just maximum power index.
- Long-running bridge tests can affect normal WFB processes on the Linux peer -> keep setup/restore steps scripted and verify normal services are restarted after tests.

## Migration Plan

1. Add report fields and fixtures without changing default TX behavior.
2. Add EFUSE-derived TX power calculations behind explicit guarded flags.
3. Compare explicit TX power modes against existing known-good/planted behavior at close range.
4. Promote the safest EFUSE-derived mode to the default only after Linux-baseline comparison passes.
5. Add calibration instrumentation and evaluate whether IQK/LCK ports improve measured results.
6. Run staged range tests and document accepted operating profiles.

Rollback is straightforward while new behavior remains opt-in: use the existing bridge default/profile and omit RF-quality overrides. If a promoted default regresses, restore the previous explicit profile as the default and keep the new mode behind a flag.

## Open Questions

- Which Linux receiver metric set is reliable enough across driver versions for RSSI/noise/SNR-like comparison?
- Do we need an RF attenuator or SDR/spectrum capture to make indoor stepped tests meaningful before outdoor range runs?
- How much of Linux `phy_iq_calibrate_8812a` and `phy_lc_calibrate_8812a` must be ported for useful long-distance gains?
- Should accepted range profiles prefer fixed MCS1/MGNT monitor-injection behavior, or should a limited fixed-rate set be exposed for field tuning?
