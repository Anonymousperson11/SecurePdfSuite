#!/usr/bin/env bash
# Shared build environment for scripts/check.sh and scripts/package.sh. Source it, do not run it.
#
# - puts ~/.cargo/bin on PATH (non-login shells do not have it)
# - detects cargo-xwin (cross-compiling from Linux / WSL) and defines the `cargo_win` wrapper
# - remaps local source paths so release binaries do not embed the build machine's directories
# - defines `winpath` (path as a Windows program expects it) and `run_exe`

export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
TARGET_TRIPLE="x86_64-pc-windows-msvc"

if command -v sccache >/dev/null 2>&1; then
    sccache --start-server </dev/null >/dev/null 2>&1 || true
fi

USE_XWIN=0
case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN* | Windows_NT) ;;
    *) if cargo xwin --version >/dev/null 2>&1; then USE_XWIN=1; fi ;;
esac

# Path as seen by the compiler / a Windows executable.
winpath() {
    if command -v wslpath >/dev/null 2>&1; then
        wslpath -w "$1"
    elif command -v cygpath >/dev/null 2>&1; then
        cygpath -w "$1"
    else
        printf '%s\n' "$1"
    fi
}

# Path as rustc sees it on this host (Windows form only when rustc itself is a Windows program).
_host_path() {
    if [ "$USE_XWIN" -eq 0 ] && command -v cygpath >/dev/null 2>&1; then
        cygpath -w "$1"
    else
        printf '%s\n' "$1"
    fi
}

_cargo_home="$(_host_path "${CARGO_HOME:-$HOME/.cargo}")"
_rustup_home="$(_host_path "${RUSTUP_HOME:-$HOME/.rustup}")"
_workspace="$(_host_path "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)")"
# The target-specific variable replaces `[target.*].rustflags` from .cargo/config.toml when cargo
# reads it directly, so the static-CRT flags are repeated here (cargo-xwin merges both; passing
# them twice is harmless).
export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS="-C target-feature=+crt-static -C link-arg=/ignore:4099 --remap-path-prefix=${_cargo_home}=/cargo --remap-path-prefix=${_rustup_home}=/rustup --remap-path-prefix=${_workspace}=/src"
unset _cargo_home _rustup_home _workspace

# cargo for the Windows target: `cargo_win build --release ...`
cargo_win() {
    local sub="$1"
    shift
    if [ "$USE_XWIN" -eq 1 ]; then
        cargo xwin "$sub" --target "$TARGET_TRIPLE" "$@"
    else
        cargo "$sub" --target "$TARGET_TRIPLE" "$@"
    fi
}

# Runs a Windows executable with a time limit and detached stdin (WSL interop hangs otherwise).
run_exe() {
    local limit="$1"
    shift
    timeout "$limit" "$@" </dev/null
}
