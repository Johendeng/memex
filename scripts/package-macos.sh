#!/usr/bin/env bash
set -euo pipefail

# Package Memex into a macOS .app bundle
# Expects: target/release/memex (universal binary), assets/memex_icon.icns

VERSION="1.1.0"
APP="dist/Memex.app"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
mkdir -p "$APP/Contents/Resources"

# Copy binary
cp target/release/memex "$APP/Contents/MacOS/memex"
chmod +x "$APP/Contents/MacOS/memex"

# Copy icon
if [ -f assets/memex_icon.icns ]; then
  cp assets/memex_icon.icns "$APP/Contents/Resources/memex_icon.icns"
elif [ -f assets/memex_icon.png ]; then
  cp assets/memex_icon.png "$APP/Contents/Resources/memex_icon.png"
fi

# Info.plist
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>Memex</string>
  <key>CFBundleDisplayName</key>
  <string>Memex</string>
  <key>CFBundleIdentifier</key>
  <string>com.johendeng.memex</string>
  <key>CFBundleVersion</key>
  <string>${VERSION}</string>
  <key>CFBundleShortVersionString</key>
  <string>${VERSION}</string>
  <key>CFBundleExecutable</key>
  <string>memex</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleIconFile</key>
  <string>memex_icon</string>
  <key>LSMinimumSystemVersion</key>
  <string>11.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSAppTransportSecurity</key>
  <dict>
    <key>NSAllowsLocalNetworking</key>
    <true/>
  </dict>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key>
      <string>Folder</string>
      <key>CFBundleTypeRole</key>
      <string>Viewer</string>
      <key>LSItemContentTypes</key>
      <array>
        <string>public.folder</string>
      </array>
    </dict>
  </array>
</dict>
</plist>
PLIST

# Copy runtime dylibs (onnxruntime, pdfium) next to binary
for lib in target/release/libonnxruntime.dylib target/release/libpdfium.dylib; do
  if [ -f "$lib" ]; then
    cp "$lib" "$APP/Contents/MacOS/"
    echo "  ✅ $(basename $lib) → Contents/MacOS/"
  fi
done

# Copy models directory (bge-small-zh-v1.5 + ocr)
if [ -d "models" ]; then
  cp -r models "$APP/Contents/MacOS/models"
  echo "  ✅ models/ → Contents/MacOS/models/"
fi

# Create data directory (app will populate data/ next to binary)
mkdir -p "$APP/Contents/MacOS/data"

echo "✅ Created $APP"
ls -la "$APP/Contents/MacOS/"
echo "Bundle size: $(du -sh "$APP" | cut -f1)"
