#!/usr/bin/env bash
# Builds the release executables, stages the payloads, packs both installers into target/dist
# and runs each installer's isolated "install -> uninstall" self-test.
#
#   ./scripts/package.sh              build + stage + pack + self-test
#   ./scripts/package.sh --no-build   reuse the existing release executables
#
# The self-test redirects every side effect (files, shortcuts, registry) into a private sandbox,
# so it is safe to run on a developer machine. It needs a Windows host or WSL interop.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# shellcheck source=scripts/build-env.sh
source "$ROOT/scripts/build-env.sh"

if [ "${1:-}" != "--no-build" ]; then
    echo "==> Release build ($TARGET_TRIPLE)"
    cargo_win build --release --workspace
fi

REL="$ROOT/target/$TARGET_TRIPLE/release"
DIST="$ROOT/target/dist"
for exe in SecurePdfReader.exe SecurePdfCreator.exe SecurePdfInstaller.exe; do
    [ -f "$REL/$exe" ] || { echo "ERROR: missing $REL/$exe" >&2; exit 1; }
done

echo "==> Release hygiene checks"
if command -v llvm-readobj >/dev/null 2>&1; then
    for exe in SecurePdfReader.exe SecurePdfCreator.exe SecurePdfInstaller.exe; do
        imports="$(llvm-readobj --coff-imports "$REL/$exe" | grep 'Name:' || true)"
        if echo "$imports" | grep -qiE 'vcruntime|msvcp|api-ms-win-crt|api-ms-win-appmodel-runtime-l1-1-[3-9]'; then
            echo "ERROR: $exe imports a VC++ runtime or Windows 11-only DLL:" >&2
            echo "$imports" | grep -iE 'vcruntime|msvcp|api-ms-win-crt|appmodel' >&2
            exit 1
        fi
        if ! llvm-readobj --coff-load-config "$REL/$exe" | grep -q 'DependentLoadFlags: 0x800'; then
            echo "ERROR: $exe was linked without /DEPENDENTLOADFLAG:0x800" >&2
            exit 1
        fi
        if ! llvm-readobj --coff-resources "$REL/$exe" | grep -q 'VERSIONINFO'; then
            echo "ERROR: $exe has no version resource" >&2
            exit 1
        fi
    done
    echo "    imports, dependent-load flag and version resources OK"
else
    echo "    (llvm-readobj not found; skipped PE checks)"
fi
if grep -aqE "(/home/|/Users/|\\\\Users\\\\)[A-Za-z0-9._-]+[/\\\\]\.(cargo|rustup)" "$REL"/SecurePd*.exe; then
    echo "ERROR: a release executable embeds a local build path" >&2
    exit 1
fi
echo "    no local build paths embedded"

echo "==> Staging payloads"
rm -rf "$DIST"
mkdir -p "$DIST/reader_stage" "$DIST/creator_stage"
BOOTSTRAP="Microsoft.WindowsAppRuntime.Bootstrap.dll"
cp "$REL/SecurePdfReader.exe" "$ROOT/assets/pdfium.dll" "$ROOT/assets/$BOOTSTRAP" \
    "$ROOT/assets/THIRD-PARTY-NOTICES.txt" "$ROOT/assets/关联spdf文件格式.bat" "$DIST/reader_stage/"
# The Creator never renders PDFs, so it ships without pdfium.dll.
cp "$REL/SecurePdfCreator.exe" "$ROOT/assets/$BOOTSTRAP" \
    "$ROOT/assets/THIRD-PARTY-NOTICES.txt" "$DIST/creator_stage/"

INSTALLER="$REL/SecurePdfInstaller.exe"
pack() {
    local app="$1" stage="$2" out="$3"
    echo "==> Packing $out"
    run_exe 300 "$INSTALLER" pack --app "$app" \
        --source-dir "$(winpath "$DIST/$stage")" --output "$(winpath "$DIST/$out")"
    [ -s "$DIST/$out" ] || { echo "ERROR: $out was not produced" >&2; exit 1; }
}
pack reader reader_stage SecurePdfReaderSetup.exe
pack creator creator_stage SecurePdfCreatorSetup.exe

selftest() {
    local setup="$1" report="$DIST/${1%.exe}.selftest.log"
    echo "==> Isolated install -> uninstall self-test: $setup"
    if ! run_exe 300 "$DIST/$setup" INSTALLER_SELFTEST --report "$(winpath "$report")"; then
        echo "ERROR: self-test failed for $setup" >&2
        [ -f "$report" ] && cat "$report" >&2
        exit 1
    fi
    [ -s "$report" ] || { echo "ERROR: self-test wrote no report for $setup" >&2; exit 1; }
    sed 's/^/    /' "$report"
}
selftest SecurePdfReaderSetup.exe
selftest SecurePdfCreatorSetup.exe

echo "==> Installers ready:"
ls -l "$DIST"/*Setup.exe
