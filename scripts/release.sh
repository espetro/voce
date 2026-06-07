#!/usr/bin/env bash
# Build, sign, notarize (Apple), and package Voce as a universal macOS .app bundle.
#
# Produces:
#   dist/Voce.app                    — stapled .app (if notarized)
#   dist/Voce-VERSION.zip            — archive for GitHub releases
#
# Local install (default):
#   Copies the universal binary to ~/.local/bin/voce.
#   Pass --skip-install to suppress.
#
# Usage:
#   ./scripts/release.sh [--profile <keychain-profile>] [--skip-notarize] [--skip-install]
#
# Apple notarization credentials (pick one):
#   Option A — stored keychain profile (recommended):
#     xcrun notarytool store-credentials default \
#       --apple-id <your@apple.id> --team-id Y3ZC4LB357 --password <app-specific-password>
#     then pass: --profile default
#
#   Option B — environment variables:
#     export APPLE_ID="your@apple.id"
#     export APPLE_TEAM_ID="Y3ZC4LB357"
#     export APPLE_APP_PASSWORD="xxxx-xxxx-xxxx-xxxx"

set -euo pipefail

SIGN_IDENTITY="Developer ID Application: Joaquin Terrasa Moya (Y3ZC4LB357)"
APPLE_TARGETS=("aarch64-apple-darwin" "x86_64-apple-darwin")
BIN_NAME="voce"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DIST_DIR="${REPO_ROOT}/dist"
ENTITLEMENTS="${REPO_ROOT}/voce.entitlements"

# Parse flags
KEYCHAIN_PROFILE=""
SKIP_NOTARIZE=false
SKIP_INSTALL=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --profile)       KEYCHAIN_PROFILE="$2"; shift 2 ;;
    --skip-notarize) SKIP_NOTARIZE=true; shift ;;
    --skip-install)  SKIP_INSTALL=true; shift ;;
    *) echo "Unknown flag: $1" && exit 1 ;;
  esac
done

# Resolve notarytool auth args
NOTARY_AUTH=()
if [[ "${SKIP_NOTARIZE}" == "true" ]]; then
  echo "warning: --skip-notarize set — .app will be signed but not notarized."
elif [[ -n "${KEYCHAIN_PROFILE}" ]]; then
  NOTARY_AUTH=(--keychain-profile "${KEYCHAIN_PROFILE}")
elif [[ -n "${APPLE_ID:-}" && -n "${APPLE_TEAM_ID:-}" && -n "${APPLE_APP_PASSWORD:-}" ]]; then
  NOTARY_AUTH=(--apple-id "${APPLE_ID}" --team-id "${APPLE_TEAM_ID}" --password "${APPLE_APP_PASSWORD}")
else
  echo "warning: no notarization credentials found — skipping notarization."
  echo "         To notarize later, store credentials and re-run with --profile <name>."
  SKIP_NOTARIZE=true
fi

VERSION="$(cargo metadata --manifest-path "${REPO_ROOT}/Cargo.toml" --no-deps --format-version 1 \
  | python3 -c 'import sys,json; print(json.load(sys.stdin)["packages"][0]["version"])')"

echo "Building Voce v${VERSION} (universal macOS)…"

# ── Per-arch builds ──────────────────────────────────────────────────────────

ARCH_BINARIES=()
for TARGET in "${APPLE_TARGETS[@]}"; do
  echo ""
  echo "==> [${TARGET}] Building…"
  rustup target add "${TARGET}"
  cargo build --release --manifest-path "${REPO_ROOT}/Cargo.toml" --target "${TARGET}"
  ARCH_BINARIES+=("${REPO_ROOT}/target/${TARGET}/release/${BIN_NAME}")
done

# ── Universal binary ─────────────────────────────────────────────────────────

UNIVERSAL_DIR="${REPO_ROOT}/target/universal-apple-darwin/release"
mkdir -p "${UNIVERSAL_DIR}"
UNIVERSAL_BIN="${UNIVERSAL_DIR}/${BIN_NAME}"

echo ""
echo "==> Creating universal binary…"
lipo -create "${ARCH_BINARIES[@]}" -output "${UNIVERSAL_BIN}"
lipo -info "${UNIVERSAL_BIN}"

# ── Bundle assembly ──────────────────────────────────────────────────────────

echo ""
echo "==> Assembling .app bundle…"
rm -rf "${DIST_DIR}"
mkdir -p "${DIST_DIR}"

BUNDLE="${DIST_DIR}/Voce.app"
cp -R "${REPO_ROOT}/voce.app" "${BUNDLE}"
cp "${UNIVERSAL_BIN}" "${BUNDLE}/Contents/MacOS/${BIN_NAME}"

# Stamp version into Info.plist
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion ${VERSION}" "${BUNDLE}/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString ${VERSION}" "${BUNDLE}/Contents/Info.plist"

# ── Signing ──────────────────────────────────────────────────────────────────

echo ""
echo "==> Signing .app bundle…"
codesign \
  --sign "${SIGN_IDENTITY}" \
  --options runtime \
  --timestamp \
  --deep \
  --force \
  --entitlements "${ENTITLEMENTS}" \
  "${BUNDLE}"
codesign --verify --verbose "${BUNDLE}"

# ── Local install ─────────────────────────────────────────────────────────────

if [[ "${SKIP_INSTALL}" != "true" ]]; then
  echo ""
  echo "==> Installing universal binary to ~/.local/bin/${BIN_NAME}…"
  mkdir -p "${HOME}/.local/bin"
  cp "${UNIVERSAL_BIN}" "${HOME}/.local/bin/${BIN_NAME}"
  echo "    Installed: $(which ${BIN_NAME} 2>/dev/null || echo ~/.local/bin/${BIN_NAME})"
fi

# ── Packaging ────────────────────────────────────────────────────────────────

ZIP_PATH="${DIST_DIR}/Voce-${VERSION}.zip"
echo ""
echo "==> Packaging ${ZIP_PATH}…"
ditto -c -k --keepParent "${BUNDLE}" "${ZIP_PATH}"

# ── Notarization + stapling ──────────────────────────────────────────────────

if [[ "${SKIP_NOTARIZE}" != "true" ]]; then
  echo ""
  echo "==> Notarizing…"
  xcrun notarytool submit "${ZIP_PATH}" "${NOTARY_AUTH[@]}" --wait

  echo "==> Stapling…"
  xcrun stapler staple "${BUNDLE}"

  echo "==> Re-packaging with stapled bundle…"
  rm -f "${ZIP_PATH}"
  ditto -c -k --keepParent "${BUNDLE}" "${ZIP_PATH}"
fi

echo ""
echo "Done. Voce v${VERSION} is ready:"
echo "  Bundle : ${BUNDLE}"
echo "  Archive: ${ZIP_PATH}"
[[ "${SKIP_INSTALL}" != "true" ]] && echo "  Binary : ~/.local/bin/${BIN_NAME}"
echo ""
echo "Upload ${ZIP_PATH} to the GitHub v${VERSION} release."
