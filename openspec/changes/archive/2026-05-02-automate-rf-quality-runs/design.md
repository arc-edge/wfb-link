## Context

The current close-range RF-quality runbook works, but it spans three execution contexts: this local checkout, the hardware Mac with the RTL8812AU adapter, and the Linux WFB peer reached through the hardware Mac. The manual flow also includes transient service disruption on the Linux peer, a UDP relay on the hardware Mac, long-running remote processes, artifact copying, and a final `rf-quality-report` envelope.

## Goals / Non-Goals

**Goals:**

- Provide a repeatable local command for the accepted close-range channel 36 HT20 EFUSE-derived profile.
- Keep every remote action visible and overrideable through environment variables.
- Always attempt Linux service restoration after setup has started.
- Produce a timestamped local artifact directory that can be attached to reports and issue comments.
- Leave enough structure to extend the runner to stepped or outdoor profiles later.

**Non-Goals:**

- Promote long-distance RF quality without a real separated, outdoor, or attenuated test geometry.
- Replace `bridge-run` or `bridge-tx-listen` with a new Rust orchestration layer in this change.
- Implement runtime IQK/LCK calibration or alter TX power calculations.
- Hide RF transmission behind implicit defaults; the runner remains explicitly named and profile-scoped.

## Decisions

1. Use a Bash runner for the first implementation.

   The workflow is primarily process orchestration over SSH. Bash keeps the first useful version small, easy to inspect, and close to the manual runbook. A Rust command can replace it later if we need richer state machines or portable process supervision.

2. Execute from the local checkout and use SSH for both remote contexts.

   The local machine owns the repo, validation, and final artifact directory. The hardware Mac runs the USB bridge and relay. The Linux peer is reached by nesting SSH through the hardware Mac, matching the current network topology.

3. Prefer timestamped `/tmp` remote artifact paths and local collection.

   `/tmp` paths match the existing runbook and avoid requiring persistent remote configuration. The local output directory becomes the stable evidence bundle.

4. Treat cleanup as best-effort but mandatory to attempt.

   The Linux WFB service must be restored even when the bridge, relay, sender, or receiver fails. Cleanup failures are recorded rather than hidden.

## Risks / Trade-offs

- SSH quoting and nested remote commands are fragile -> keep the first script profile-scoped, run `bash -n`, and expose dry-run output.
- Remote service names and IP addresses may drift -> define environment variables for every site-specific value.
- Artifact copying can fail after a useful RF run -> collect remaining artifacts and report missing paths instead of deleting local evidence.
- Throughput timing remains orchestration-influenced -> keep using receiver-backed payload recovery as the primary close-range acceptance signal.

## Migration Plan

1. Add the script alongside existing RF-quality support scripts.
2. Document it as the preferred way to run the close-range sanity profile.
3. Keep the manual runbook intact as a fallback and debugging reference.
4. Extend the script or replace it with a Rust command only after the shell runner has stabilized.
