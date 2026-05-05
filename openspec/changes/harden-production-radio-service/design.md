## Context

`radio-run` now enters the production path through `wfb-radio-runtime` and has
passed RX-only, TX-positive, and receiver-backed short-range smokes. The next
production gap is operational: a future supervised process should be able to
start the accepted profile from a reviewed config file, observe readiness and
health through stable artifacts, stop the process cleanly, and decide whether a
run was healthy without parsing every diagnostic detail.

The current command remains useful as a development tool, but production
operation should not depend on copying a long CLI command by hand or inferring
health solely from the final JSON report. This change keeps the existing flow
and defaults intact while adding service-friendly configuration and state
surfaces.

## Goals / Non-Goals

**Goals:**

- Add a `radio-run` config file path for stable production profiles.
- Keep CLI flags available and let explicit CLI values override config file
  values when both are supplied.
- Write a service health artifact that is cheap for a supervisor to read while
  the process is running and after it exits.
- Add runtime-owned health/final-state classification that captures readiness,
  heartbeat, TX failures/drops, RX forwarding, stop reason, and operator-action
  hints without depending on diagnostic structs.
- Add a repeatable production service smoke using the robust short-range
  receiver-backed tuple that just passed the runtime cutover.

**Non-Goals:**

- Replacing `radio-run` with a daemon.
- Adding macOS launchd/systemd packaging in this slice.
- Changing WFB packet format, RF channel defaults, calibration defaults, or TX
  power policy.
- Promoting runtime IQK, EFUSE-derived TX power, HT40/80, or long-distance
  profiles to production default.

## Decisions

### 1. Config file maps onto existing runtime config

The config file should deserialize into a production-specific config layer that
maps to `ProductionRuntimeFlowConfig` and execution inputs. It should not
expose diagnostic-only register pokes, PCAP/JSONL paths, trace replay, or TX
status probes.

TOML is the preferred first format because it is readable, commonly used for
service configs, and already natural in a Rust project. JSON can remain an
internal report format. CLI flags should remain first-class and override file
values so existing scripts keep working and operators can do bounded overrides
without editing config.

Alternative considered: make the final report JSON reusable as config. That
would blur input and output contracts and would accidentally invite report-only
fields back into runtime input.

### 2. Health artifact is separate from ready marker and final report

The ready marker means "traffic may begin." The final report is comprehensive
but only available when the run exits. A service health file should be a small
JSON artifact that can be rewritten at major lifecycle points:

- starting / validating
- initialized / ready
- running
- stopping
- exited pass / exited fail

The health artifact should contain a stable `state`, timestamp, adapter/channel
identity when available, heartbeat counters, RX/TX health summaries, stop
reason when available, last error when present, and a concise
`operator_action` classification.

Alternative considered: extend the ready marker until it is effectively a
health file. That would break current automation semantics where ready marker
creation is a one-time start signal.

### 3. Runtime owns health classification, diagnostic owns file loading

The runtime should own health structs and classification helpers because a
future daemon will need them without linking diagnostic report types. The
diagnostic crate should own config-file loading and conversion for now because
paths, CLI parsing, and source assets still live there.

Alternative considered: keep all health classification in `wfb-radio-diag`.
That would repeat the ownership problem the runtime execution cutover just
fixed.

### 4. Production service smoke uses the robust short-range tuple

The default `8/12` duplex tuple is useful for throughput pressure but failed
under the current low-SNR bench placement while production plumbing was healthy.
The service hardening gate should use the accepted robust tuple:

- M2L `3/12`, L2M `3/12`
- MCS1 both directions
- 20 ms payload pacing
- observed session acquisition with 1 s settle

This gate proves service/config/health changes preserve receiver-backed WFB
flow. Higher-throughput and long-distance promotion remain separate RF-quality
matrix work.

## Risks / Trade-offs

- **[Risk]** Config-file support duplicates CLI defaults incorrectly.
  **Mitigation:** centralize merging into one adapter function and add tests for
  config-only, CLI-only, and CLI-overrides-config cases.
- **[Risk]** Health classification becomes too coarse for RF debugging.
  **Mitigation:** keep detailed reports unchanged; health points to the final
  report and artifacts rather than replacing them.
- **[Risk]** Service smoke becomes an RF tuning gate instead of a production
  plumbing gate. **Mitigation:** use the robust tuple and explicitly keep
  throughput/range matrices separate.
- **[Risk]** Health file writes during a tight bridge loop add overhead.
  **Mitigation:** update only at lifecycle transitions in this slice; periodic
  heartbeat counters can wait for a later watchdog cadence if needed.

## Migration Plan

1. Add config and health schema types with unit tests.
2. Add `radio-run --config <path>` and optional `--health-file <path>` while
   preserving all current flags.
3. Thread runtime health classification into `run_production_runtime_flow` and
   emit health updates at validation, ready, and exit boundaries.
4. Add a checked-in sample production profile for the robust short-range tuple
   and update smoke automation to consume it.
5. Run unit tests, strict OpenSpec validation, local production smoke, and the
   receiver-backed robust tuple smoke.

Rollback is a normal revert: the existing CLI-only `radio-run` path and smoke
scripts remain supported throughout the change.

## Open Questions

- Should the health file include a monotonically increasing update counter in
  this slice, or is timestamp plus state sufficient?
- Should the sample config live under `configs/`, `examples/`, or `docs/`?
- Should service mode be a boolean flag now, or should `--health-file` plus
  config file be enough until a real daemon/launchd wrapper exists?
