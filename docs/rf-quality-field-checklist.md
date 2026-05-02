# RF Quality Field Checklist

Use this checklist before any stepped, attenuated, or outdoor run.

## Before The Run

- Confirm the close-range report for the same tuple exists and passed.
- Confirm channel, bandwidth, TX rate/profile, TX power mode, calibration mode,
  WFB link ID, radio port, FEC, and payload length match the planned run.
- Stop or isolate normal Linux WFB services.
- Pin Linux `wfb0` to the planned channel/bandwidth.
- Start receiver logs and RF capture.
- Start Mac bridge or bridge-run command with `--report`.
- Start any UDP relay required by the bench network path.
- Fill in `docs/rf-quality-field-notes-template.md`.

## During The Run

- Record distance or geometry estimate.
- Record antenna orientation and polarization at both ends.
- Record adapter placement, USB cable length, and nearby metal/carbon/battery
  placement.
- Record weather, obstructions, motion, and visible RF interference.
- Keep packet count, payload size, and source marker fixed for the whole run.

## After The Run

- Stop temporary WFB processes, relays, packet captures, and counters.
- Restore `arc-wfb-link-1` or the normal Linux WFB service set.
- Build an `rf-quality-report` envelope.
- Attach receiver logs, pcap, frame JSONL, counter JSON, SDR/spectrum captures,
  photos, or maps as companion artifacts.
- Check `comparison.status`, `comparison.outcome.acceptance_margin.status`,
  `profile_gate.status`, and `bandwidth_evidence.status`.
- Do not call a run range-ready if the close-range gate failed, the tuple
  mismatched, payload loss is outside margin, or wide-mode proof is missing.
