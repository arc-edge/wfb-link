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
    "length": 1,
    "data_hex": "00"
  }
]
```

Supported `kind` values are `control_read`, `control_write`, `bulk_in`, and `bulk_out`.

Use `request_type`, `request`, `value`, and `index` for USB control transfers. Use `endpoint` for bulk transfers. `length` is the transfer payload length, not including USB framing.
When usbmon submit lines include payload bytes after `=`, the importer preserves them as lowercase compact `data_hex`. Linux submit lines usually contain control-write and bulk-OUT payloads; control-read and bulk-IN payloads normally appear on completion lines and remain omitted by the importer.

Example:

```sh
cargo run -p wfb-radio-diag -- --json trace-import \
  --input fixtures/traces/usbmon-sample.txt \
  --output /tmp/usbmon-sample.json

cargo run -p wfb-radio-diag -- --json trace-compare \
  --expected fixtures/traces/init-minimal-expected.json \
  --observed fixtures/traces/init-minimal-observed.json

cargo run -p wfb-radio-diag -- trace-registers \
  --input linux-awus036ach-init.usbmon \
  --output /tmp/linux-awus036ach-register-final.json \
  --min-address 0x0000 \
  --max-address 0x0fff

cargo run -p wfb-radio-diag -- --json bridge-tx-bench \
  --macos-usbhost \
  --tx-status-registers-from /tmp/linux-awus036ach-register-final.json \
  --tx-status-delay-ms 100 \
  --frame-kind wfb-data \
  --payload-marker LINUXMAP1 \
  --i-understand-this-transmits
```

`--tx-status-registers-from` accepts the JSON written by `trace-registers --output` and merges those register addresses into the built-in TX status snapshot. It implies `--tx-status`, reads the extra registers before and after TX, and labels them as chip-side telemetry rather than RF confirmation.

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

Completion lines and comments are ignored. Submit-line payload bytes after `=` are preserved when present. The register-summary command reduces `control_write` events for Realtek vendor request `0x05` into final per-register payload bytes for 1-, 2-, and 4-byte writes. Longer vendor writes, such as firmware chunks, are excluded from that register map.

The current comparison is strict and positional for known fields because transfer ordering matters during init. Omitted `data_hex` in an expected trace means payload bytes are unknown, so older generated traces can still compare against imported Linux submit lines. When an expected trace includes `data_hex`, payload comparison is strict.
