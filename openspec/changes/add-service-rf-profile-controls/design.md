## Context

The production service now calls `wfb-radio-runtime` directly, but the service
config surface is narrower than the diagnostic compatibility command. RF-quality
scripts can start `radio-service`, but they currently reject TX-power
experiments for that command and tell operators to use `radio-run` instead.

The runtime already owns the TX-power and calibration profile policy, reporting,
and guardrails. This change is a command/config mapping and automation parity
slice, not a new calibration routine.

## Goals / Non-Goals

**Goals:**

- Expose runtime TX-power mode and calibration profile selection through
  `wfb-radio-service` CLI and TOML config.
- Preserve config-first service behavior while allowing CLI flags to override
  config values.
- Let close-range RF-quality automation run `radio-service` for the same
  production RF profile matrix currently available through `radio-run`.
- Keep evidence command-specific so service and diagnostic compatibility runs
  remain distinguishable.

**Non-Goals:**

- Port new IQK/LCK algorithms or change default RF behavior.
- Add diagnostic-only register writes, PCAP/JSONL capture, or TX-status probes
  to the service command.
- Change the runtime report schema beyond already-supported RF profile fields.

## Decisions

- Reuse the runtime profile enums and existing `radio-run` string semantics.
  This keeps service behavior aligned with the runtime and avoids adding a
  service-only profile vocabulary.
- Mirror the existing `radio-run` config layout by keeping TX-power controls in
  `[tx_power]` and calibration controls in `[calibration]`, with CLI overrides.
  This keeps production service configs compatible with the diagnostic
  compatibility command's profile vocabulary while still resolving to runtime
  types.
- Update RF-quality automation to pass service-native flags instead of
  synthesizing a config file. The script already controls command construction;
  keeping the selection in arguments makes dry-run output easier to inspect.

## Risks / Trade-offs

- Invalid profile strings could otherwise fail after USB open. Mitigation:
  parse and validate in service resolution before building runtime config.
- Experimental RF profiles may produce worse RF quality than the default.
  Mitigation: preserve the existing explicit transmit authorization and let
  RF-quality reports classify experimental or stop-gap profiles separately.
- Automation can drift between `radio-run` and `radio-service` command
  construction. Mitigation: share environment variable names and add dry-run
  coverage for service TX-power/profile command generation.

## Migration Plan

Existing service configs continue to work because default profile behavior is
unchanged. RF-quality scripts can switch from `MAC_RADIO_COMMAND=radio-run` to
`MAC_RADIO_COMMAND=radio-service` for profile experiments once this change is
merged.
