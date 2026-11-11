#!/usr/bin/env bash
# Cross-platform asset downloader for CI.
# Downloads: onnxruntime, pdfium, bge-small-zh-v1.5 model, OCR models.
# Platform-specific: uses $RUNNER_OS to pick the right binaries.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"

# Detect platform from RUNNER_OS env (set by GitHub Actions)
OS="${RUNNER_OS:-$(uname -s)}"
case "$OS" in
  Linux)   OS="linux" ;;
  Darwin)  OS="macos" ;;
  Windows*) OS="windows" ;;
esac

ARCH="${RUNNER_ARCH:-$(uname -m)}"
case "$ARCH" in
  X64|x86_64)  ARCH="x64" ;;
  ARM64|aarch64) ARCH="arm64" ;;
  X86|i386|i686) ARCH="x86" ;;
esac

echo "=== Memex CI Asset Downloader ==="
echo "  OS=$OS  ARCH=$ARCH"
echo ""

# ─── 1. ONNX Runtime (ort 2.0.0-rc.9 → onnxruntime 1.20.1) ───
ORT_VER="1.20.1"
echo "[1/4] Downloading ONNX Runtime $ORT_VER..."

case "$OS-$ARCH" in
  windows-x64)   ORT_URL="https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VER}/onnxruntime-win-x64-${ORT_VER}.zip"; ORT_LIB="lib/onnxruntime.dll" ;;
  macos-arm64)   ORT_URL="https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VER}/onnxruntime-osx-arm64-${ORT_VER}.tgz"; ORT_LIB="lib/libonnxruntime.dylib" ;;
  macos-x64)     ORT_URL="https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VER}/onnxruntime-osx-x64-${ORT_VER}.tgz"; ORT_LIB="lib/libonnxruntime.dylib" ;;
  linux-x64)     ORT_URL="https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VER}/onnxruntime-linux-x64-${ORT_VER}.tgz"; ORT_LIB="lib/libonnxruntime.so" ;;
  linux-arm64)   ORT_URL="https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VER}/onnxruntime-linux-aarch64-${ORT_VER}.tgz"; ORT_LIB="lib/libonnxruntime.so" ;;
  *) echo "  ⚠ Unsupported: $OS-$ARCH, skipping onnxruntime"; ORT_URL="" ;;
esac

if [ -n "$ORT_URL" ]; then
  mkdir -p /tmp/ort-extract
  if [ "$OS" = "windows" ]; then
    curl -fsSL "$ORT_URL" -o /tmp/ort.zip
    unzip -o /tmp/ort.zip -d /tmp/ort-extract
  else
    curl -fsSL "$ORT_URL" -o /tmp/ort.tgz
    tar xzf /tmp/ort.tgz -C /tmp/ort-extract --strip-components=1
  fi
  # Find the lib file
  ORT_LIB_PATH=$(find /tmp/ort-extract -name "$(basename $ORT_LIB)" | head -1)
  if [ -n "$ORT_LIB_PATH" ]; then
    mkdir -p target/release
    cp "$ORT_LIB_PATH" target/release/
    echo "  ✅ $(basename $ORT_LIB) → target/release/"
  else
    echo "  ❌ onnxruntime lib not found in archive"
  fi
fi

# ─── 2. pdfium ───
echo ""
echo "[2/4] Downloading pdfium..."

# Get latest pdfium tag
PY="python3"
command -v python3 >/dev/null 2>&1 || PY="python"
PDF_TAG=$(curl -sL "https://api.github.com/repos/bblanchon/pdfium-binaries/releases/latest" \
  | $PY -c "import json,sys;print(json.load(sys.stdin)['tag_name'])" 2>/dev/null || echo "chromium-8138")
echo "  Latest pdfium tag: $PDF_TAG"

case "$OS-$ARCH" in
  windows-x64)  PDF_PKG="pdfium-win-x64.tgz" ;;
  macos-arm64)  PDF_PKG="pdfium-macos-arm64.tgz" ;;
  macos-x64)    PDF_PKG="pdfium-macos-x64.tgz" ;;
  linux-x64)    PDF_PKG="pdfium-linux-x64.tgz" ;;
  linux-arm64)  PDF_PKG="pdfium-linux-arm64.tgz" ;;
  *) echo "  ⚠ Unsupported: $OS-$ARCH, skipping pdfium"; PDF_PKG="" ;;
esac

if [ -n "$PDF_PKG" ]; then
  PDF_URL="https://github.com/bblanchon/pdfium-binaries/releases/download/${PDF_TAG}/${PDF_PKG}"
  mkdir -p /tmp/pdfium-extract
  if curl -fsSL "$PDF_URL" -o /tmp/pdfium.tgz; then
    tar xzf /tmp/pdfium.tgz -C /tmp/pdfium-extract
    # pdfium libs are in bin/ (windows .dll, linux .so) or lib/ (macos .dylib)
    PDF_LIB=$(find /tmp/pdfium-extract -name "*.dll" -o -name "*.so" -o -name "*.dylib" | head -1)
    if [ -n "$PDF_LIB" ]; then
      cp "$PDF_LIB" target/release/
      echo "  ✅ $(basename $PDF_LIB) → target/release/"
    else
      echo "  ❌ pdfium lib not found in archive"
    fi
  else
    echo "  ❌ Failed to download pdfium"
  fi
fi

# ─── 3. bge-small-zh-v1.5 embedding model (~91MB) ───
echo ""
echo "[3/4] Downloading bge-small-zh-v1.5 model..."

BGE_DIR="models/bge-small-zh-v1.5"
mkdir -p "$BGE_DIR/onnx"
BASE="https://hf-mirror.com/Xenova/bge-small-zh-v1.5/resolve/main"

curl -fsSL "$BASE/onnx/model.onnx" -o "$BGE_DIR/onnx/model.onnx"
for f in tokenizer.json config.json special_tokens_map.json tokenizer_config.json; do
  curl -fsSL "$BASE/$f" -o "$BGE_DIR/$f"
done
echo "  ✅ bge-small-zh-v1.5 → models/bge-small-zh-v1.5/"

# ─── 4. OCR models (PP-OCRv6 Tiny, ~6MB) ───
echo ""
echo "[4/4] Downloading PP-OCRv6 Tiny OCR models..."

OCR_DIR="models/ocr"
mkdir -p "$OCR_DIR"
MODEL_URL="https://github.com/byrizki/rusto-rs/releases/download/models-rten-v0.3.2/rusto-model-ppocrv6-tiny-0.3.2.tgz"

curl -fsSL "$MODEL_URL" -o /tmp/ocr-model.tgz
mkdir -p /tmp/ocr-extract
tar xzf /tmp/ocr-model.tgz -C /tmp/ocr-extract
cp /tmp/ocr-extract/package/models/det.rten "$OCR_DIR/"
cp /tmp/ocr-extract/package/models/rec.rten "$OCR_DIR/"
cp /tmp/ocr-extract/package/models/dict.txt "$OCR_DIR/"
echo "  ✅ det.rten + rec.rten + dict.txt → models/ocr/"

echo ""
echo "=== Done ==="
