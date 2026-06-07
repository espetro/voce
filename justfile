build-panel:
    pnpm --dir assets/panel install
    assets/panel/node_modules/.bin/vite build --config assets/panel/vite.config.ts --root assets/panel

dev-panel:
    pnpm --dir assets/panel install
    assets/panel/node_modules/.bin/vite --config assets/panel/vite.config.ts --root assets/panel

build-rust:
    cargo build --release

build-all: build-panel build-rust

run: build-all
    ./target/release/voce

dev: build-panel
    cargo run
