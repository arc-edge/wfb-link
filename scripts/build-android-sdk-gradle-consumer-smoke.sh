#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
sdk_root="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-${HOME}/Library/Android/sdk}}"
platform="${ANDROID_PLATFORM:-android-35}"
android_jar="${sdk_root}/platforms/${platform}/android.jar"
sample_root="${repo_root}/android/sdk-gradle-consumer"
sample_java="${sample_root}/app/src/main/java"
work_dir="${repo_root}/target/android-sdk-gradle-consumer-smoke"
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

if rg -n 'com\.arcedge\.wfblink\.smoke' "${sample_root}" >/dev/null; then
  echo "Gradle consumer sample must not import the smoke harness package" >&2
  exit 2
fi

rm -rf "${work_dir}"
mkdir -p "${work_dir}/aar" "${work_dir}/classes"
unzip -q "${aar_path}" classes.jar -d "${work_dir}/aar"
require_file "${work_dir}/aar/classes.jar" "SDK classes.jar"

javac \
  --release 8 \
  -classpath "${android_jar}:${work_dir}/aar/classes.jar" \
  -d "${work_dir}/classes" \
  $(find "${sample_java}" -name '*.java' | sort)

if [[ "${RUN_GRADLE_ANDROID_SAMPLE:-0}" == "1" ]]; then
  mkdir -p "${sample_root}/app/libs"
  cp "${aar_path}" "${sample_root}/app/libs/wfb-link-android-sdk-debug.aar"
  (
    cd "${sample_root}"
    if [[ -x ./gradlew ]]; then
      ./gradlew :app:assembleDebug
    else
      gradle :app:assembleDebug
    fi
  )
fi

echo "${work_dir}/classes"
