#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
sdk_root="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-${HOME}/Library/Android/sdk}}"
platform="${ANDROID_PLATFORM:-android-35}"
min_api="${ANDROID_MIN_API:-24}"
target_api="${ANDROID_TARGET_API:-35}"
build_tools_version="${ANDROID_BUILD_TOOLS_VERSION:-35.0.0}"
build_tools="${sdk_root}/build-tools/${build_tools_version}"
android_jar="${sdk_root}/platforms/${platform}/android.jar"
work_dir="${repo_root}/target/android-smoke-apk"
apk_name="wfb-link-android-smoke-debug.apk"
unsigned_apk="${work_dir}/smoke-unsigned.apk"
aligned_apk="${work_dir}/smoke-aligned.apk"
signed_apk="${work_dir}/${apk_name}"
keystore="${work_dir}/debug.keystore"
native_so="${repo_root}/target/aarch64-linux-android/release/libwfb_android_smoke.so"
helper_dir="${ANDROID_WFB_HELPER_DIR:-${repo_root}/target/wfb-ng-android/bin}"
include_helpers="${INCLUDE_ANDROID_WFB_HELPERS:-auto}"

require_file() {
  local path="$1"
  local description="$2"
  if [[ ! -e "${path}" ]]; then
    echo "${description} not found at ${path}" >&2
    exit 2
  fi
}

require_file "${android_jar}" "Android platform jar"
require_file "${build_tools}/aapt2" "aapt2"
require_file "${build_tools}/d8" "d8"
require_file "${build_tools}/zipalign" "zipalign"
require_file "${build_tools}/apksigner" "apksigner"

chmod +x "${repo_root}/scripts/build-wfb-ng-android-codec.sh"

"${repo_root}/scripts/build-android-smoke.sh" build
require_file "${native_so}" "Android smoke native library"

if [[ "${include_helpers}" == "1" ]]; then
  "${repo_root}/scripts/build-wfb-ng-android-codec.sh"
fi

rm -rf "${work_dir}/compiled-res" "${work_dir}/gen" "${work_dir}/classes" "${work_dir}/dex" "${work_dir}/apkroot"
mkdir -p "${work_dir}/compiled-res" "${work_dir}/gen" "${work_dir}/classes" "${work_dir}/dex" "${work_dir}/apkroot/lib/arm64-v8a"

"${build_tools}/aapt2" compile \
  --dir "${repo_root}/android/smoke-harness/src/main/res" \
  -o "${work_dir}/compiled-res/resources.zip"

"${build_tools}/aapt2" link \
  -I "${android_jar}" \
  --manifest "${repo_root}/android/smoke-harness/src/main/AndroidManifest.xml" \
  --java "${work_dir}/gen" \
  --min-sdk-version "${min_api}" \
  --target-sdk-version "${target_api}" \
  -o "${unsigned_apk}" \
  "${work_dir}/compiled-res/resources.zip"

javac \
  --release 8 \
  -classpath "${android_jar}" \
  -d "${work_dir}/classes" \
  $(find \
    "${work_dir}/gen" \
    "${repo_root}/android/sdk/src/main/java" \
    "${repo_root}/android/smoke-harness/src/main/java" \
    -name '*.java' | sort)

"${build_tools}/d8" \
  --min-api "${min_api}" \
  --lib "${android_jar}" \
  --output "${work_dir}/dex" \
  $(find "${work_dir}/classes" -name '*.class' | sort)

cp "${native_so}" "${work_dir}/apkroot/lib/arm64-v8a/libwfb_android_smoke.so"
cp "${native_so}" "${work_dir}/apkroot/lib/arm64-v8a/libwfb_android.so"
if [[ -x "${helper_dir}/wfb_tx" && -x "${helper_dir}/wfb_rx" ]]; then
  cp "${helper_dir}/wfb_tx" "${work_dir}/apkroot/lib/arm64-v8a/libwfb_tx_exec.so"
  cp "${helper_dir}/wfb_rx" "${work_dir}/apkroot/lib/arm64-v8a/libwfb_rx_exec.so"
  if [[ -x "${helper_dir}/wfb_keygen" ]]; then
    cp "${helper_dir}/wfb_keygen" "${work_dir}/apkroot/lib/arm64-v8a/libwfb_keygen_exec.so"
  fi
else
  if [[ "${include_helpers}" == "1" ]]; then
    echo "error: Android WFB helper binaries were requested but not found in ${helper_dir}" >&2
    exit 2
  fi
  echo "warning: Android WFB helper binaries not packaged; run INCLUDE_ANDROID_WFB_HELPERS=1 $0 to include them" >&2
fi
cp "${unsigned_apk}" "${work_dir}/smoke-with-code.apk"
(
  cd "${work_dir}/dex"
  zip -q "${work_dir}/smoke-with-code.apk" classes.dex
)
(
  cd "${work_dir}/apkroot"
  zip -qr "${work_dir}/smoke-with-code.apk" lib
)

if [[ ! -f "${keystore}" ]]; then
  keytool -genkeypair \
    -keystore "${keystore}" \
    -storepass android \
    -keypass android \
    -alias androiddebugkey \
    -keyalg RSA \
    -keysize 2048 \
    -validity 10000 \
    -dname "CN=Android Debug,O=Android,C=US" >/dev/null
fi

"${build_tools}/zipalign" -f 4 "${work_dir}/smoke-with-code.apk" "${aligned_apk}"
"${build_tools}/apksigner" sign \
  --ks "${keystore}" \
  --ks-pass pass:android \
  --key-pass pass:android \
  --out "${signed_apk}" \
  "${aligned_apk}"
"${build_tools}/apksigner" verify "${signed_apk}"

echo "${signed_apk}"
