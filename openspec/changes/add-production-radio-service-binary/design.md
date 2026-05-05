## Context

`radio-run` has crossed the important internal boundary: normal production
execution now calls `wfb-radio-runtime::run_production_runtime_flow` and emits
runtime-owned health/report types. The remaining operator boundary is the
executable itself. A production deployment should not have to invoke
`wfb-radio-diag`, because that binary also contains bring-up diagnostics,
register experiments, trace tooling, and other workflows that are intentionally
outside the production surface.

The current production config loader and command-line merge code live in
`crates/wfb-radio-diag/src/main.rs`. A standalone production binary needs the
same reviewed config semantics without importing diagnostic command structs or
report formatting.

## Goals / Non-Goals

**Goals:**

- Add a small production binary that runs the same runtime-owned production
  flow used by `radio-run`.
- Make the binary config-first and supervisor-friendly: `--config`,
  `--ready-file`, `--health-file`, `--report`, bounded overrides, and the
  required live TX/write acknowledgements.
- Share the production config merge and runtime input mapping between the
  diagnostic compatibility command and the production binary.
- Update smoke automation so receiver-backed production gates can exercise the
  production binary by default or by explicit selection.

**Non-Goals:**

- Removing `wfb-radio-diag radio-run`.
- Adding launchd/systemd units in this slice.
- Changing RF defaults, calibration defaults, WFB packet format, or accepted
  robust tuple gates.
- Promoting runtime IQK, EFUSE-derived TX power, HT40/80, or long-distance
  profiles to production defaults.

## Decisions

### 1. Add a separate binary crate for the production command

The production executable should be visible as its own workspace package, for
example `wfb-radio-service`, rather than as another subcommand of
`wfb-radio-diag`. This keeps packaging and operator documentation honest: the
binary name communicates that it is the supported service surface.

Alternative considered: add a second binary target inside `wfb-radio-diag`.
That would be quicker, but still ties the production executable to the
diagnostic package boundary and makes it easier to accidentally reuse
diagnostic-only modules.

### 2. Extract production command support before adding behavior

The config file schema, CLI override merge, report writing, and command-facing
runtime input construction should move into a reusable module or small library
surface that both `wfb-radio-diag radio-run` and `wfb-radio-service` can call.
The runtime crate remains the owner of hardware execution and health/report
types; the command support layer owns operator config parsing and path handling.

Alternative considered: duplicate the config loader in the new binary. That
would get a binary faster but would immediately create two subtly different
production config contracts.

### 3. Preserve the diagnostic compatibility command during migration

`wfb-radio-diag radio-run` should remain a compatibility adapter that delegates
to the shared production command support. This lets existing scripts keep
working while production smoke automation begins exercising the smaller binary.

Alternative considered: switch every script at once and leave `radio-run`
stale. That would create unnecessary rollback risk because the diagnostic
command is still useful during RF bring-up.

### 4. Smoke automation selects the command path explicitly

Scripts should accept a command selector, with the production binary as the
target path for production gates and diagnostic `radio-run` retained as an
override. The selector should be recorded in smoke summaries so artifact review
can prove which command surface was exercised.

Alternative considered: infer the binary from `PATH`. That makes failures hard
to read and can accidentally test a stale installed executable.

## Risks / Trade-offs

- **[Risk]** The extraction touches command code that just passed receiver-backed
  gates. **Mitigation:** keep the first extraction mechanical and require unit
  tests plus the same local and receiver-backed smoke before marking complete.
- **[Risk]** A new binary can diverge from diagnostic `radio-run` during the
  transition. **Mitigation:** both command paths must use the same production
  config merge and execution adapter.
- **[Risk]** Packaging work expands scope. **Mitigation:** stop at a workspace
  binary plus scripts; launchd/systemd units remain a later change.

## Migration Plan

1. Extract the existing `radio-run` config-file schema, CLI merge result, and
   runtime execution adapter into shared production command support.
2. Update `wfb-radio-diag radio-run` to call the shared adapter without changing
   its report or CLI contract.
3. Add the production binary with a minimal config-first CLI and JSON/text
   output matching the production report contract.
4. Update smoke automation to select the production binary and record the
   command surface in summaries.
5. Re-run formatting, tests, strict OpenSpec validation, local smoke, and
   receiver-backed robust tuple smoke.

Rollback is a normal revert. The diagnostic compatibility path remains present
throughout the change.

## Open Questions

- Should the binary name be `wfb-radio-service` or `wfb-radio-run`? The design
  assumes `wfb-radio-service` unless implementation exposes a stronger local
  convention.
- Should the shared command support live in `wfb-radio-runtime` or a small new
  support crate? Prefer the smallest boundary that avoids diagnostic
  dependencies while keeping runtime hardware APIs report-neutral.
