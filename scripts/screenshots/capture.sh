#!/usr/bin/env bash
# Captures README / landing page screenshots from the demo harness (mocked IPC, demo data).
# Usage: scripts/screenshots/capture.sh   (needs `npm run dev` running and Google Chrome/Chromium)
set -euo pipefail
cd "$(dirname "$0")/../.."
chrome=$(command -v google-chrome || command -v chromium || command -v chromium-browser)
out=assets/screenshots
mkdir -p "$out"
for page in updates apps settings; do
  "$chrome" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=2 \
    --window-size=900,680 --virtual-time-budget=4000 \
    --screenshot="$out/$page.png" "http://localhost:1420/scripts/screenshots/demo.html?page=$page" 2>/dev/null
  echo "$out/$page.png"
done
"$chrome" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=1 \
  --window-size=1280,640 --virtual-time-budget=2000 \
  --screenshot=assets/social-preview.png "http://localhost:1420/scripts/screenshots/social.html" 2>/dev/null
echo assets/social-preview.png
