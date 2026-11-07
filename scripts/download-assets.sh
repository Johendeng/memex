#!/usr/bin/env bash
# Download external runtime files for Memex PDF + OCR features.
# Run this after building with: cargo build --release --features "pdfium ocr"
#
# Files downloaded:
#   1. pdfium.dll  (~7MB)  → placed next to memex.exe
#   2. PP-OCRv6 Tiny model files (~6MB total) → models/ocr/
#
# Usage: bash download-assets.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR/.."

echo "=== Memex Asset Downloader ==="
echo ""

# --- 1. pdfium.dll ---
echo "[1/2] Downloading pdfium.dll..."
TGZ="pdfium-win-x64.tgz"

# Get latest release tag from GitHub API
TAG=$(curl -sL "https://api.github.com/repos/bblanchon/pdfium-binaries/releases/latest" | python3 -c "import json,sys;print(json.load(sys.stdin)['tag_name'])" 2>/dev/null)
if [ -z "$TAG" ]; then
    TAG="chromium/8138"
fi
echo "  Latest pdfium tag: $TAG"
URL="https://github.com/bblanchon/pdfium-binaries/releases/download/${TAG}/pdfium-win-x64.tgz"

if curl -L --insecure --connect-timeout 15 --max-time 300 -o "/tmp/${TGZ}" "$URL"; then
    echo "  Extracting pdfium.dll..."
    mkdir -p /tmp/pdfium-extract
    tar xzf "/tmp/${TGZ}" -C /tmp/pdfium-extract
    cp /tmp/pdfium-extract/bin/pdfium.dll target/release/
    echo "  ✅ pdfium.dll → target/release/pdfium.dll"
else
    echo "  ❌ Failed to download pdfium.dll."
    echo "     Manual: https://github.com/bblanchon/pdfium-binaries/releases"
    echo "     Download pdfium-win-x64.tgz, extract bin/pdfium.dll to target/release/"
fi

# --- 2. OCR Model files (PP-OCRv6 Tiny) ---
echo ""
echo "[2/2] Downloading PP-OCRv6 Tiny model files..."
OCR_DIR="models/ocr"
mkdir -p "$OCR_DIR"

# Download from GitHub releases (models-rten-v0.3.2 tag)
MODEL_URL="https://github.com/byrizki/rusto-rs/releases/download/models-rten-v0.3.2/rusto-model-ppocrv6-tiny-0.3.2.tgz"

if curl -L --insecure --connect-timeout 15 --max-time 120 -o "/tmp/ocr-model.tgz" "$MODEL_URL"; then
    echo "  Extracting model files..."
    mkdir -p /tmp/ocr-extract
    tar xzf "/tmp/ocr-model.tgz" -C /tmp/ocr-extract
    cp /tmp/ocr-extract/package/models/det.rten "$OCR_DIR/"
    cp /tmp/ocr-extract/package/models/rec.rten "$OCR_DIR/"
    cp /tmp/ocr-extract/package/models/dict.txt "$OCR_DIR/"
    echo "  ✅ det.rten → $OCR_DIR/det.rten"
    echo "  ✅ rec.rten → $OCR_DIR/rec.rten"
    echo "  ✅ dict.txt → $OCR_DIR/dict.txt"
else
    echo "  ❌ Failed to download OCR models."
    echo "     Manual: https://github.com/byrizki/rusto-rs/releases/tag/models-rten-v0.3.2"
    echo "     Download rusto-model-ppocrv6-tiny-0.3.2.tgz, extract to models/ocr/"
fi

# Verify
echo ""
echo "=== Verification ==="
if [ -f "target/release/pdfium.dll" ]; then
    echo "✅ pdfium.dll present"
else
    echo "❌ pdfium.dll missing"
fi

MODEL_COUNT=$(ls "$OCR_DIR"/*.rten "$OCR_DIR"/dict.txt 2>/dev/null | wc -l)
if [ "$MODEL_COUNT" -eq 3 ]; then
    echo "✅ OCR model files complete (det.rten + rec.rten + dict.txt)"
else
    echo "❌ OCR model files incomplete ($MODEL_COUNT/3)"
fi

echo ""
echo "Done. Now rebuild the installer:"
echo "  ISCC.exe installer/memex.iss"
