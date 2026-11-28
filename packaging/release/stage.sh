#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

fail() {
    echo "stage.sh: $*" >&2
    exit 1
}

if [[ $# != 3 ]]; then
    echo 'Usage: stage.sh <target> <slug> <output-dir>' >&2
    exit 2
fi

target=$1
slug=$2
output=$3
exe=
elf_class=
case "$target:$slug" in
    x86_64-unknown-linux-gnu:linux-x86_64)
        elf_class=ELF64 machine='Advanced Micro Devices X86-64' interpreter=/lib64/ld-linux-x86-64.so.2
        ;;
    aarch64-unknown-linux-gnu:linux-arm64)
        elf_class=ELF64 machine=AArch64 interpreter=/lib/ld-linux-aarch64.so.1
        ;;
    armv7-unknown-linux-gnueabihf:linux-armhf)
        elf_class=ELF32 machine=ARM interpreter=/lib/ld-linux-armhf.so.3
        ;;
    i686-unknown-linux-gnu:linux-i686)
        elf_class=ELF32 machine='Intel 80386' interpreter=/lib/ld-linux.so.2
        ;;
    x86_64-pc-windows-msvc:windows-x86_64|i686-pc-windows-msvc:windows-i686|aarch64-pc-windows-msvc:windows-arm64)
        exe=.exe
        ;;
    aarch64-apple-darwin:macos-arm64|x86_64-apple-darwin:macos-x86_64)
        ;;
    *)
        fail "Unexpected target and slug: $target $slug"
        ;;
esac

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
build_dir="${CARGO_TARGET_DIR:-$root/target}/$target/release"
version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' "$root/Cargo.toml")
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "Invalid workspace version: $version"
[[ ! -e "$output" && ! -L "$output" ]] || fail "Output already exists: $output"

inspect_elf() {
    local binary=$1 info required_versions
    info=$(readelf -hlVW -- "$binary") || fail "Cannot inspect ELF: $binary"
    if ! awk -v class="$elf_class" -v machine="$machine" -v interpreter="$interpreter" '
        /^[[:space:]]+Class:/ { actual_class = $2 }
        /^[[:space:]]+Data:/ { little_endian = /little endian/ }
        /^[[:space:]]+Machine:/ {
            sub(/^[[:space:]]+Machine:[[:space:]]+/, "")
            actual_machine = $0
        }
        /^[[:space:]]+Flags:/ { hard_float = /Version5 EABI, hard-float ABI/ }
        /^[[:space:]]+\[Requesting program interpreter:/ {
            sub(/^[[:space:]]+\[Requesting program interpreter: /, "")
            sub(/\]$/, "")
            actual_interpreter = $0
        }
        END {
            exit !(actual_class == class && little_endian && actual_machine == machine &&
                   actual_interpreter == interpreter && (machine != "ARM" || hard_float))
        }
    ' <<< "$info"; then
        fail "Wrong ELF architecture or interpreter for $target: $binary"
    fi

    required_versions=$(awk '
        /^Version (symbols|definition) section/ { needs = 0 }
        /^Version needs section/ { needs = 1 }
        needs {
            for (i = 1; i < NF; i++) {
                if ($i != "Name:") continue
                if ($(i + 1) == "GLIBC_ABI_DT_RELR") print "2.36"
                if ($(i + 1) ~ /^GLIBC_[0-9]+\.[0-9]+(\.[0-9]+)?$/) print substr($(i + 1), 7)
            }
        }
    ' <<< "$info")
    [[ -n "$required_versions" ]] || fail "Missing glibc symbol versions: $binary"
    printf '%s\n' "$required_versions"
}

programs=(airscout airscout-planeserver airscout-cli catcheck elevation-tile-gen maptile-gen)
binaries=()
versions=()
for program in "${programs[@]}"; do
    binary="$build_dir/$program$exe"
    [[ -f "$binary" && -x "$binary" ]] || fail "Missing executable: $binary"
    binaries+=("$binary")
    if [[ -n "$elf_class" ]]; then
        symbols=$(inspect_elf "$binary")
        versions+=("$symbols")
    fi
done

floor=null
if [[ -n "$elf_class" ]]; then
    glibc_floor=$(printf '%s\n' "${versions[@]}" | sort -t . -k1,1n -k2,2n -k3,3n | tail -n 1)
    if [[ "$slug" == linux-x86_64 || "$slug" == linux-arm64 ]]; then
        highest=$(printf '%s\n' "$glibc_floor" 2.35 | sort -t . -k1,1n -k2,2n -k3,3n | tail -n 1)
        [[ "$highest" == 2.35 ]] || fail "$slug requires glibc $glibc_floor, above the 2.35 limit"
    fi
    floor="\"$glibc_floor\""
    printf '%s requires glibc %s\n' "$slug" "$glibc_floor"
fi

mkdir -p -- "$output/bin"
cp -p -- "${binaries[@]}" "$output/bin/"
cp -- "$root/README.md" "$root/LICENSE" "$root/NOTICE" "$output/"
printf '{\n  "target": "%s",\n  "slug": "%s",\n  "version": "%s",\n  "glibc_floor": %s\n}\n' \
    "$target" "$slug" "$version" "$floor" > "$output/stage.json"
