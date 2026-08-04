#!/usr/bin/env bash
# hosted_to_webp.sh <版本目錄>
# 把 out/hosted/<版本>/ 下 sheets/ 與 textures/ 的 PNG 平行轉成 lossless WebP，
# 成功後刪除原 PNG。需要 cwebp（Windows 可用 choco install webp）。
set -euo pipefail

DIR="${1:?用法: hosted_to_webp.sh out/hosted/vYYYYMMDD}"

if ! command -v cwebp >/dev/null 2>&1; then
  echo "找不到 cwebp，請先安裝（choco install webp）" >&2
  exit 1
fi

convert_one() {
  f="$1"
  out="${f%.png}.webp"
  if cwebp -quiet -lossless "$f" -o "$out"; then
    rm "$f"
  else
    echo "轉檔失敗: $f" >&2
  fi
}
export -f convert_one

find "$DIR/sheets" "$DIR/textures" -name '*.png' -print0 |
  xargs -0 -P "$(nproc)" -I{} bash -c 'convert_one "$@"' _ {}

echo "完成。剩餘 PNG 數（應為 0）："
find "$DIR/sheets" "$DIR/textures" -name '*.png' | wc -l
