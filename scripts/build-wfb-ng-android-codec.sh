#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"

android_api="${ANDROID_API:-24}"
arch="${ANDROID_ARCH:-arm64-v8a}"

if [[ "${arch}" != "arm64-v8a" ]]; then
  echo "error: only ANDROID_ARCH=arm64-v8a is currently supported" >&2
  exit 2
fi

sdk_root="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-$HOME/Library/Android/sdk}}"
if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
  ndk_home="$(find "${sdk_root}/ndk" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort -V | tail -n 1 || true)"
else
  ndk_home="${ANDROID_NDK_HOME}"
fi

if [[ -z "${ndk_home:-}" || ! -d "${ndk_home}" ]]; then
  echo "error: Android NDK not found; set ANDROID_NDK_HOME or ANDROID_SDK_ROOT" >&2
  exit 2
fi

host_tag="darwin-x86_64"
toolchain="${ndk_home}/toolchains/llvm/prebuilt/${host_tag}/bin"
if [[ ! -d "${toolchain}" ]]; then
  host_tag="darwin-arm64"
  toolchain="${ndk_home}/toolchains/llvm/prebuilt/${host_tag}/bin"
fi
if [[ ! -d "${toolchain}" ]]; then
  echo "error: unsupported NDK host toolchain under ${ndk_home}" >&2
  exit 2
fi

cc="${toolchain}/aarch64-linux-android${android_api}-clang"
cxx="${toolchain}/aarch64-linux-android${android_api}-clang++"
ar="${toolchain}/llvm-ar"
ranlib="${toolchain}/llvm-ranlib"
strip="${toolchain}/llvm-strip"

wfb_src="${WFB_NG_SRC:-/tmp/wfb-ng-android-src}"
sodium_src="${LIBSODIUM_SRC:-/tmp/libsodium-android-src}"
out_dir="${OUT_DIR:-${repo_root}/target/wfb-ng-android}"
sodium_prefix="${out_dir}/deps/libsodium-${arch}"
bin_dir="${out_dir}/bin"
build_dir="${out_dir}/build"
compat_dir="${repo_root}/tools/wfb-ng-android-compat"

jobs="${JOBS:-$(sysctl -n hw.ncpu 2>/dev/null || echo 4)}"

ensure_git_checkout() {
  local url="$1"
  local dir="$2"
  local ref="${3:-}"
  if [[ ! -d "${dir}/.git" ]] || ! git -C "${dir}" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    rm -rf "${dir}"
    if [[ -n "${ref}" ]]; then
      git clone --depth 1 --branch "${ref}" "${url}" "${dir}"
    else
      git clone --depth 1 "${url}" "${dir}"
    fi
  fi
}

ensure_git_checkout "https://github.com/svpcom/wfb-ng" "${wfb_src}" "${WFB_NG_REF:-}"
ensure_git_checkout "https://github.com/jedisct1/libsodium" "${sodium_src}" "${LIBSODIUM_REF:-stable}"

mkdir -p "${build_dir}" "${bin_dir}"

if [[ ! -f "${sodium_prefix}/lib/libsodium.a" ]]; then
  echo "Building Android libsodium in ${sodium_prefix}"
  if [[ ! -x "${sodium_src}/configure" ]]; then
    (cd "${sodium_src}" && ./autogen.sh)
  fi
  (
    cd "${sodium_src}"
    make distclean >/dev/null 2>&1 || true
    CC="${cc}" AR="${ar}" RANLIB="${ranlib}" ./configure \
      --host=aarch64-linux-android \
      --prefix="${sodium_prefix}" \
      --disable-shared \
      --enable-static \
      --with-pic
    make -j"${jobs}"
    make install
  )
fi

"${cc}" -I"${compat_dir}" -fPIE -c "${compat_dir}/pcap_stub.c" -o "${build_dir}/pcap_stub.o"
"${ar}" rcs "${build_dir}/libpcap.a" "${build_dir}/pcap_stub.o"
"${ranlib}" "${build_dir}/libpcap.a"

common_cflags="-I${compat_dir} -I${sodium_prefix}/include -fPIE -DANDROID -D__ANDROID__"
common_ldflags="-fPIE -pie -static-libstdc++"
extra_libs="${common_ldflags} -L${build_dir} ${sodium_prefix}/lib/libsodium.a"

echo "Building Android wfb-ng codec helpers from ${wfb_src}"
make -C "${wfb_src}" clean >/dev/null 2>&1 || true
make -C "${wfb_src}" -j"${jobs}" wfb_tx wfb_rx wfb_keygen \
  CC="${cc}" \
  CXX="${cxx}" \
  AR="${ar}" \
  CFLAGS="${common_cflags} ${CFLAGS:-}" \
  LDFLAGS="${common_ldflags} ${LDFLAGS:-}" \
  _LDFLAGS="${extra_libs}"

cp "${wfb_src}/wfb_tx" "${bin_dir}/wfb_tx"
cp "${wfb_src}/wfb_rx" "${bin_dir}/wfb_rx"
cp "${wfb_src}/wfb_keygen" "${bin_dir}/wfb_keygen"
chmod 0755 "${bin_dir}/wfb_tx" "${bin_dir}/wfb_rx" "${bin_dir}/wfb_keygen"
"${strip}" "${bin_dir}/wfb_tx" "${bin_dir}/wfb_rx" "${bin_dir}/wfb_keygen" || true

echo "Android codec helpers:"
ls -lh "${bin_dir}/wfb_tx" "${bin_dir}/wfb_rx" "${bin_dir}/wfb_keygen"
