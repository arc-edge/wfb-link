## ADDED Requirements

### Requirement: Runtime MAC Address Initialization
The runtime library SHALL own RTL8812AU EFUSE MAC extraction and REG_MACID programming for initialization callers.

#### Scenario: EFUSE MAC extracted by runtime
- **WHEN** runtime initialization needs the adapter MAC address
- **THEN** the runtime reads physical EFUSE bytes through guarded EFUSE control-register operations, decodes the logical map, and returns a programmed non-blank MAC address when present

#### Scenario: REG_MACID programmed by runtime
- **WHEN** a caller provides a local adapter MAC address
- **THEN** the runtime reads the current REG_MACID bytes, writes the supplied six MAC bytes, reads them back, and returns before/written/after evidence plus counter deltas
