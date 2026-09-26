#!/usr/bin/env bash
# Turns the Mac builds into "Brixo Player.dmg" and "Brixo Studio.dmg": open
# one and drag the app into Applications. Runs on a Mac (the GitHub
# workflow does it: .github/workflows/mac.yml), after building both
# targets:
#   cargo build --release --target aarch64-apple-darwin -p brixo-player -p brixo-studio
#   cargo build --release --target x86_64-apple-darwin  -p brixo-player -p brixo-studio
#   bash tools/package-mac.sh 2026.09.26.0100
set -euo pipefail
VERSION="${1:?usage: package-mac.sh VERSION}"
cd "$(dirname "$0")/.."
OUT=dist-mac
WORK="$OUT/work"
rm -rf "$OUT"
mkdir -p "$WORK"

# The icon, at every size a Mac shows it.
ICONSET="$WORK/Brixo.iconset"
mkdir -p "$ICONSET"
SRC=crates/brixo-client/assets/icon-1024.png
for s in 16 32 128 256 512; do
    sips -z "$s" "$s" "$SRC" --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
    sips -z "$((s * 2))" "$((s * 2))" "$SRC" --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$WORK/Brixo.icns"

# make_app NAME EXE CRATE BUNDLE_ID URL_SCHEME DMG
make_app() {
    local name="$1" exe="$2" crate="$3" id="$4" scheme="$5" dmg="$6"
    local stage="$WORK/$name"
    local app="$stage/$name.app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    # One program that runs on both Apple Silicon and Intel Macs.
    lipo -create \
        "target/aarch64-apple-darwin/release/$crate" \
        "target/x86_64-apple-darwin/release/$crate" \
        -output "$app/Contents/MacOS/$exe"
    chmod +x "$app/Contents/MacOS/$exe"
    cp "$WORK/Brixo.icns" "$app/Contents/Resources/Brixo.icns"

    # Player opens brixo:// links (Play on the website).
    local links=""
    if [ -n "$scheme" ]; then
        links="
    <key>CFBundleURLTypes</key>
    <array>
        <dict>
            <key>CFBundleURLName</key><string>$id.play</string>
            <key>CFBundleURLSchemes</key><array><string>$scheme</string></array>
        </dict>
    </array>"
    fi
    cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>$name</string>
    <key>CFBundleDisplayName</key><string>$name</string>
    <key>CFBundleIdentifier</key><string>$id</string>
    <key>CFBundleExecutable</key><string>$exe</string>
    <key>CFBundleIconFile</key><string>Brixo</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.games</string>
    <key>NSHighResolutionCapable</key><true/>$links
</dict>
</plist>
EOF
    plutil -lint "$app/Contents/Info.plist" >/dev/null

    # Apple Silicon Macs won't run a program with no signature at all. This
    # is a free "ad-hoc" one: enough to run, not Apple's paid check.
    codesign --force --deep --sign - "$app"

    # The window you see when you open the .dmg: the app and a shortcut to
    # Applications to drag it onto.
    ln -s /Applications "$stage/Applications"
    hdiutil create -volname "$name" -srcfolder "$stage" -ov -format UDZO "$OUT/$dmg" >/dev/null
    echo "made $OUT/$dmg"
}

make_app "Brixo Player" BrixoPlayer brixo-player com.playbrixo.player brixo BrixoPlayer.dmg
make_app "Brixo Studio" BrixoStudio brixo-studio com.playbrixo.studio "" BrixoStudio.dmg
