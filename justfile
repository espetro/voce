mod eval "eval/justfile"
mod driver "audio-driver/justfile"

# ── Panel (SolidJS) ──────────────────────────────────────────────────────────

build-panel:
    pnpm --dir assets/panel install
    cd assets/panel && pnpm exec vite build

dev-panel:
    pnpm --dir assets/panel install
    cd assets/panel && pnpm exec vite

# ── Rust ─────────────────────────────────────────────────────────────────────

build-rust:
    cargo build

build-rust-release:
    cargo build --release

# ── App icon (.icns) ─────────────────────────────────────────────────────────
# Source: assets/panel/src/assets/logo-on.svg (white on transparent)
# Design: white wave on #0F1013 dark background; scaled to 520 px tall so the
# wave bleeds past the left/right edges, filling the square canvas naturally.

build-icons:
    #!/usr/bin/env bash
    set -euo pipefail
    SVG=assets/panel/src/assets/logo-on.svg
    TMP=$(mktemp -d)
    ICONSET="$TMP/voce.iconset"
    mkdir -p "$ICONSET"

    # Render SVG at 4× target height for sharpness, then trim transparent padding
    inkscape --export-type=png --export-width=8640 --export-filename="$TMP/hires.png" "$SVG" 2>/dev/null
    magick "$TMP/hires.png" -trim +repage "$TMP/trimmed.png"

    # Scale so the wave is 520 px tall (bleeds ~154 px beyond each side of the 1024 canvas)
    magick "$TMP/trimmed.png" -resize x2080 "$TMP/wave.png"

    # Composite centred on a 1024×1024 dark background (#0F1013 keeps sRGB encoding)
    magick -size 4096x4096 xc:'#0F1013' "$TMP/wave.png" \
        -gravity Center -compose Over -composite -alpha off \
        "$TMP/icon_4096.png"
    magick "$TMP/icon_4096.png" -resize 1024x1024 "$TMP/icon_1024.png"

    # All required iconset sizes
    for SIZE in 16 32 128 256 512; do
        magick "$TMP/icon_1024.png" -resize ${SIZE}x${SIZE}     "$ICONSET/icon_${SIZE}x${SIZE}.png"
        magick "$TMP/icon_1024.png" -resize $((SIZE*2))x$((SIZE*2)) "$ICONSET/icon_${SIZE}x${SIZE}@2x.png"
    done

    iconutil -c icns "$ICONSET" -o assets/icons/voce.icns
    rm -rf "$TMP"
    echo "assets/icons/voce.icns updated"

# ── Dev .app bundle (ad-hoc signed, debug binary) ────────────────────────────
# Copies the debug binary into the bundle, ad-hoc signs, then opens.
# macOS will prompt for mic access on first launch.

dev-app: build-panel build-rust
    rm -f voce.app/Contents/MacOS/voce
    cp target/debug/voce voce.app/Contents/MacOS/voce
    cp assets/icons/voce.icns voce.app/Contents/Resources/voce.icns
    codesign --force --deep --sign - \
        --entitlements voce.entitlements \
        voce.app
    open voce.app

# ── Local install to ~/Applications (release binary, ad-hoc signed) ──────────

install-local: build-panel build-rust-release
    mkdir -p voce.app/Contents/MacOS
    cp target/release/voce voce.app/Contents/MacOS/voce
    cp assets/icons/voce.icns voce.app/Contents/Resources/voce.icns
    codesign --force --deep --sign - \
        --entitlements voce.entitlements \
        voce.app
    rm -rf ~/Applications/Voce.app
    cp -R voce.app ~/Applications/Voce.app
    mdimport ~/Applications/Voce.app
    echo "Installed to ~/Applications/Voce.app"

# ── Release (universal binary, signed for distribution) ──────────────────────

build-all: build-panel build-rust-release

release *FLAGS:
    ./scripts/release.sh {{FLAGS}}
