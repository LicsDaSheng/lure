#!/usr/bin/env bash
# 从唯一的 Lure SVG 标志构建可由 Dock / Finder 识别的原生应用包。
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo '此脚本仅支持 macOS' >&2
  exit 1
fi
for tool in cargo rsvg-convert sips iconutil; do
  command -v "$tool" >/dev/null || { echo "缺少工具: $tool" >&2; exit 1; }
done

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
cargo build --release -p lure-desktop
bundle="$root/target/release/Lure.app"
if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
  bundle="${CARGO_TARGET_DIR%/}/release/Lure.app"
fi
contents="$bundle/Contents"
mkdir -p "$contents/MacOS" "$contents/Resources"
cp packaging/macos/Info.plist "$contents/Info.plist"
cp "${bundle%/Lure.app}/lure-desktop" "$contents/MacOS/lure-desktop"

icons="$(mktemp -d)/Lure.iconset"
mkdir -p "$icons"
trap 'rm -rf "${icons%/Lure.iconset}"' EXIT
for size in 16 32 128 256 512; do
  rsvg-convert -w "$size" -h "$size" assets/logo.svg -o "$icons/icon_${size}x${size}.png"
done
for size in 16 32 128 256 512; do
  double=$((size * 2))
  rsvg-convert -w "$double" -h "$double" assets/logo.svg -o "$icons/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$icons" -o "$contents/Resources/Lure.icns"
echo "已生成 $bundle"
