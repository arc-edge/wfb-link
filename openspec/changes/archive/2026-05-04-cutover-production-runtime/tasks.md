## 1. Runtime Production Types

- [x] 1.1 Add runtime-owned production flow config, report, telemetry, and error types.
- [x] 1.2 Add validation for production-only bounds and required authorization before USB open.
- [x] 1.3 Add runtime tests proving production config/report types do not carry diagnostic-only register experiment fields.

## 2. Production Command Surface

- [x] 2.1 Add a thin production command or binary that maps CLI flags into runtime production config.
- [x] 2.2 Keep diagnostic `runtime-flow` compatibility while routing shared telemetry through runtime-owned types where practical.
- [x] 2.3 Document the production command/API and the remaining diagnostic boundary.

## 3. Verification

- [x] 3.1 Run formatting, workspace tests, strict OpenSpec validation, and diff checks.
- [x] 3.2 Commit and push the cutover slice.
