## ADDED Requirements

### Requirement: Android SDK Production Runtime Startup
The production runtime SHALL preserve production readiness, endpoint, telemetry,
and managed-helper semantics when started through the Android SDK.

#### Scenario: SDK startup reports readiness
- **WHEN** the Android SDK starts a managed WFB stream session and radio
  initialization completes
- **THEN** the runtime writes or returns readiness data containing source,
  channel, bandwidth, runtime bounds, init/calibration flags, and local endpoint
  details before payload traffic is expected

#### Scenario: SDK startup reports managed stream telemetry
- **WHEN** the Android SDK managed session exits
- **THEN** the result includes production TX datagram/submission counters, RX
  frame/forward counters, raw application TX/RX counters, helper status, stop
  reason, and runtime error state

#### Scenario: Local forwarding failures do not abort RF runtime
- **WHEN** local Android UDP forwarding to a managed helper transiently fails
- **THEN** the production runtime records the failure as telemetry and continues
  the RF session unless a required helper exits or the USB/radio runtime fails
