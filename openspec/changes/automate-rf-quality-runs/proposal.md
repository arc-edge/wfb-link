## Why

The accepted close-range RF-quality workflow is still a hand-orchestrated sequence across the hardware Mac and the Linux WFB peer. Automating the repeatable parts reduces setup drift, preserves service restore behavior, and makes future RF quality comparisons easier to reproduce.

## What Changes

- Add a run-automation capability for controlled close-range and future stepped RF-quality runs.
- Provide a scriptable local entry point that coordinates the hardware Mac, Linux peer, UDP relay, bridge command, receiver capture, payload generation, and report collection.
- Record enough metadata and command output to diagnose failed setup, failed recovery, or incomplete cleanup without rerunning the whole test manually.
- Keep long-distance profile acceptance deferred until the hardware can be placed in a suitable separated, outdoor, or attenuated geometry.

## Capabilities

### New Capabilities

- `rf-quality-run-automation`: Orchestration for reproducible RF-quality runs across the local checkout, hardware Mac, and Linux WFB peer.

### Modified Capabilities

- None.

## Impact

- Adds automation scripts under `scripts/`.
- Updates RF-quality documentation to point at the automated runner.
- Uses existing commands: `bridge-tx-listen`, `rf-quality-report`, Linux `wfb_tx`, Linux `wfb_rx`, `tcpdump`, `iw`, and Docker service stop/start.
- No new runtime dependency is required beyond Bash, SSH, Python 3, Cargo, and the tools already used by the manual runbook.
