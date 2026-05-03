## ADDED Requirements

### Requirement: Runtime Radio Session
The runtime library SHALL expose a live radio session object that owns the selected transport, adapter metadata, endpoint layout, and runtime counters.

#### Scenario: Session created from opened transport
- **WHEN** a runtime USB transport open succeeds
- **THEN** the runtime can wrap it in a session carrying the transport, adapter metadata, endpoints, and initial counter state

#### Scenario: Session exposes operational handles
- **WHEN** runtime callers need register access or bulk endpoint selection
- **THEN** the session provides helpers for register access and selected bulk IN/OUT endpoint lookup without requiring diagnostic-only wrapper types
