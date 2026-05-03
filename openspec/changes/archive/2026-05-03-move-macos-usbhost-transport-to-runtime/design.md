## Context

The current macOS USBHost module is already a runtime abstraction: it owns device/session RAII wrappers, retained bulk pipes, RTL8812AU register access, and the `UsbBulkTransfer` implementation. The diagnostic binary calls it from many command paths, but the implementation itself is not diagnostic-specific.

## Goals / Non-Goals

**Goals:**

- Move macOS USBHost transport implementation into `wfb-radio-runtime`.
- Preserve all existing type names and method names for diagnostic callers.
- Keep platform-specific compilation guarded to macOS.
- Keep the Objective-C shim linked by the crate that owns the FFI declarations.

**Non-Goals:**

- Do not redesign the transport API.
- Do not move command argument structs or diagnostic reports.
- Do not change libusb fallback behavior.
- Do not alter hardware init, TX/RX, IQK, or LCK logic.

## Decisions

- Move the module mostly unchanged.

  This keeps risk low. A later pass can reshape APIs once the runtime crate owns more session lifecycle.

- Put the build script in `wfb-radio-runtime`.

  The crate that declares the extern FFI should also compile/link the Objective-C shim and macOS frameworks. That keeps `wfb-radio-diag` from carrying runtime transport build responsibilities.

- Re-export the module only on macOS.

  Existing code is macOS-oriented today, and the IOUSBHost shim is not portable. Keeping the module behind `#[cfg(target_os = "macos")]` preserves non-mac build intent.

## Risks / Trade-offs

- Link behavior can regress if the shim build moves incorrectly. → Run full workspace tests on macOS and keep the FFI symbol names unchanged.
- Diagnostic code still references many runtime transport types. → Preserve the `macos_usbhost::TypeName` path by importing the runtime module under the same name.
- The runtime crate now has a platform-specific build script. → Keep the build script no-op on non-macOS.

## Migration Plan

1. Move Rust wrapper, Objective-C shim, and build script to `wfb-radio-runtime`.
2. Add runtime crate dependencies required by the transport.
3. Update diagnostic imports.
4. Run full workspace tests and OpenSpec validation.
