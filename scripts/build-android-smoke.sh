#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target="${ANDROID_RUST_TARGET:-aarch64-linux-android}"
api="${ANDROID_API:-24}"
mode="${1:-build}"
if [[ $# -gt 0 ]]; then
  shift
fi

if [[ "${target}" != "aarch64-linux-android" ]]; then
  echo "unsupported ANDROID_RUST_TARGET=${target}; this smoke script currently supports aarch64-linux-android" >&2
  exit 2
fi

sdk_root="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}"
ndk_root="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
if [[ -z "${ndk_root}" ]]; then
  if [[ -z "${sdk_root}" ]]; then
    sdk_root="${HOME}/Library/Android/sdk"
  fi
  ndk_root="$(find "${sdk_root}/ndk" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort | tail -n 1 || true)"
fi

if [[ -z "${ndk_root}" || ! -d "${ndk_root}" ]]; then
  echo "Android NDK not found; set ANDROID_NDK_HOME or ANDROID_SDK_ROOT" >&2
  exit 2
fi

toolchain_bin="$(find "${ndk_root}/toolchains/llvm/prebuilt" -mindepth 2 -maxdepth 2 -type d -name bin 2>/dev/null | sort | head -n 1 || true)"
if [[ -z "${toolchain_bin}" || ! -x "${toolchain_bin}/${target}${api}-clang" ]]; then
  echo "Android clang not found for ${target} API ${api} under ${ndk_root}" >&2
  exit 2
fi

export CC_aarch64_linux_android="${toolchain_bin}/${target}${api}-clang"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="${toolchain_bin}/${target}${api}-clang"
export AR_aarch64_linux_android="${toolchain_bin}/llvm-ar"

cd "${repo_root}"
case "${mode}" in
  check)
    cargo check -p wfb-android-smoke --target "${target}" --locked "$@"
    ;;
  build)
    cargo build -p wfb-android-smoke --target "${target}" --release --locked "$@"
    ;;
  *)
    echo "usage: $0 [check|build] [cargo args...]" >&2
    exit 2
    ;;
esac
