#!/bin/sh
# Build "Rusty Z.app" in target/release/bundle/. Story files found in
# stories/ are copied into the bundle, so the app opens straight into the
# game. Pass --universal for an Apple Silicon + Intel build (needs
# `rustup target add x86_64-apple-darwin`).
set -eu
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
app="target/release/bundle/Rusty Z.app"

if [ "${1:-}" = "--universal" ]; then
    cargo build --release -p rustyz --target aarch64-apple-darwin --target x86_64-apple-darwin
    bin=target/release/rustyz-universal
    lipo -create -output "$bin" \
        target/aarch64-apple-darwin/release/rustyz target/x86_64-apple-darwin/release/rustyz
else
    cargo build --release -p rustyz
    bin=target/release/rustyz
fi

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/stories"
cp "$bin" "$app/Contents/MacOS/rustyz"

# Icon: every size iconutil expects, from the 1024px master.
iconset=target/release/AppIcon.iconset
rm -rf "$iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z $size $size assets/icon.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z $double $double assets/icon.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/AppIcon.icns"

for f in stories/*; do
    [ "$f" = stories/README.md ] || cp "$f" "$app/Contents/Resources/stories/"
done
if [ -z "$(ls "$app/Contents/Resources/stories")" ]; then
    echo "No story file in stories/ to bundle (e.g. copy planetfall.zip there)." >&2
    exit 1
fi

cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Rusty Z</string>
    <key>CFBundleDisplayName</key><string>Rusty Z</string>
    <key>CFBundleIdentifier</key><string>io.github.rustyz</string>
    <key>CFBundleExecutable</key><string>rustyz</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.adventure-games</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>MIT licensed. Story files belong to their owners.</string>
</dict>
</plist>
EOF

# Ad-hoc signature: enough to run locally on Apple Silicon. Distributing to
# other Macs needs a Developer ID signature and notarization instead.
codesign --force --sign - "$app"
echo "Built $app"
