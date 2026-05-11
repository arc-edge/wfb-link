#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
sdk_root="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-${HOME}/Library/Android/sdk}}"
platform="${ANDROID_PLATFORM:-android-35}"
android_jar="${sdk_root}/platforms/${platform}/android.jar"
work_dir="${repo_root}/target/android-sdk-consumer-smoke"
aar_path="${ANDROID_WFB_LINK_AAR:-${repo_root}/target/android-sdk-aar/wfb-link-android-sdk-debug.aar}"

require_file() {
  local path="$1"
  local description="$2"
  if [[ ! -e "${path}" ]]; then
    echo "${description} not found at ${path}" >&2
    exit 2
  fi
}

require_file "${android_jar}" "Android platform jar"
if [[ ! -f "${aar_path}" ]]; then
  "${repo_root}/scripts/build-android-sdk-aar.sh"
fi
require_file "${aar_path}" "WFB Link Android SDK AAR"

rm -rf "${work_dir}"
mkdir -p "${work_dir}/aar" "${work_dir}/classes"
unzip -q "${aar_path}" classes.jar -d "${work_dir}/aar"
require_file "${work_dir}/aar/classes.jar" "SDK classes.jar"

javac \
  --release 8 \
  -classpath "${android_jar}:${work_dir}/aar/classes.jar" \
  -d "${work_dir}/classes" \
  $(find "${repo_root}/android/sdk-consumer/src/main/java" -name '*.java' | sort)

echo "${work_dir}/classes"
