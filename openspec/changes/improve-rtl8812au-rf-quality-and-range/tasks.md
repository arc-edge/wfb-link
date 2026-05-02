## 1. Measurement Foundation

- [x] 1.1 Add an RF-quality report model covering adapter identity, EFUSE summary, RFE type, channel, bandwidth, TX descriptor profile, TX power mode, calibration state, WFB settings, receiver artifacts, and acceptance status
- [x] 1.2 Add a diagnostic command or subcommand that can run a macOS RF-quality profile and write the structured report without changing existing bridge defaults
- [x] 1.3 Add report support for referencing a Linux baseline run and recording parameter mismatches that make a comparison invalid or degraded
- [x] 1.4 Add tests for RF-quality report serialization, parameter mismatch detection, and safe behavior when no Linux baseline is supplied

## 2. Linux Baseline Capture

- [x] 2.1 Document the required Linux baseline commands for fixed-rate 20 MHz WFB TX/RX using the same adapter class, antennas, key, radio port, FEC, and payload size
- [x] 2.2 Add scripts or runbook snippets that collect Linux receiver logs, tcpdump/pcap artifacts, WFB payload counters, adapter identity, channel state, and command parameters
- [x] 2.3 Capture a close-range Linux 20 MHz baseline for the current bench and store artifact paths in the RF-quality report format
- [x] 2.4 Add a comparison summary that reports macOS payload recovery, loss, throughput, and receiver metadata against the Linux baseline

## 3. EFUSE-Derived TX Power

- [x] 3.1 Audit the decoded RTL8812AU EFUSE TX-power fields against the Linux driver's per-path/per-rate power-index calculation for the AWUS036ACH RFE type
- [x] 3.2 Implement a pure calculation helper that maps EFUSE power data, channel group, RF path, rate group, and safety clamps into TXAGC register values
- [x] 3.3 Add unit tests using captured EFUSE data and Linux-derived expected TXAGC values for at least channel 36 HT20
- [x] 3.4 Add an explicit guarded EFUSE-derived TX power mode to bridge TX commands, preserving the existing manual `--tx-power-index` override
- [x] 3.5 Report decoded source values, selected channel group, selected paths, per-rate indexes, clamps, and before/write/after TXAGC register evidence
- [x] 3.6 Run close-range macOS TX tests comparing planted/captured power behavior, manual power index behavior, and EFUSE-derived power behavior

## 4. Calibration State and RF Path Quality

- [x] 4.1 Add report fields for IQK, LCK, thermal, RFE pinmux, RFE timing, and RF path state used by each RF-quality run
- [x] 4.2 Label all planted, captured, or static calibration values as stop-gap calibration in reports
- [x] 4.3 Add readback probes for IQK/LCK-related BB/RF registers before and after channel setup and before TX
- [x] 4.4 Compare macOS calibration state against a Linux baseline run on the same adapter and channel
- [ ] 4.5 Decide whether a partial IQK/LCK approximation is sufficient or whether to port the Linux runtime routines, based on measured RF outcome
- [ ] 4.6 If justified, implement the smallest calibration routine subset that improves receiver-backed RF-quality metrics

## 5. Range Profiles and Acceptance Gates

- [ ] 5.1 Define close-range sanity, stepped/attenuated, and outdoor/long-distance RF-quality profiles in documentation
- [ ] 5.2 Add profile validation that requires a passing close-range run before recording a long-distance run for the same channel, bandwidth, rate, power mode, and payload settings
- [ ] 5.3 Add companion-note fields or templates for distance, antenna orientation, adapter placement, environment notes, and artifact paths
- [ ] 5.4 Define acceptance margins for macOS-vs-Linux comparison using WFB payload recovery, loss, throughput, and receiver metadata where reliable
- [ ] 5.5 Run and document an accepted close-range 20 MHz profile with the selected power/calibration mode
- [ ] 5.6 Run and document a longer-distance 20 MHz profile against the Linux baseline

## 6. Wide-Bandwidth Evidence

- [ ] 6.1 Keep HT40/VHT80 range profiles marked experimental until evidence proves actual wide-PPDU transmit/decode behavior
- [ ] 6.2 Add report fields that distinguish channel-context bandwidth from observed frame/PPDU bandwidth
- [ ] 6.3 Identify whether Linux receiver metadata, Mac RX descriptors, SDR/spectrum capture, or another tool will be the wide-PPDU evidence source
- [ ] 6.4 Run a controlled HT40 evidence test and classify the result as channel-context flow or proven wide-PPDU operation

## 7. Documentation and Regression

- [ ] 7.1 Update RF-quality and range documentation with baseline setup, Mac commands, Linux commands, interpretation guidance, and rollback instructions
- [ ] 7.2 Add regression fixtures for RF-quality report parsing and Linux-baseline comparison summaries
- [ ] 7.3 Add a final checklist for field runs that includes service restore, artifact collection, and accepted operating profile recording
- [ ] 7.4 Validate the change with `cargo fmt`, targeted tests, full workspace tests, and `openspec validate --strict`
