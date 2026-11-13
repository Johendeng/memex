#!/usr/bin/env bash
set -euo pipefail

# Package Memex into a Linux AppImage
# Expects: target/release/memex (x86_64 binary), assets/memex_icon.png

APP_NAME="Memex"
APPDIR="dist/AppDir"
BIN="target/release/memex"
ICON="assets/memex_icon.png"

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin"
mkdir -p "$APPDIR/usr/share/applications"
mkdir -p "$APPDIR/usr/share/icons/hicolor/256x256/apps"

# Copy binary
cp "$BIN" "$APPDIR/usr/bin/memex"
chmod +x "$APPDIR/usr/bin/memex"

# Copy runtime shared libraries (onnxruntime, pdfium) next to binary
for lib in target/release/libonnxruntime.so target/release/libpdfium.so; do
  if [ -f "$lib" ]; then
    cp "$lib" "$APPDIR/usr/bin/"
    echo "  ✅ $(basename $lib) → usr/bin/"
  fi
done

# Copy models directory
if [ -d "models" ]; then
  cp -r models "$APPDIR/usr/bin/models"
  echo "  ✅ models/ → usr/bin/models/"
fi

# Copy desktop entry
cp scripts/memex.desktop "$APPDIR/usr/share/applications/memex.desktop"

# Copy icon
cp "$ICON" "$APPDIR/usr/share/icons/hicolor/256x256/apps/memex_icon.png"

# Create AppRun (custom entry point)
cat > "$APPDIR/AppRun" <<'APPRUN'
#!/usr/bin/env bash
HERE="$(dirname "$(readlink -f "${0}")")"
exec "${HERE}/usr/bin/memex" "$@"
APPRUN
chmod +x "$APPDIR/AppRun"

# Symlink desktop file + icon at root (AppImage convention)
ln -sf usr/share/applications/memex.desktop "$APPDIR/memex.desktop"
ln -sf usr/share/icons/hicolor/256x256/apps/memex_icon.png "$APPDIR/memex_icon.png"

# Download linuxdeploy
echo "Downloading linuxdeploy..."
LINUXDEPLOY_URL="https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage"
curl -fsSL "$LINUXDEPLOY_URL" -o /tmp/linuxdeploy
chmod +x /tmp/linuxdeploy

# Download appimagetool
echo "Downloading appimagetool..."
APPIMAGETOOL_URL="https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage"
curl -fsSL "$APPIMAGETOOL_URL" -o /tmp/appimagetool
chmod +x /tmp/appimagetool

# Set output name
export OUTPUT="Memex-x86_64.AppImage"
export VERSION="${VERSION:-1.2.0}"

# AppImage tools need this on CI (no FUSE available)
export APPIMAGE_EXTRACT_AND_RUN=1
export NO_STRIP=true

# Use linuxdeploy to bundle dependencies, then appimagetool to create final AppImage
# linuxdeploy with --appdir and --plugin gtk bundles WebKitGTK deps automatically
echo "Bundling dependencies with linuxdeploy..."
/tmp/linuxdeploy \
  --appdir "$APPDIR" \
  --desktop-file "$APPDIR/usr/share/applications/memex.desktop" \
  --icon-file "$APPDIR/usr/share/icons/hicolor/256x256/apps/memex_icon.png" \
  --output appimage

# Move AppImage to dist/
mkdir -p dist
if [ -f "Memex-x86_64.AppImage" ]; then
  mv "Memex-x86_64.AppImage" dist/
fi

echo "✅ Created dist/Memex-x86_64.AppImage"
ls -lh dist/Memex-x86_64.AppImage
