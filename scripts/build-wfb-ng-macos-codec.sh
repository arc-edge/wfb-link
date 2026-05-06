#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)

WFB_NG_REPO=${WFB_NG_REPO:-https://github.com/svpcom/wfb-ng}
WFB_NG_REF=${WFB_NG_REF:-master}
WFB_NG_SRC=${WFB_NG_SRC:-/tmp/wfb-ng-src}
OUT_DIR=${OUT_DIR:-$REPO_ROOT/target/wfb-ng-macos/bin}
COMPAT_DIR=${COMPAT_DIR:-$REPO_ROOT/tools/wfb-ng-macos-compat}

if [[ ! -d "$WFB_NG_SRC/.git" ]]; then
  git clone --depth 1 --branch "$WFB_NG_REF" "$WFB_NG_REPO" "$WFB_NG_SRC"
else
  git -C "$WFB_NG_SRC" fetch --depth 1 origin "$WFB_NG_REF"
  git -C "$WFB_NG_SRC" checkout --detach FETCH_HEAD
fi

if [[ -r /opt/homebrew/opt/libsodium/lib/libsodium.a ]]; then
  sodium_cflags="-I/opt/homebrew/opt/libsodium/include"
  codec_ldflags="/opt/homebrew/opt/libsodium/lib/libsodium.a"
elif pkg-config --exists libsodium; then
  sodium_cflags="$(pkg-config --cflags libsodium)"
  codec_ldflags="$(pkg-config --libs libsodium)"
else
  echo "libsodium not found. Install with: brew install libsodium" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
make -C "$WFB_NG_SRC" clean >/dev/null 2>&1 || true
make -C "$WFB_NG_SRC" wfb_tx wfb_rx wfb_keygen \
  CFLAGS="-include $COMPAT_DIR/wfb_macos.h -I$COMPAT_DIR $sodium_cflags ${CFLAGS:-}" \
  LDFLAGS="${LDFLAGS:-}" \
  _LDFLAGS="$codec_ldflags"

cp "$WFB_NG_SRC/wfb_tx" "$WFB_NG_SRC/wfb_rx" "$WFB_NG_SRC/wfb_keygen" "$OUT_DIR/"
chmod +x "$OUT_DIR/wfb_tx" "$OUT_DIR/wfb_rx" "$OUT_DIR/wfb_keygen"

echo "Built:"
echo "  $OUT_DIR/wfb_tx"
echo "  $OUT_DIR/wfb_rx"
echo "  $OUT_DIR/wfb_keygen"
