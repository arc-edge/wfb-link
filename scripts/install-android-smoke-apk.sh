#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
apk="${1:-${repo_root}/target/android-smoke-apk/wfb-link-android-smoke-debug.apk}"
component="com.arcedge.wfblink.smoke/com.arcedge.wfblink.smoke.WfbUsbSmokeActivity"
device_asset_dir="${ANDROID_SMOKE_ASSET_DIR:-/data/local/tmp/wfb-link}"
firmware="${ANDROID_SMOKE_FIRMWARE:-/tmp/rtl8812aefw.bin}"
mac_source="${ANDROID_SMOKE_MAC_SOURCE:-/tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_mac.c}"
bb_source="${ANDROID_SMOKE_BB_SOURCE:-/tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_bb.c}"
rf_source="${ANDROID_SMOKE_RF_SOURCE:-/tmp/wfb-ref-rtl8812au/hal/phydm/rtl8812a/halhwimg8812a_rf.c}"

if [[ ! -f "${apk}" ]]; then
  "${repo_root}/scripts/build-android-smoke-apk.sh"
fi

adb install -r "${apk}"
adb shell "mkdir -p '${device_asset_dir}'"
if [[ -f "${firmware}" && -f "${mac_source}" && -f "${bb_source}" && -f "${rf_source}" ]]; then
  adb push "${firmware}" "${device_asset_dir}/rtl8812aefw.bin" >/dev/null
  adb push "${mac_source}" "${device_asset_dir}/halhwimg8812a_mac.c" >/dev/null
  adb push "${bb_source}" "${device_asset_dir}/halhwimg8812a_bb.c" >/dev/null
  adb push "${rf_source}" "${device_asset_dir}/halhwimg8812a_rf.c" >/dev/null
else
  echo "warning: Android smoke init assets not found; register smoke can run, init smoke will fail" >&2
fi
adb shell am start -n "${component}"
