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

# Prefer CommandLineTools in local/dev environments to avoid Xcode license blocks.
if [[ -z "${DEVELOPER_DIR:-}" && -d "/Library/Developer/CommandLineTools" ]]; then
  export DEVELOPER_DIR="/Library/Developer/CommandLineTools"
fi

# Update version/build in Info.plist
cp "$APP_TEMPLATE_PLIST" "$APP_TEMPLATE_PLIST.tmp"
sed -i '' -e "s/{{ VERSION }}/$VERSION/g" "$APP_TEMPLATE_PLIST.tmp"
sed -i '' -e "s/{{ BUILD }}/$BUILD/g" "$APP_TEMPLATE_PLIST.tmp"
mv "$APP_TEMPLATE_PLIST.tmp" "$APP_TEMPLATE_PLIST"

export MACOSX_DEPLOYMENT_TARGET="11.0"

RUST_SYSROOT="$(rustc --print sysroot)"
HOST_TARGET="$(rustc -vV | awk '/host:/{print $2}')"
HAS_X86_STD=false
HAS_ARM_STD=false
[[ -d "$RUST_SYSROOT/lib/rustlib/x86_64-apple-darwin/lib" ]] && HAS_X86_STD=true
[[ -d "$RUST_SYSROOT/lib/rustlib/aarch64-apple-darwin/lib" ]] && HAS_ARM_STD=true

if $HAS_X86_STD && $HAS_ARM_STD; then
  # Build both archs and merge as universal.
  cargo build --release --locked --target=x86_64-apple-darwin
  cargo build --release --locked --target=aarch64-apple-darwin
  lipo \
    "target/x86_64-apple-darwin/release/$TARGET" \
    "target/aarch64-apple-darwin/release/$TARGET" \
    -create -output "$APP_BINARY"
else
  # Fallback: build host-arch only when cross stdlibs are unavailable.
  echo "Missing one or more macOS target stdlibs; building host target only ($HOST_TARGET)"
  cargo build --release --locked --target="$HOST_TARGET"
  cp "target/$HOST_TARGET/release/$TARGET" "$APP_BINARY"
fi

# Build app bundle
rm -rf "$APP_DIR/$APP_NAME"
mkdir -p "$APP_BINARY_DIR"
mkdir -p "$APP_EXTRAS_DIR"
cp -fRp "$APP_TEMPLATE" "$APP_DIR"
cp -fp "$APP_BINARY" "$APP_BINARY_DIR"
touch -r "$APP_BINARY" "$APP_DIR/$APP_NAME"

# Re-sign with a stable identifier so macOS TCC permissions remain bound to one app identity.
BUNDLE_ID=$(/usr/libexec/PlistBuddy -c "Print :CFBundleIdentifier" "$APP_DIR/$APP_NAME/Contents/Info.plist")
# For ad-hoc signatures, default designated requirement is cdhash (changes every build).
# Pin designated requirement to bundle identifier so TCC permissions survive updates.
codesign --force --deep --sign - \
  --identifier "$BUNDLE_ID" \
  "-r=designated => identifier \"$BUNDLE_ID\"" \
  "$APP_DIR/$APP_NAME"
codesign --verify --deep --strict --verbose=2 "$APP_DIR/$APP_NAME"

echo "Created '$APP_NAME' in '$APP_DIR'"
if [[ -n "${GITHUB_ENV:-}" ]]; then
  echo "APP_BUNDLE_PATH=$APP_DIR/$APP_NAME" >> "$GITHUB_ENV"
  echo "DMG_NAME=$DMG_NAME" >> "$GITHUB_ENV"
  echo "DMG_DIR=$DMG_DIR" >> "$GITHUB_ENV"
fi
