## Context

The first live automation run showed that `SYNC_HW_REPO=1` is not reliable on the hardware Mac because that host cannot authenticate to GitHub. The same host also has a dirty checkout with hardware-facing edits, so a blind rsync over `~/projects/arc/wfb-mac-radio-agent` would risk overwriting useful local state.

## Goals / Non-Goals

**Goals:**

- Provide an opt-in local-to-hardware-Mac deploy mode for RF-quality automation.
- Default the deploy destination to a separate run directory.
- Keep remote Git sync available for environments where it works.
- Make dry-run output and run configuration show which code path will be used.

**Non-Goals:**

- Replace Git history or remote repository publishing.
- Clean or reconcile the dirty hardware-Mac working checkout.
- Add bidirectional file sync from the hardware Mac back into the local checkout.

## Decisions

1. Use `rsync` from the local checkout to a separate deploy directory.

   The local machine has the authoritative committed state and GitHub access. `rsync` avoids remote Git credentials and can exclude `.git`, `target`, and transient artifacts.

2. Refuse deploy-to-working-checkout by default.

   The hardware-Mac working checkout is dirty and may contain useful state. A separate default deploy path prevents accidental overwrite while still letting the bridge run current code.

3. Keep `SYNC_HW_REPO=1` as remote Git pull behavior.

   Some future hardware hosts may have working Git credentials. Keeping that mode avoids conflating deploy sync with repository management.

## Risks / Trade-offs

- The deploy directory can grow stale build output if exclusions are wrong -> exclude `target` and let Cargo rebuild as needed.
- Deploy mode may be slower on the first run -> acceptable because RF validation is slower than rsync for this repo.
- Operators may point deploy mode at the dirty checkout intentionally -> require an explicit override flag for that case.
