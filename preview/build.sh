#!/usr/bin/env bash
# Regenerate the marketplace previews from live Omarchy captures.
#
# Usage: preview/build.sh <news.png> <reader.png>
#
# Both are full-screen 1920x1080 shots with the Gazette panel open and the
# widget in the bar's centre section, so the panel sits at x=672 y=32.
# Needs: chromium, ImageMagick.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(dirname "$here")
work="$here/.work"

news=${1:?full-screen capture of the article list}
reader=${2:?full-screen capture of the in-panel reader}

command -v chromium >/dev/null || { echo "chromium is required" >&2; exit 1; }
command -v magick >/dev/null || { echo "ImageMagick (magick) is required" >&2; exit 1; }

mkdir -p "$work"
trap '[[ -f $work/server.pid ]] && kill "$(cat "$work/server.pid")" 2>/dev/null || true' EXIT

if [[ ! -f $work/wallpaper.jpg ]]; then
  cp "$HOME/.local/state/omarchy/current/theme/backgrounds/1-cosmic.jpg" "$work/wallpaper.jpg"
fi

# 700x910 around the bar and the panel.
magick "$news" -crop 700x910+652+0 +repage "$work/shot.png"
cp "$here/poster.html" "$work/poster.html"

port=${PREVIEW_PORT:-8793}
pushd "$work" >/dev/null
python3 -m http.server "$port" --bind 127.0.0.1 >/dev/null 2>&1 &
echo $! > server.pid
popd >/dev/null
sleep 0.5

cd "$work"
chromium --headless=new --disable-gpu --hide-scrollbars \
  --force-device-scale-factor=2 --window-size=1600,1000 \
  --virtual-time-budget=4000 --screenshot=poster2x.png \
  http://127.0.0.1:"$port"/poster.html >/dev/null 2>&1

magick poster2x.png -resize 1600x1000 -strip -define png:compression-level=9 "$repo/preview.png"
magick "$reader" -crop 700x910+652+0 +repage -strip -define png:compression-level=9 "$repo/preview-reader.png"
echo "wrote preview.png, preview-reader.png"
