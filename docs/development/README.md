# Development Notes

This directory holds bring-up history, diagnostic command references, and
engineering notes that are useful while changing the radio backend but are too
detailed for the root README.

- [Bring-up notes](bring-up-notes.md): the original long README, preserved as
  a development journal and command reference.
- [Bench plan](../bench-plan.md): current bench assumptions, smoke paths, and
  production-readiness evidence.
- [Runtime boundary](../runtime-boundary.md): what lives in
  `wfb-radio-runtime`, what remains diagnostic-owned, and the migration order.
- [macOS USBHost](../macos-usbhost.md): direct IOUSBHost fallback used when
  libusb cannot enumerate the adapter.
- [USB trace format](../usb-trace-format.md): normalized transfer traces for
  Linux/macOS comparison.
- [RTL8812AU port checklist](../rtl8812au-port-checklist.md): porting and
  register-function checklist.
- [Calibration state](../rtl8812au-calibration-state.md): IQK/LCK/TX-power
  evidence and known gaps.

