## Why

The hardware Mac cannot currently fast-forward from GitHub over SSH, and its working checkout is dirty. RF-quality automation needs a safe way to run the local code on the hardware Mac without overwriting that checkout or depending on remote Git credentials.

## What Changes

- Add an optional deploy mode to the RF-quality runner that rsyncs the local checkout to a separate hardware-Mac deploy directory.
- Keep deploy mode opt-in and refuse to deploy over the configured working checkout path by default.
- Exclude `.git`, build outputs, and transient local artifacts from deployment.
- Document when to use deploy mode versus the existing remote `git pull --ff-only` mode.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `rf-quality-run-automation`: Add hardware-Mac deploy sync behavior for RF-quality automation runs.

## Impact

- Updates `scripts/run-rf-quality-close-range.sh`.
- Updates RF-quality runbook documentation.
- Requires local `rsync` only when deploy mode is enabled.
- Does not modify the hardware-Mac dirty checkout unless the operator explicitly points deploy mode at that path.
