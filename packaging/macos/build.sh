#!/usr/bin/env bash
set -euo pipefail

# zimged -- build the macOS release artifact (#42, spec §8.5) from an already-built binary.
#
# Usage: packaging/macos/build.sh <out-dir> [binary]
#
#   <out>/zimged-<ver>-macos-<arch>.dmg: zimged.app, ad-hoc signed, beside an /Applications link.
#   <arch> is arm64 or x64, read from the binary (default target/release/zimged).
#
# macOS only (plutil, codesign, hdiutil, lipo). It never builds zimged: run
# `cargo build --release --locked -p zimged --features avif` first. The version is the workspace
# version in Cargo.toml. The app is ad-hoc signed, not notarised: Gatekeeper asks before its first
# launch (right-click > Open). Info.plist's LSHandlerRank Alternate keeps zimged from ever being
# the default handler (spec §8.5).

root="$(cd "$(dirname "$0")/../.." && pwd)"
here="${root}/packaging/macos"
cd "${root}"

usage() { sed -n '/^# Usage:/,/^# macOS only/p' "$0" | sed 's/^# \{0,1\}//' | sed '$d' >&2; exit 2; }
[[ $# -ge 1 && $# -le 2 ]] || usage

out="$1"
binary="${2:-${root}/target/release/zimged}"
mkdir -p "${out}"
out="$(cd "${out}" && pwd)"

version="$(sed -n 's/^version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' Cargo.toml | head -n1)"
[[ -n "${version}" ]] || { echo "ERROR: no version in Cargo.toml's [workspace.package]." >&2; exit 1; }
[[ -x "${binary}" ]] || {
  echo "ERROR: ${binary} is missing: run cargo build --release --locked -p zimged --features avif first." >&2
  exit 1
}

case "$(lipo -archs "${binary}")" in
  arm64) arch=arm64 ;;
  x86_64) arch=x64 ;;
  *) echo "ERROR: ${binary} is not a single-architecture arm64 or x86_64 binary." >&2; exit 1 ;;
esac

stage="$(mktemp -d)"
trap 'rm -rf "${stage}"' EXIT
app="${stage}/zimged.app"
install -d "${app}/Contents/MacOS" "${app}/Contents/Resources"
install -m755 "${binary}" "${app}/Contents/MacOS/zimged"
install -m644 "${here}/zimged.icns" "${app}/Contents/Resources/zimged.icns"
install -m644 LICENSE "${app}/Contents/Resources/LICENSE"
sed "s/@VERSION@/${version}/g" "${here}/Info.plist" > "${app}/Contents/Info.plist"
printf 'APPL????' > "${app}/Contents/PkgInfo"
plutil -lint "${app}/Contents/Info.plist"

# Ad-hoc signature ("-"): no identity, so no notarisation, but the bundle (Info.plist and
# resources included) is sealed, which Apple silicon requires before it runs anything.
codesign --force --sign - --timestamp=none "${app}"
codesign --verify --strict --verbose=2 "${app}"

ln -s /Applications "${stage}/Applications"
dmg="${out}/zimged-${version}-macos-${arch}.dmg"
hdiutil create -volname "zimged ${version}" -srcfolder "${stage}" -fs HFS+ -format UDZO -ov "${dmg}"
hdiutil verify "${dmg}"
echo "${dmg}"
