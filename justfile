# ── Panel (SolidJS) ──────────────────────────────────────────────────────────

build-panel:
    pnpm --dir assets/panel install
    assets/panel/node_modules/.bin/vite build --config assets/panel/vite.config.ts --root assets/panel

dev-panel:
    pnpm --dir assets/panel install
    assets/panel/node_modules/.bin/vite --config assets/panel/vite.config.ts --root assets/panel

# ── Rust ─────────────────────────────────────────────────────────────────────

build-rust:
    cargo build

build-rust-release:
    cargo build --release

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

# ── Release (universal binary, signed for distribution) ──────────────────────

build-all: build-panel build-rust-release

release *FLAGS:
    ./scripts/release.sh {{FLAGS}}
