## ADDED Requirements

### Requirement: Production Service Receiver-Backed Smoke
The RF-quality automation SHALL provide a production service smoke that runs
`radio-run` from the reviewed production config and validates the accepted
robust short-range receiver-backed tuple.

#### Scenario: Service smoke uses reviewed config
- **WHEN** an operator runs the production service smoke automation
- **THEN** the automation starts `radio-run` with the checked-in production
  config file, a health artifact path, a ready marker, and a final report path
  without reconstructing the full runtime profile only from shell variables

#### Scenario: Service smoke gates robust tuple
- **WHEN** the production service smoke runs receiver-backed traffic
- **THEN** it uses symmetric M2L/L2M `3/12` FEC, MCS1, 20 ms payload pacing,
  observed session acquisition, and 1 s settle unless explicitly overridden

#### Scenario: Service smoke validates production health
- **WHEN** the production service smoke completes
- **THEN** it validates `radio_result=pass`, service health final state,
  zero post-session decrypt failures, zero TX failures, zero TX drops, nonzero
  RX forwarding snapshot counters, source timing evidence, and per-direction
  marked payload recovery at or above the configured minimum

#### Scenario: Service smoke preserves RF matrix separation
- **WHEN** the robust service smoke passes
- **THEN** the automation records it as a production plumbing/service-health
  gate and MUST NOT promote higher-throughput, runtime IQK, EFUSE-derived TX
  power, HT40/80, or long-distance profiles without separate RF-quality matrix
  evidence
