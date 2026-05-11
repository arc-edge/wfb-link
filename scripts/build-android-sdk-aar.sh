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
work_dir="${repo_root}/target/android-sdk-aar"
aar_root="${work_dir}/aarroot"
resources_apk="${work_dir}/resources.apk"
aar_path="${work_dir}/wfb-link-android-sdk-debug.aar"
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

"${repo_root}/scripts/build-android-smoke.sh" build
require_file "${native_so}" "Android SDK native library"

if [[ "${include_helpers}" == "1" ]]; then
  "${repo_root}/scripts/build-wfb-ng-android-codec.sh"
fi

rm -rf "${work_dir}/compiled-res" "${work_dir}/gen" "${work_dir}/classes" "${aar_root}" "${resources_apk}" "${aar_path}"
mkdir -p \
  "${work_dir}/compiled-res" \
  "${work_dir}/gen" \
  "${work_dir}/classes" \
  "${aar_root}/jni/arm64-v8a"

"${build_tools}/aapt2" compile \
  --dir "${repo_root}/android/sdk/src/main/res" \
  -o "${work_dir}/compiled-res/resources.zip"

"${build_tools}/aapt2" link \
  --static-lib \
  -I "${android_jar}" \
  --manifest "${repo_root}/android/sdk/src/main/AndroidManifest.xml" \
  --java "${work_dir}/gen" \
  --min-sdk-version "${min_api}" \
  --target-sdk-version "${target_api}" \
  --output-text-symbols "${aar_root}/R.txt" \
  -o "${resources_apk}" \
  "${work_dir}/compiled-res/resources.zip"

cp "${repo_root}/android/sdk/src/main/AndroidManifest.xml" "${aar_root}/AndroidManifest.xml"
mkdir -p "${aar_root}/res"
cp -R "${repo_root}/android/sdk/src/main/res/." "${aar_root}/res/"

javac \
  --release 8 \
  -classpath "${android_jar}" \
  -d "${work_dir}/classes" \
  $(find "${repo_root}/android/sdk/src/main/java" -name '*.java' | sort)

(
  cd "${work_dir}/classes"
  jar cf "${aar_root}/classes.jar" .
)

cp "${native_so}" "${aar_root}/jni/arm64-v8a/libwfb_android.so"
if [[ -x "${helper_dir}/wfb_tx" && -x "${helper_dir}/wfb_rx" ]]; then
  cp "${helper_dir}/wfb_tx" "${aar_root}/jni/arm64-v8a/libwfb_tx_exec.so"
  cp "${helper_dir}/wfb_rx" "${aar_root}/jni/arm64-v8a/libwfb_rx_exec.so"
  if [[ -x "${helper_dir}/wfb_keygen" ]]; then
    cp "${helper_dir}/wfb_keygen" "${aar_root}/jni/arm64-v8a/libwfb_keygen_exec.so"
  fi
else
  if [[ "${include_helpers}" == "1" ]]; then
    echo "error: Android WFB helper binaries were requested but not found in ${helper_dir}" >&2
    exit 2
  fi
  echo "warning: Android WFB helper binaries not packaged; run INCLUDE_ANDROID_WFB_HELPERS=1 $0 to include them" >&2
fi

(
  cd "${aar_root}"
  zip -qr "${aar_path}" .
)

echo "${aar_path}"
