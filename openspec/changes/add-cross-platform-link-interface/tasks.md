## 1. Interface Design

- [x] 1.1 Define the cross-platform control-plane and data-plane boundary.
- [x] 1.2 Document macOS userspace-radio and Linux native-WFB backend
      responsibilities.
- [x] 1.3 Define the 8-hour macOS embedding slice and longer-term codec path.

## 2. Embedding API

- [ ] 2.1 Add a small product-facing Rust interface module/crate with
      `LinkBackend`, `LinkHandle`, endpoint, health, and report types.
- [ ] 2.2 Add a macOS backend handle that starts the existing production
      runtime on a thread without installing process signal handlers.
- [ ] 2.3 Add cooperative stop plumbing for embedded runtime use.

## 3. Examples And Validation

- [ ] 3.1 Add an example Rust binary that starts the macOS backend, waits for
      ready, prints endpoints/health, requests stop, and prints the report.
- [ ] 3.2 Add unit tests for endpoint shape, embedded no-signal behavior, and
      cooperative stop/report behavior.
- [ ] 3.3 Run the hardware `PROFILE_SET=loaded` gate after embedding changes.
