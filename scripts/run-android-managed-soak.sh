#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

channel_number="${CHANNEL_NUMBER:-161}"
duration_ms="${DURATION_MS:-1200000}"
payload_interval_ms="${PAYLOAD_INTERVAL_MS:-100}"
if [[ "${payload_interval_ms}" -le 0 ]]; then
  echo "PAYLOAD_INTERVAL_MS must be positive" >&2
  exit 2
fi
payload_count="${PAYLOAD_COUNT:-$(((duration_ms + payload_interval_ms - 1) / payload_interval_ms))}"
managed_only="${MANAGED_ONLY:-true}"
validation_traffic="${VALIDATION_TRAFFIC:-true}"
preauthorize_android_network="${PREAUTHORIZE_ANDROID_NETWORK:-true}"
android_network_policy_mode="${ANDROID_NETWORK_POLICY_MODE:-}"
if [[ -z "${android_network_policy_mode}" ]]; then
  if [[ "${preauthorize_android_network}" == "true" ]]; then
    android_network_policy_mode="preauthorize"
  else
    android_network_policy_mode="strict"
  fi
fi
android_keep_awake="${ANDROID_KEEP_AWAKE:-}"
if [[ -z "${android_keep_awake}" ]]; then
  if [[ "${android_network_policy_mode}" == "preauthorize" ]]; then
    android_keep_awake="true"
  else
    android_keep_awake="false"
  fi
fi
log_dir="${LOG_DIR:-/tmp/wfb-link-android-managed-soak-$(date +%Y%m%d-%H%M%S)}"
package_name="com.arcedge.wfblink.smoke"
activity_name="${package_name}/.WfbUsbSmokeActivity"

case "${android_network_policy_mode}" in
  preauthorize|strict|unchanged) ;;
  *)
    echo "ANDROID_NETWORK_POLICY_MODE must be preauthorize, strict, or unchanged" >&2
    exit 2
    ;;
esac

mkdir -p "${log_dir}"

cat >"${log_dir}/request.json" <<EOF
{
  "channel_number": ${channel_number},
  "duration_ms": ${duration_ms},
  "payload_count": ${payload_count},
  "payload_interval_ms": ${payload_interval_ms},
  "managed_only": "${managed_only}",
  "validation_traffic": "${validation_traffic}",
  "preauthorize_android_network": "${preauthorize_android_network}",
  "android_network_policy_mode": "${android_network_policy_mode}",
  "android_keep_awake": "${android_keep_awake}"
}
EOF

package_uid="$(
  adb shell dumpsys package "${package_name}" \
    | sed -n 's/.*appId=\([0-9][0-9]*\).*/\1/p' \
    | head -n 1 \
    | tr -d '\r'
)"

{
  echo "package=${package_name}"
  echo "uid=${package_uid:-unknown}"
  echo "mode=${android_network_policy_mode}"
  echo "keep_awake=${android_keep_awake}"
} >"${log_dir}/android-network-policy.txt"

if [[ "${android_network_policy_mode}" == "preauthorize" ]]; then
  if [[ -n "${package_uid}" ]]; then
    adb shell cmd connectivity set-background-networking-enabled-for-uid "${package_uid}" true \
      >/dev/null 2>&1 || true
  fi
  adb shell cmd deviceidle whitelist +"${package_name}" >/dev/null 2>&1 || true
elif [[ "${android_network_policy_mode}" == "strict" ]]; then
  if [[ -n "${package_uid}" ]]; then
    adb shell cmd connectivity set-background-networking-enabled-for-uid "${package_uid}" false \
      >/dev/null 2>&1 || true
  fi
  adb shell cmd deviceidle whitelist -"${package_name}" >/dev/null 2>&1 || true
fi

if [[ "${android_keep_awake}" == "true" ]]; then
  adb shell svc power stayon true >/dev/null 2>&1 || true
  adb shell input keyevent KEYCODE_WAKEUP >/dev/null 2>&1 || true
  adb shell wm dismiss-keyguard >/dev/null 2>&1 || true
elif [[ "${android_keep_awake}" == "false" ]]; then
  adb shell svc power stayon false >/dev/null 2>&1 || true
fi

adb logcat -c
adb shell am force-stop "${package_name}" >/dev/null 2>&1 || true
adb shell am start \
  -n "${activity_name}" \
  --ei channelNumber "${channel_number}" \
  --ez runManagedStreams true \
  --ez managedOnly "${managed_only}" \
  --ez managedValidationTraffic "${validation_traffic}" \
  --ei managedDurationMs "${duration_ms}" \
  --ei managedPayloadCount "${payload_count}" \
  --ei managedPayloadIntervalMs "${payload_interval_ms}" \
  | tee "${log_dir}/am-start.txt"

adb logcat -v time -s WfbUsbSmoke WfbLinkAndroid RustStdoutStderr AndroidRuntime DEBUG \
  >"${log_dir}/logcat.txt" &
logcat_pid=$!

sleep_seconds=$(((duration_ms + 999) / 1000 + 90))
deadline=$((SECONDS + sleep_seconds))
while (( SECONDS < deadline )); do
  if grep -Eq 'Managed-stream (smoke|product-mode) (completed|failed)|Managed-stream smoke SDK error|F/DEBUG|FATAL EXCEPTION|OutOfMemoryError|JNI DETECTED ERROR|SIGABRT' \
    "${log_dir}/logcat.txt" 2>/dev/null; then
    break
  fi
  sleep 1
done

if kill -0 "${logcat_pid}" >/dev/null 2>&1; then
  kill "${logcat_pid}" >/dev/null 2>&1 || true
  wait "${logcat_pid}" >/dev/null 2>&1 || true
fi

adb shell am force-stop "${package_name}" >/dev/null 2>&1 || true

grep -E 'F/DEBUG|AndroidRuntime|OutOfMemoryError|JNI DETECTED ERROR|SIGABRT' \
  "${log_dir}/logcat.txt" >"${log_dir}/crash-lines.txt" || true
grep -E 'Managed-stream (smoke|product-mode) completed' "${log_dir}/logcat.txt" \
  >"${log_dir}/managed-completed.txt" || true
grep -E 'Managed-stream (smoke|product-mode) failed' "${log_dir}/logcat.txt" \
  >"${log_dir}/managed-failed.txt" || true
grep -E 'Managed-stream smoke SDK error' "${log_dir}/logcat.txt" \
  >"${log_dir}/managed-sdk-error.txt" || true

echo "${log_dir}"
