## MODIFIED Requirements

### Requirement: Runtime Bridge Loop Plan
The runtime library SHALL expose a production WFB bridge-loop plan and TX
ingress lifecycle that expand TX ingress sockets, RX forwarding targets,
runtime bounds, and WFB metadata without depending on diagnostic command
argument structs.

#### Scenario: Valid loop plan is built
- **WHEN** a production runtime flow supplies a TX bind address, additional TX
  bind addresses, RX timeout, TX burst limit, max datagrams, and WFB forwarding
  targets
- **THEN** the runtime library returns a loop plan containing ordered TX bind
  addresses, validated RX forwarding configs, WFB metadata, and runtime bounds

#### Scenario: TX ingress starts from plan
- **WHEN** the production loop plan is ready and execution begins
- **THEN** the runtime library binds TX ingress sockets and starts receiver
  threads using the ordered TX bind addresses from the plan

#### Scenario: Invalid loop bounds are rejected
- **WHEN** a production runtime flow supplies zero RX timeout or zero TX burst
  limit
- **THEN** the runtime library rejects the loop plan before socket binding or
  USB open with a stable runtime error code
