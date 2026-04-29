#!/bin/bash
set -euo pipefail

APP_NAME="Agent Island"
BUNDLE_ID="com.agent-island.app"
BUILD_DIR="src-tauri/target"
DIST_DIR="dist"
DMG_NAME="AgentIsland.dmg"

NOTARIZE=false
SKIP_SIGN="${SKIP_SIGN:-0}"
SKIP_NOTARIZE="${SKIP_NOTARIZE:-0}"

usage() {
    cat <<'EOF'
Usage: ./build.sh [--notarize] [--help]

  --notarize    Sign and notarize the app bundle
  --help        Show this help

Environment variables:
  SKIP_SIGN=1         Skip code signing
  SKIP_NOTARIZE=1     Skip notarization (signing still happens)
  CODESIGN_IDENTITY   Code signing identity (Developer ID Application: ...)
  APPLE_ID            Apple ID for notarization
  APPLE_PASSWORD      App-specific password for notarization
  APPLE_TEAM_ID       Apple Team ID
EOF
}

for arg in "$@"; do
    case "$arg" in
        --notarize)
            NOTARIZE=true
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $arg" >&2
            usage >&2
            exit 1
            ;;
    esac
done

check_deps() {
    local missing=()
    for cmd in cargo pnpm create-dmg; do
        if ! command -v "$cmd" >/dev/null 2>&1; then
            missing+=("$cmd")
        fi
    done
    if [ ${#missing[@]} -gt 0 ]; then
        echo "Missing dependencies: ${missing[*]}" >&2
        echo "Install with: brew install create-dmg && cargo --version && pnpm --version" >&2
        exit 1
    fi
}

build_frontend() {
    echo "==> Building frontend..."
    pnpm install --frozen-lockfile
    pnpm build
}

build_rust() {
    echo "==> Building Rust binaries (arm64 + x86_64)..."
    cd src-tauri
    cargo build --release --target aarch64-apple-darwin
    cargo build --release --target x86_64-apple-darwin
    cd ..
}

create_universal() {
    echo "==> Creating universal binaries..."
    local arm_dir="$BUILD_DIR/aarch64-apple-darwin/release"
    local x86_dir="$BUILD_DIR/x86_64-apple-darwin/release"
    local out_dir="$BUILD_DIR/universal/release"
    mkdir -p "$out_dir"

    lipo -create \
        "$arm_dir/agent-island" \
        "$x86_dir/agent-island" \
        -output "$out_dir/agent-island"

    lipo -create \
        "$arm_dir/agent-island-bridge" \
        "$x86_dir/agent-island-bridge" \
        -output "$out_dir/agent-island-bridge" 2>/dev/null || true
}

build_app_bundle() {
    echo "==> Building app bundle..."
    local bundle_dir="$DIST_DIR/$APP_NAME.app/Contents"
    rm -rf "$DIST_DIR/$APP_NAME.app"
    mkdir -p "$bundle_dir/MacOS"
    mkdir -p "$bundle_dir/Resources"
    mkdir -p "$bundle_dir/Frameworks"

    cp "$BUILD_DIR/universal/release/agent-island" "$bundle_dir/MacOS/$APP_NAME"
    chmod +x "$bundle_dir/MacOS/$APP_NAME"

    # Copy Info.plist from tauri bundle if available
    if [ -f "src-tauri/Info.plist" ]; then
        cp "src-tauri/Info.plist" "$bundle_dir/Info.plist"
    fi

    # Copy icons
    if [ -f "src-tauri/icons/icon.icns" ]; then
        cp "src-tauri/icons/icon.icns" "$bundle_dir/Resources/AppIcon.icns"
    fi
}

sign_app() {
    if [ "$SKIP_SIGN" = "1" ]; then
        echo "==> Skipping code signing (SKIP_SIGN=1)"
        return
    fi

    local identity="${CODESIGN_IDENTITY:-}"
    if [ -z "$identity" ]; then
        echo "==> No CODESIGN_IDENTITY set, skipping signing"
        return
    fi

    echo "==> Signing app bundle..."
    local entitlements="src-tauri/Entitlements.plist"
    local app_path="$DIST_DIR/$APP_NAME.app"

    codesign --force --options runtime \
        --entitlements "$entitlements" \
        --sign "$identity" \
        --deep \
        "$app_path"

    echo "==> Verifying signature..."
    codesign --verify --deep --strict "$app_path"
    spctl --assess --type exec "$app_path" 2>/dev/null || echo "Note: spctl assessment skipped (no Gatekeeper on CI)"
}

create_dmg() {
    echo "==> Creating DMG..."
    mkdir -p "$DIST_DIR"
    local app_path="$DIST_DIR/$APP_NAME.app"
    local dmg_path="$DIST_DIR/$DMG_NAME"
    rm -f "$dmg_path"

    create-dmg \
        --volname "$APP_NAME" \
        --window-pos 200 120 \
        --window-size 600 400 \
        --icon-size 100 \
        --icon "$APP_NAME.app" 175 190 \
        --hide-extension "$APP_NAME.app" \
        --app-drop-link 425 190 \
        "$dmg_path" \
        "$app_path"

    echo "DMG created: $dmg_path"
}

sign_dmg() {
    if [ "$SKIP_SIGN" = "1" ]; then
        return
    fi
    local identity="${CODESIGN_IDENTITY:-}"
    if [ -z "$identity" ]; then
        return
    fi

    echo "==> Signing DMG..."
    codesign --force --sign "$identity" "$DIST_DIR/$DMG_NAME"
}

notarize_app() {
    if [ "$SKIP_NOTARIZE" = "1" ] || [ "$NOTARIZE" = "false" ]; then
        echo "==> Skipping notarization"
        return
    fi

    local apple_id="${APPLE_ID:-}"
    local apple_password="${APPLE_PASSWORD:-}"
    local team_id="${APPLE_TEAM_ID:-}"

    if [ -z "$apple_id" ] || [ -z "$apple_password" ] || [ -z "$team_id" ]; then
        echo "==> Notarization credentials not set, skipping"
        return
    fi

    echo "==> Notarizing DMG..."
    xcrun notarytool submit "$DIST_DIR/$DMG_NAME" \
        --apple-id "$apple_id" \
        --password "$apple_password" \
        --team-id "$team_id" \
        --wait

    echo "==> Stapling notarization ticket..."
    xcrun stapler staple "$DIST_DIR/$DMG_NAME"
}

main() {
    check_deps
    build_frontend
    build_rust
    create_universal
    build_app_bundle
    sign_app
    create_dmg
    sign_dmg
    notarize_app
    echo "==> Build complete: $DIST_DIR/$DMG_NAME"
}

main
