## MODIFIED Requirements

### Requirement: Production Runtime Command Surface
The system SHALL provide a production-oriented WFB runtime entry point that
opens, initializes, receives, and transmits through runtime-owned types rather
than diagnostic bridge argument or report types.

#### Scenario: Production runtime can be embedded
- **WHEN** a Rust product backend embeds the macOS production radio runtime
- **THEN** it can build a runtime-owned production config, start execution
  without process-wide signal handlers, observe readiness/health/report data,
  and request cooperative shutdown through a Rust handle
