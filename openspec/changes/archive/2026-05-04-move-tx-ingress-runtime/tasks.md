## 1. Runtime TX Ingress

- [x] 1.1 Add runtime-owned TX ingress socket, queued datagram, receiver, and shutdown types.
- [x] 1.2 Move UDP receive-buffer and read-timeout setup into runtime helpers with stable runtime errors.
- [x] 1.3 Add runtime tests for bind ordering, queued datagram delivery, and bind failure.

## 2. Diagnostic Adapter

- [x] 2.1 Refactor bridge-run TX socket binding to call runtime TX ingress helpers.
- [x] 2.2 Refactor bridge-run receiver loop to consume runtime queued datagrams.
- [x] 2.3 Keep existing bridge-run and radio-run report behavior unchanged.

## 3. Verification

- [x] 3.1 Update runtime boundary docs for TX ingress ownership.
- [x] 3.2 Run formatting, workspace tests, strict OpenSpec validation, and diff checks.
- [x] 3.3 Commit, push, sync to hardware Mac, and run a short no-TX or RX-only smoke if practical.
