## ADDED Requirements

### Requirement: RTL8812AU RF Readback
The system SHALL support RTL8812AU RF serial readback for RF path A and RF path B using the Linux 8812A `phy_RFSerialRead` register sequence.

#### Scenario: RF register is read
- **WHEN** a diagnostic or calibration routine reads an RTL8812AU RF register
- **THEN** the system MUST select the RF offset through the HSSI read-address register, read the 20-bit value from the path's PI or SI readback register, and record the readback source in structured output

### Requirement: Guarded RTL8812A LCK Calibration
The system SHALL provide an explicit RTL8812A LCK calibration profile that runs the upstream local-oscillator calibration sequence after init and before TX.

#### Scenario: LCK profile is enabled
- **WHEN** the operator enables the LCK calibration profile
- **THEN** the command MUST pause packet TX when appropriate, read and preserve RF channel state, enter LCK mode, trigger the RF CHNLBW calibration bit, wait for the calibration window, exit LCK mode, restore state, and record structured evidence

#### Scenario: LCK profile is not enabled
- **WHEN** the operator uses the default calibration profile
- **THEN** the command MUST NOT run LCK and MUST preserve the existing default TX behavior
