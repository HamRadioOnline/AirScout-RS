#!/usr/bin/env bash
# Prepares a self-hosted Linux runner: Rust toolchain, a build directory that survives
# between runs, and the cross toolchain for the given targets.
# Usage: ci-prepare.sh [rust-target ...]
set -euo pipefail

if [ "${RUNNER_OS:-Linux}" != "Linux" ]; then
  echo "::error::This job expects a Linux runner (found ${RUNNER_OS})."
  exit 1
fi

# --- Rust -------------------------------------------------------------------------
export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v rustup >/dev/null 2>&1; then
  echo "rustup not found, installing it for $(id -un)"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain none
fi
rustup toolchain install stable --profile minimal --component clippy --component rustfmt
for target in "$@"; do
  rustup target add --toolchain stable "$target"
done

# --- System packages --------------------------------------------------------------
missing=()
command -v cc >/dev/null 2>&1 || missing+=(build-essential)
for target in "$@"; do
  case "$target" in
    x86_64-pc-windows-gnu)
      command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1 || missing+=(gcc-mingw-w64-x86-64) ;;
  esac
done
# The alarm beep (rodio, feature "sound") links ALSA on a native Linux build; the windows-gnu
# cross build uses WASAPI and needs nothing.
command -v pkg-config >/dev/null 2>&1 || missing+=(pkg-config)
pkg-config --exists alsa >/dev/null 2>&1 || missing+=(libasound2-dev)
# The planeserver tray icon (tray-icon) links GTK 3 on a native Linux build (no target
# given); the windows-gnu cross build needs none of it. Only libgtk-3-dev is needed to build:
# muda 0.21 does not link xdo, and AppIndicator is loaded at run time. CI still installs the
# AppIndicator and xdo dev packages as harmless extras (the docker gate does too). The header
# check below only detects whether the xdo dev package is present (libxdo has no .pc file on
# some distros, and the runtime libxdo3 alone has no header).
if [ "$#" -eq 0 ]; then
  pkg-config --exists gtk+-3.0 >/dev/null 2>&1 || missing+=(libgtk-3-dev)
  pkg-config --exists ayatana-appindicator3-0.1 >/dev/null 2>&1 \
    || missing+=(libayatana-appindicator3-dev)
  test -e /usr/include/xdo.h || missing+=(libxdo-dev)
fi
if [ "${#missing[@]}" -gt 0 ]; then
  if command -v apt-get >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
    echo "Installing: ${missing[*]}"
    sudo -n apt-get update -qq
    sudo -n apt-get install -y --no-install-recommends "${missing[@]}"
  else
    echo "::error::Missing on this runner and not installable without sudo: ${missing[*]}"
    exit 1
  fi
fi

# --- Environment for the following steps ------------------------------------------
if [ -n "${GITHUB_ENV:-}" ]; then
  {
    echo "RUSTUP_TOOLCHAIN=stable"
    # Outside the checkout, so the build cache survives between runs.
    echo "CARGO_TARGET_DIR=${RUNNER_TOOL_CACHE:-$HOME/.cache}/airscout-rs-target"
  } >> "$GITHUB_ENV"
  echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"
fi
rustc +stable --version
cargo +stable --version
df -h "${RUNNER_TOOL_CACHE:-$HOME}" | tail -1
