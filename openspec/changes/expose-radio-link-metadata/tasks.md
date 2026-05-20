## 1. Runtime Signal Model

- [x] 1.1 Extend runtime signal metric/summary types with `last`,
      `last_sample_unix_ms`, state, quality level/label, and quality basis.
- [x] 1.2 Preserve existing aggregate `rx.signal` fields while adding the new
      fields in a backward-compatible serialized shape.
- [x] 1.3 Add unit tests for valid samples, invalid/fallback RSSI, quality
      thresholds, and stale/unknown states.

## 2. Stream Attribution

- [x] 2.1 Add signal summaries and RF debug context to
      `ProductionRuntimeRxForwardRuntime` and
      `ProductionRuntimeRxForwardSnapshot`.
- [x] 2.2 Observe stream-level signal only after a frame matches the configured
      WFB forward link ID/radio port.
- [x] 2.3 Add unit tests proving matching frames update the matching stream
      signal and non-matching frames do not.

## 3. Production Health Cadence

- [x] 3.1 Add bounded running health snapshot writes for configured health files,
      with a documented default cadence such as 1 Hz.
- [x] 3.2 Ensure periodic health snapshots include aggregate and per-forward
      radio-link metadata without introducing diagnostic-only report fields.
- [x] 3.3 Add tests for running health snapshot cadence/no-op behavior when no
      health file is configured.

## 4. Product And Android Surfaces

- [x] 4.1 Add structured signal metadata to `wfb-link` aggregate and stream RX
      health while preserving existing average fields.
- [x] 4.2 Expand Android SDK signal classes with quality/state/last fields and
      parse them from runtime report JSON.
- [x] 4.3 Update docs for product link health, Android SDK consumption, local
      receiver semantics, unknown/stale/disconnected behavior, and remote RSSI
      follow-up scope.

## 5. Verification

- [x] 5.1 Run focused Rust tests for runtime RX signal, RX forward attribution,
      and `wfb-link` JSON mapping.
- [x] 5.2 Run Android SDK/consumer compile tests affected by Java model changes.
- [x] 5.3 Run `openspec validate expose-radio-link-metadata --strict`.
