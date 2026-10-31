#!/bin/sh
# Build "Planetfall.app" in target/release/bundle/: the Rusty Z interpreter
# with the story files from stories/ inside, so the app opens straight into
# the game. The About box shows the story's release and serial number.
# Pass --universal for an Apple Silicon + Intel build (needs
# `rustup target add x86_64-apple-darwin`).
set -eu
cd "$(dirname "$0")/.."

name=Planetfall
credit="Planetfall by Steve Meretzky. © 1983 Infocom, Inc."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
app="target/release/bundle/$name.app"

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

for f in stories/*; do
    [ "$f" = stories/README.md ] || cp "$f" "$app/Contents/Resources/stories/"
done
story=$(ls "$app/Contents/Resources/stories" | head -n 1)
if [ -z "$story" ]; then
    echo "No story file in stories/ to bundle (e.g. copy planetfall.zip there)." >&2
    exit 1
fi
# The app opens the first story alphabetically; take its release number
# (header word at 0x02) and serial number (six ASCII digits at 0x12).
story="$app/Contents/Resources/stories/$story"
release=$(od -An -tu1 -j2 -N2 "$story" | awk 'NF {print $1 * 256 + $2; exit}')
serial=$(dd if="$story" bs=1 skip=18 count=6 2>/dev/null)

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

# Shown under the version in the standard About box.
cat > "$app/Contents/Resources/Credits.html" <<EOF
<p style="font: 11px -apple-system; text-align: center">$credit<br>
Release $release / Serial number $serial<br><br>
Played with Rusty Z $version, an MIT-licensed Z-machine interpreter.</p>
EOF

cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>$name</string>
    <key>CFBundleDisplayName</key><string>$name</string>
    <key>CFBundleIdentifier</key><string>io.github.rustyz.planetfall</string>
    <key>CFBundleExecutable</key><string>rustyz</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$release</string>
    <key>CFBundleVersion</key><string>$serial</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.adventure-games</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>$credit</string>
</dict>
</plist>
EOF

# Ad-hoc signature: enough to run locally on Apple Silicon. Distributing to
# other Macs needs a Developer ID signature and notarization instead.
codesign --force --sign - "$app"
echo "Built $app"
