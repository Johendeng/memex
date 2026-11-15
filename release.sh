#!/usr/bin/env bash
# Memex release script — build, package, upload, tag, push in one command.
#
# Usage:
#   ./release.sh <version>              # e.g. ./release.sh 1.3.0
#   ./release.sh <version> --local      # build only, no upload/tag/push
#
# Prereqs (checked before doing anything):
#   - project root has models/ (bge-small-zh + ocr) — ISCC packs them
#   - DLLs (onnxruntime/pdfium/WebView2Loader) beside target/release/memex.exe
#   - GITEE_TOKEN env var for Release upload (prompted if missing)
set -euo pipefail

VER="${1:?usage: ./release.sh <version> [--local]}"
LOCAL_ONLY="${2:-}"
ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

EXE="target/release/memex.exe"
OUT="installer/Output"
GITEE_OWNER="JohenTeng"
GITHUB_OWNER="Johendeng"
REPO="memex"

echo "==> [1/6] preflight"
for f in models/bge-small-zh-v1.5/onnx/model.onnx models/ocr; do
  [ -e "$f" ] || { echo "MISSING: $f (ISCC needs models/ in project root)"; exit 1; }
done
for dll in onnxruntime.dll pdfium.dll WebView2Loader.dll; do
  [ -e "target/release/$dll" ] || { echo "MISSING: target/release/$dll"; exit 1; }
done
grep -q "version = \"$VER\"" Cargo.toml || {
  echo "Cargo.toml version != $VER — update Cargo.toml / memex.iss / start.bat first"
  exit 1
}

echo "==> [2/6] frontend build"
( cd frontend && npm run build )

echo "==> [3/6] cargo release build (~6 min)"
MSYS_NO_PATHCONV=1 taskkill //F //IM memex.exe 2>/dev/null || true
sleep 3
cargo build --release --features "pdfium ocr"

echo "==> [4/6] Inno Setup"
"C:/Users/10784/AppData/Local/Programs/Inno Setup 6/ISCC.exe" installer/memex.iss
SETUP="Memex-Setup-$VER.exe"
[ -e "$OUT/$SETUP" ] || { echo "installer output not found: $OUT/$SETUP"; exit 1; }
ls -la "$OUT/$SETUP"

if [ "$LOCAL_ONLY" = "--local" ]; then
  echo "==> done (local build only): $OUT/$SETUP"
  exit 0
fi

echo "==> [5/6] upload to Gitee + GitHub releases"
: "${GITEE_TOKEN:?GITEE_TOKEN env var required for release upload}"
SIZE=$(stat -c%s "$OUT/$SETUP")
# Gitee: create release then attach asset (form-data)
REL_ID=$(curl -s -X POST "https://gitee.com/api/v5/repos/$GITEE_OWNER/$REPO/releases" \
  -H "Content-Type: application/json;charset=UTF-8" \
  -d "{\"access_token\":\"$GITEE_TOKEN\",\"tag_name\":\"v$VER\",\"name\":\"Memex v$VER\",\"body\":\"See CHANGELOG / commit log for details.\",\"target_commitish\":\"master\",\"prerelease\":false}" \
  | python -c 'import sys,json;print(json.load(sys.stdin)["id"])')
curl -s -X POST "https://gitee.com/api/v5/repos/$GITEE_OWNER/$REPO/releases/$REL_ID/attach_files" \
  -F "access_token=$GITEE_TOKEN" -F "file=@$OUT/$SETUP" > /dev/null
echo "    gitee release $REL_ID created"
# GitHub: create release with asset (needs gh auth)
if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
  gh release create "v$VER" "$OUT/$SETUP" \
    --repo "$GITHUB_OWNER/$REPO" \
    --title "Memex v$VER" --notes "See CHANGELOG / commit log for details." || true
  echo "    github release created"
else
  echo "    gh CLI not authed — skipping GitHub upload"
fi

echo "==> [6/6] commit, tag, push"
git add -A
git commit -m "release: v$VER" || true
git tag "v$VER"
echo "    done. push when ready:  git push && git push --tags"
echo "    (push intentionally left manual — slow network)"
