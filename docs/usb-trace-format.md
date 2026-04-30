# USB Trace Format

`wfb-radio-diag trace-import` converts Linux usbmon text into normalized USB transfer events. `wfb-radio-diag trace-compare` compares two normalized event sequences. The format is intentionally small so Linux captures can be reduced before comparing them with macOS runs.

Each trace file is a JSON array:

```json
[
  {
    "kind": "control_write",
    "endpoint": null,
    "request_type": 64,
    "request": 5,
    "value": 2,
    "index": 0,
    "length": 1
  }
]
```

Supported `kind` values are `control_read`, `control_write`, `bulk_in`, and `bulk_out`.

Use `request_type`, `request`, `value`, and `index` for USB control transfers. Use `endpoint` for bulk transfers. `length` is the transfer payload length, not including USB framing.

Example:

```sh
cargo run -p wfb-radio-diag -- --json trace-import \
  --input fixtures/traces/usbmon-sample.txt \
  --output /tmp/usbmon-sample.json

cargo run -p wfb-radio-diag -- --json trace-compare \
  --expected fixtures/traces/init-minimal-expected.json \
  --observed fixtures/traces/init-minimal-observed.json
```

## Capturing On Linux

For a quick software capture, mount debugfs and capture the adapter's USB bus with usbmon:

```sh
sudo modprobe usbmon
sudo mount -t debugfs none /sys/kernel/debug || true
sudo cat /sys/kernel/debug/usb/usbmon/1u > linux-awus036ach-init.usbmon
```

Replace `1u` with the bus number from `lsusb -t`. Start capture before plugging or binding the adapter if you need driver attach/init transfers.

The importer currently reads usbmon text submit lines:

- `S Co... s ...` -> `control_write`
- `S Ci... s ...` -> `control_read`
- `S Bo...` -> `bulk_out`
- `S Bi...` -> `bulk_in`

Completion lines and comments are ignored. Payload bytes are intentionally not imported yet; the first comparison axis is transfer order, request fields, endpoints, and lengths.

The current comparison is strict and positional. That is useful for early init work because transfer ordering matters. If Linux captures show harmless polling-length variance, add a normalization script rather than weakening the core comparator first.
