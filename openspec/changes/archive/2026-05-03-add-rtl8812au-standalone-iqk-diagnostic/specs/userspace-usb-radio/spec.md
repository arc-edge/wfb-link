## ADDED Requirements

### Requirement: RTL8812AU Standalone IQK Diagnostic
The system SHALL provide a guarded standalone RTL8812AU IQK diagnostic that
initializes the adapter and collects deep IQK evidence without running WFB TX,
WFB RX, synthetic TX, or the IQK calibration sweep.

#### Scenario: Standalone IQK diagnostic collects evidence
- **WHEN** an operator runs the standalone IQK diagnostic on an initialized or
  initializable RTL8812AU adapter with the required hardware-write
  acknowledgement
- **THEN** the system reports MAC/BB backup registers, AFE backup registers, RF
  backup offsets for path A and path B, page-C1 latch registers, normal-page
  IQK result registers, USB counters, and cleanup status

#### Scenario: Standalone IQK diagnostic avoids live traffic
- **WHEN** the standalone IQK diagnostic runs
- **THEN** the system MUST NOT submit WFB datagrams, synthetic TX frames, or
  bulk-IN receive loops as part of the diagnostic

#### Scenario: Standalone IQK diagnostic restores selectors
- **WHEN** the diagnostic reads page-C1 or RF serial IQK evidence
- **THEN** the system attempts to restore BB page selection, HSSI/RF readback
  selectors, and RF serial state before exiting and reports any cleanup failure

#### Scenario: Standalone IQK diagnostic does not claim calibration
- **WHEN** the diagnostic completes successfully
- **THEN** the report MUST label the output as evidence-only and MUST NOT
  report runtime IQK calibration as completed
