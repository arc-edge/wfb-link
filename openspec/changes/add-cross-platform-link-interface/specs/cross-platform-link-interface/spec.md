## ADDED Requirements

### Requirement: Cross-Platform Link Control Interface
The system SHALL define a product-facing Rust link interface that can start,
observe, and stop a WFB link without exposing platform-specific radio
implementation details to the product binary.

#### Scenario: Backend starts link
- **WHEN** the product binary starts a configured link backend
- **THEN** the backend starts the platform-specific radio/WFB stack and returns
  a handle that exposes endpoints, readiness, health, cooperative stop, and a
  final report

#### Scenario: Ready state is observable
- **WHEN** the backend has claimed/configured the local radio path and local
  data-plane endpoints are usable
- **THEN** `wait_ready` succeeds and returns the endpoint snapshot without
  claiming remote-peer RF quality has been accepted

#### Scenario: Embedded stop avoids process signals
- **WHEN** the product binary embeds a backend in-process
- **THEN** the backend MUST support cooperative stop without requiring
  process-wide SIGINT or SIGTERM handlers

### Requirement: Shared WFB Data-Plane Endpoints
The system SHALL expose product-facing local stream or tunnel endpoints whose
semantics are stable across macOS and Linux backends.

#### Scenario: Stream endpoints are exposed
- **WHEN** a backend starts streams
- **THEN** it reports each stream's name, direction, local UDP endpoint,
  payload kind, and WFB stream identity when that endpoint maps to one stream

#### Scenario: Product code avoids raw RF dependency
- **WHEN** product code sends or receives normal payload traffic
- **THEN** it uses reported WFB stream/tunnel endpoints rather than raw
  RTL8812AU USB, Linux monitor injection, or 802.11 descriptor APIs

#### Scenario: Lower-level WFB datagram mode remains explicit
- **WHEN** a backend exposes WFB distributor datagram endpoints directly
- **THEN** the endpoint payload kind MUST identify that the product or caller
  owns the WFB-NG codec/session layer
- **AND** endpoints that can carry multiple WFB streams MAY omit a single
  stream identity

### Requirement: Platform Backend Responsibilities
The system SHALL let macOS and Linux use different backend implementations
behind the shared link interface.

#### Scenario: macOS backend uses userspace radio runtime
- **WHEN** the selected backend is macOS userspace radio
- **THEN** it uses the production `wfb-radio-runtime` / `wfb-radio-service`
  path for AWUS036ACH ownership and reports production radio telemetry

#### Scenario: Linux backend uses native WFB stack
- **WHEN** the selected backend is Linux native WFB
- **THEN** it uses the Linux monitor-mode interface, stock WFB-NG tools, and
  aircrack/rtl88xxau driver path rather than this macOS USB bridge

#### Scenario: Backend health preserves evidence
- **WHEN** a backend reports health or exits
- **THEN** it provides normalized lifecycle, endpoint, TX, RX, and readiness
  fields while preserving backend-specific diagnostics under a backend-specific
  evidence section
