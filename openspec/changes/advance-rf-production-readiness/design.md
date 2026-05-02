## Context

The existing RF-quality work proved that the macOS RTL8812AU userspace path can initialize an AWUS036ACH, submit WFB traffic, and interoperate with a Linux WFB receiver at close range. It also added EFUSE-derived TXAGC and calibration-state reporting. The remaining production gap is not one thing; it is a chain of evidence and calibration quality.

The Linux aircrack-ng RTL8812AU driver is still the implementation reference. Register diffs show that static tables get the chip into a configured-but-not-final RF state, then runtime Linux functions rewrite RFE pinmux, TX scale, TX BB control, TXAGC, IQK result, and LCK-related state. Our current path uses captured/approximated tail writes for bring-up, which is acceptable for close-range validation but not enough to claim long-distance RF quality.

The Linux peer also needs hardening. Some drone images do not expose a full noninteractive `PATH`, and tools such as `iw` may be absent or installed outside the default shell path. The automation should make that visible, not fail cryptically or silently skip critical state.

## Goals / Non-Goals

**Goals:**

- Make Linux peer readiness explicit through a preflight artifact that records command paths, missing optional tools, and fail-fast policy decisions.
- Keep RF runs reversible and bounded even when optional Linux tools are missing.
- Add a targeted Linux-parity calibration profile for the known channel-36/HT20 register mismatches while clearly labeling it as targeted/replayed, not full IQK/LCK.
- Add receiver metadata fields that preserve raw descriptor/PHY evidence and mark RSSI/SNR confidence.
- Keep report surfaces honest about what is production-ready now versus what requires stepped/outdoor validation.

**Non-Goals:**

- Claiming long-distance acceptance before a real range or attenuated geometry exists.
- Fully porting all Linux ODM/PHYDM calibration machinery in one pass if live validation cannot prove each stage.
- Automatic Linux package installation or persistent drone image mutation.
- Regulatory-domain or automatic power policy management beyond existing explicit guarded controls.

## Decisions

### Decision: Treat Linux peer command discovery as a run artifact

The runner will build a remote command inventory with absolute paths where possible. Required tools fail the run before RF starts. Optional tools are recorded and skipped with clear logs unless a stricter environment variable makes them required.

This keeps the drone state observable without assuming a stable login shell, Docker availability, or `iw` installation.

### Decision: Add targeted parity before full IQK/LCK

Full `phy_iq_calibrate_8812a` is large, stateful, and sequence-sensitive. The next useful implementation step is a named targeted profile that writes the small set of Linux-final values known to affect the current channel-36/HT20 parity gap, then records before/after readback. That lets close-range tests and later distance tests compare “captured stop-gap” versus “targeted Linux parity” without hiding the fact that full runtime calibration is still incomplete.

The profile will not be labeled `linux-ported`; it will be labeled as targeted/replayed calibration until true IQK/LCK routines are implemented and validated.

### Decision: Make RX signal fields nullable/source-labeled

The current parser emits a default RSSI when PHY status is unavailable. Production reports should not treat that as measured signal. The parser will keep compatibility by retaining `rssi_dbm`, but it will also expose `rssi_dbm_source`, `rssi_dbm_valid`, optional per-chain signal placeholders, optional noise/SNR fields, PHY-status presence, driver-info size, and a bounded raw PHY-status sample.

### Decision: Leave long-distance profile application blocked by physical setup

Code can be improved now, but range acceptance remains blocked until the adapter/peer can be placed for distance or attenuation. The report should preserve this as a known validation gap rather than collapsing close-range success into a production claim.

## Risks / Trade-offs

- Hardcoded Linux-final calibration values may improve parity for one adapter/channel but be wrong for another geometry. Mitigation: make the profile explicit, report the exact writes, and keep EFUSE/runtime porting as the long-term path.
- Partial LCK/IQK ports can destabilize RF state if sequenced incorrectly. Mitigation: start with readback and targeted overrides, then add write routines only behind guarded flags with restore behavior.
- RX PHY-status parsing may be incomplete. Mitigation: report raw bytes and confidence instead of pretending all RSSI/SNR fields are authoritative.
- Linux peer hardening may expose missing tools that previous runs ignored. Mitigation: optional-vs-required policy keeps close-range WFB tests possible while showing the gap.

## Validation Plan

1. Run `openspec validate --strict`.
2. Run `cargo fmt`.
3. Run targeted Rust tests for RX parsing, bridge forwarding, RF-quality serialization, and calibration/profile helpers.
4. Run runner dry-run to verify command inventory/config output.
5. If hardware is reachable, deploy with rsync and run a close-range RF-quality smoke using the hardened runner. If Linux lacks `iw`, record the preflight gap and continue only when the selected policy allows it.

## Open Questions

- Which exact Linux PHY-status fields should be promoted into first-class per-chain RSSI/EVM/SNR once we have reliable descriptor captures?
- Does the targeted Linux-parity profile improve range metrics over the existing captured stop-gap when tested under attenuation or distance?
- How much of `phy_iq_calibrate_8812a` is needed before long-distance results stop being calibration-limited?
