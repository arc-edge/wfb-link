## 1. Linux Peer Hardening

- [x] 1.1 Add Linux peer command preflight with configurable noninteractive PATH, required/optional command classification, and JSON/text artifacts
- [x] 1.2 Make `iw`, `tcpdump`, `docker`, `ip`, `ps`, and cleanup commands use discovered absolute paths when available
- [x] 1.3 Add policy controls for missing `iw`/channel evidence so runs can either fail before RF or proceed with a clear degraded status
- [x] 1.4 Collect preflight/channel-state artifacts and include new settings in run config and dry-run output

## 2. Targeted Calibration Parity

- [x] 2.1 Add a guarded TX calibration profile enum/CLI/report value for targeted Linux-parity overrides
- [x] 2.2 Implement channel-36/HT20 targeted parity writes for the known RFE pinmux, TX scale, and TX BB control mismatches, with before/write/after evidence
- [x] 2.3 Keep targeted parity distinct from full `linux-ported` IQK/LCK in reports and stop-gap labels
- [x] 2.4 Add tests for profile parsing, write selection, and calibration-mode/report labels
- [x] 2.5 Document the remaining full IQK/LCK porting and distance-validation blockers

## 3. Receiver Metadata

- [x] 3.1 Extend `radio-core::RxFrame` with PHY-status presence, driver-info size, shift, RSSI source/validity, optional noise/SNR, and bounded raw PHY-status bytes
- [x] 3.2 Emit the richer metadata in RX JSONL and WFB bridge forwarding/report surfaces without breaking existing fields
- [x] 3.3 Update tests/fixtures to assert fallback RSSI is labeled as fallback and PHY-status RSSI is labeled as measured

## 4. Production-Readiness Reporting

- [x] 4.1 Update RF-quality report generation and docs to surface preflight status, calibration profile, and metadata confidence
- [x] 4.2 Run local formatting/tests and strict OpenSpec validation
- [x] 4.3 Commit and push the changes
- [x] 4.4 If the hardware path is reachable, rsync/deploy and run the hardened close-range smoke; otherwise record the validation blocker
