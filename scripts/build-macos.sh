#!/usr/bin/env -S bash -e

TARGET="coco"                       # your Cargo binary name
ASSETS_DIR="assets"
RELEASE_DIR="target/release"
APP_NAME="Coco.app"
APP_TEMPLATE="$ASSETS_DIR/macos/$APP_NAME"
APP_TEMPLATE_PLIST="$APP_TEMPLATE/Contents/Info.plist"
APP_DIR="$RELEASE_DIR/macos"
APP_BINARY="$RELEASE_DIR/$TARGET"
APP_BINARY_DIR="$APP_DIR/$APP_NAME/Contents/MacOS"
APP_EXTRAS_DIR="$APP_DIR/$APP_NAME/Contents/Resources"
DMG_NAME="coco.dmg"
DMG_DIR="$RELEASE_DIR/macos"

VERSION="{$APP_VERSION}"
BUILD=$(git describe --always --dirty --exclude='*')

# Update version/build in Info.plist
cp "$APP_TEMPLATE_PLIST" "$APP_TEMPLATE_PLIST.tmp"
sed -i '' -e "s/{{ VERSION }}/$VERSION/g" "$APP_TEMPLATE_PLIST.tmp"
sed -i '' -e "s/{{ BUILD }}/$BUILD/g" "$APP_TEMPLATE_PLIST.tmp"
mv "$APP_TEMPLATE_PLIST.tmp" "$APP_TEMPLATE_PLIST"

export MACOSX_DEPLOYMENT_TARGET="11.0"

# Ensure both targets exist
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin

# Build both archs
cargo build --release --locked --target=x86_64-apple-darwin
cargo build --release --locked --target=aarch64-apple-darwin

# Create universal binary
lipo \
  "target/x86_64-apple-darwin/release/$TARGET" \
  "target/aarch64-apple-darwin/release/$TARGET" \
  -create -output "$APP_BINARY"

# Build app bundle
rm -rf "$APP_DIR/$APP_NAME"
mkdir -p "$APP_BINARY_DIR"
mkdir -p "$APP_EXTRAS_DIR"
cp -fRp "$APP_TEMPLATE" "$APP_DIR"
cp -fp "$APP_BINARY" "$APP_BINARY_DIR"
touch -r "$APP_BINARY" "$APP_DIR/$APP_NAME"

echo "Created '$APP_NAME' in '$APP_DIR'"
echo "APP_BUNDLE_PATH=$APP_DIR/$APP_NAME" >> "$GITHUB_ENV"
echo "DMG_NAME=$DMG_NAME" >> "$GITHUB_ENV"
echo "DMG_DIR=$DMG_DIR" >> "$GITHUB_ENV"
