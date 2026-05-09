#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
apk="${1:-${repo_root}/target/android-smoke-apk/wfb-link-android-smoke-debug.apk}"
component="com.arcedge.wfblink.smoke/com.arcedge.wfblink.smoke.WfbUsbSmokeActivity"

if [[ ! -f "${apk}" ]]; then
  "${repo_root}/scripts/build-android-smoke-apk.sh"
fi

adb install -r "${apk}"
adb shell am start -n "${component}"
