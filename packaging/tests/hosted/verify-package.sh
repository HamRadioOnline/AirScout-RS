#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

[[ $# == 3 ]] || { echo 'Usage: verify-package.sh <format> <asset> <version>' >&2; exit 2; }
format=$1
asset=$(realpath "$2")
version=$3
hosted=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$hosted/../../.." && pwd)
name=${asset##*/}
# The tag may contain dots, so derive the slug from the full filename.
slug=${name%."${name##*.}"}
[[ "$format" != archlinux ]] || slug=${name%.pkg.tar.zst}
slug=linux-${slug##*-}

case "$slug" in
    linux-x86_64) platform=linux/amd64; machine='Advanced Micro Devices X86-64'; class=ELF64; deb_arch=amd64; pkg_arch=x86_64 ;;
    linux-arm64) platform=linux/arm64; machine=AArch64; class=ELF64; deb_arch=arm64; pkg_arch=aarch64 ;;
    linux-armhf) platform=linux/arm/v7; machine=ARM; class=ELF32; deb_arch=armhf; pkg_arch=armv7h ;;
    linux-i686) platform=linux/386; machine='Intel 80386'; class=ELF32; deb_arch=i386; pkg_arch=i686 ;;
    *) echo "Unexpected asset name: $name" >&2; exit 2 ;;
esac
app_id=io.github.hamradioonline.AirScoutRS
unit=airscout-planeserver.service
config=/etc/airscout-planeserver/planeserver.toml

check_elf_header() {
    local headers
    headers=$(readelf -hW "$1")
    grep -Eq "Class:[[:space:]]+$class$" <<< "$headers"
    grep -Eq "Machine:[[:space:]]+$machine$" <<< "$headers"
    grep -F 'little endian' <<< "$headers"
    if [[ "$slug" == linux-armhf && "${2:-}" != runtime ]]; then
        grep -F 'hard-float ABI' <<< "$headers"
    fi
}

pin() {
    python3 - "$repo/packaging/tools.lock.json" "$@" <<'PY'
import json
import sys

pin = json.load(open(sys.argv[1], encoding="utf-8"))[sys.argv[2]][sys.argv[3]]
print(pin["url"] + "@" + pin["digest"])
PY
}

if [[ ${VERIFY_MODE:-} == systemd ]]; then
    [[ "$format" == deb && $(ps -p 1 -o comm=) == systemd ]]
    : "${UPGRADE_DEB:?Set UPGRADE_DEB to the newer package}"
    state=/var/lib/airscout-planeserver
    cleanup() {
        sudo systemctl disable --now "$unit" || true
        sudo apt-get purge -y airscout-rs || true
        sudo rm -rf -- "$state"
    }
    trap cleanup EXIT
    sudo apt-get update -qq
    sudo apt-get install -y --no-install-recommends "$asset"
    [[ $(systemctl is-enabled "$unit" || true) == disabled ]]
    [[ $(systemctl show -p ActiveState --value "$unit") == inactive ]]
    [[ $(systemctl show -p ExecMainStartTimestampMonotonic --value "$unit") == 0 ]]
    [[ $(systemctl show -p User --value "$unit") == airscout-planeserver ]]
    sudo apt-get -o Dpkg::Options::=--force-confold install -y --no-install-recommends "$UPGRADE_DEB"
    [[ $(systemctl is-enabled "$unit" || true) == disabled ]]
    [[ $(systemctl show -p ActiveState --value "$unit") == inactive ]]
    [[ $(systemctl show -p ExecMainStartTimestampMonotonic --value "$unit") == 0 ]]
    sudo apt-get install -y --allow-downgrades --no-install-recommends "$asset"
    sudo systemctl enable --now "$unit"
    serving() {
        curl --fail --silent --show-error --noproxy '*' --retry 5 --retry-all-errors \
            --retry-delay 1 --max-time 2 http://127.0.0.1:9872/planes.json |
          python3 -c 'import json, sys; assert json.load(sys.stdin) == {"version": 1, "full_count": 0}'
        pid=$(systemctl show -p MainPID --value "$unit")
        [[ $(ps -p "$pid" -o uid= | tr -d ' ') == "$(id -u airscout-planeserver)" ]]
    }
    serving
    old_pid=$pid
    printf '\n# TE1ST JO50AA\n' | sudo tee -a "$config" >/dev/null
    printf 'TE1ST JO50AA\n' | sudo -u airscout-planeserver tee "$state/smoke-state" >/dev/null
    sudo apt-get -o Dpkg::Options::=--force-confold install -y --no-install-recommends "$UPGRADE_DEB"
    [[ $(systemctl is-enabled "$unit") == enabled ]]
    serving
    [[ "$pid" != "$old_pid" ]]
    grep -Fx '# TE1ST JO50AA' "$config"
    sudo grep -Fx 'TE1ST JO50AA' "$state/smoke-state"
    exit
fi

if [[ ${VERIFY_MODE:-} != container ]]; then
    case "$format:$slug" in
        rpm:*) image=$(pin rpm-version-check "$slug") ;;
        archlinux:linux-x86_64) image=$(pin arch-version-check linux-x86_64) ;;
        *:linux-i686) image=$(pin package-smoke-base "$slug") ;;
        *) image=$(pin container-base "$slug") ;;
    esac
    docker_options=(--env VERIFY_MODE=container --env DEBIAN_FRONTEND=noninteractive)
    host_dirs=()
    emulator_container=
    cleanup_host() {
        if [[ -n "$emulator_container" ]]; then
            docker rm "$emulator_container" >/dev/null || true
        fi
        if ((${#host_dirs[@]})); then
            rm -rf -- "${host_dirs[@]}"
        fi
    }
    trap cleanup_host EXIT
    if [[ "$format" == archlinux && "$slug" != linux-x86_64 ]]; then
        # Ubuntu's patched GNU tar (CVE-2025-45582) opens each parent with
        # openat2(RESOLVE_BENEATH). The musl-built binfmt qemu-aarch64 corrupts
        # that call's O_NOFOLLOW flag, so every nested member fails with EINVAL.
        # Unpack natively on the host. The target userspace still inspects the
        # hooks and executes the ELF payload.
        payload=$(mktemp -d "${TMPDIR:-/tmp}/airscout-payload.XXXXXX")
        host_dirs+=("$payload")
        tar --zstd -xf "$asset" -C "$payload"
        # runuser -u nobody executes the payload inside the container.
        chmod 755 "$payload"
        docker_options+=(--mount "type=bind,source=$payload,target=/payload,readonly")
    fi
    if [[ "$(uname -m)" == x86_64 && "$platform" == linux/arm* ]]; then
        emulator=$(pin container-tools binfmt)
        docker run --rm --privileged "$emulator" --install arm64,arm
        if [[ "$format:$slug" == appimage:linux-armhf ]]; then
            # The AppImage AI marker occupies ELF identification bytes that
            # qemu-arm's binfmt rule requires to be zero.
            emulator_dir=$(mktemp -d "${TMPDIR:-/tmp}/airscout-qemu.XXXXXX")
            host_dirs+=("$emulator_dir")
            emulator_container=$(docker create "$emulator")
            docker cp "$emulator_container:/usr/bin/qemu-arm" "$emulator_dir/qemu-arm"
            docker_options+=(--mount "type=bind,source=$emulator_dir/qemu-arm,target=/qemu-arm,readonly"
                --env APPIMAGE_QEMU=/qemu-arm)
        fi
    fi
    docker run --rm --platform "$platform" --volume "$hosted:/workspace/packaging/tests/hosted:ro" \
        --volume "$asset:/asset/$name:ro" "${docker_options[@]}" "$image" \
        bash /workspace/packaging/tests/hosted/verify-package.sh "$format" "/asset/$name" "$version"
    exit
fi

work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
chmod 755 "$work"
root=/usr
cd "$work"

case "$format:$slug" in
    rpm:*)
        dnf -y install python3 binutils desktop-file-utils appstream util-linux "$asset"
        [[ $(rpm -qp --qf '%{ARCH}' "$asset") == "$pkg_arch" ]]
        rpm -qp --requires "$asset" > dependencies
        rpm -qp --scripts "$asset" > scripts
        grep -Eq '^systemd($| )' dependencies
        rpm -qp --qf '[%{FILENAMES} %{FILEFLAGS:fflags}\n]' "$asset" |
            awk -v path="$config" '$1 == path && $2 ~ /c/ && $2 ~ /n/ { ok=1 } END { exit !ok }'
        ;;
    archlinux:linux-x86_64)
        pacman -Syu --noconfirm --needed archlinux-keyring python binutils desktop-file-utils appstream sudo
        # Sync dependencies from signed Arch repositories. The candidate is a
        # local artifact, so its trust comes from this workflow's transport.
        pacman -U --noconfirm "$asset"
        tar -xOf "$asset" .PKGINFO > pkginfo
        tar -xOf "$asset" .INSTALL > scripts
        ;;
    *)
        apt-get update -qq
        apt-get install -y --no-install-recommends python3 binutils desktop-file-utils appstream zstd util-linux
        if [[ "$format" == deb ]]; then
            [[ $(dpkg-deb -f "$asset" Architecture) == "$deb_arch" ]]
            dpkg-deb -f "$asset" Depends | grep -Eq '(^|[, ])systemd([, (]|$)'
            dpkg-deb -e "$asset" control
            grep -Fx "$config" control/conffiles
            cat control/postinst control/prerm > scripts
            # apt only treats paths ending in .deb as package files.
            apt-get install -y --no-install-recommends "$asset"
        elif [[ "$format" == archlinux ]]; then
            apt-get install -y --no-install-recommends libgtk-3-0 libasound2 \
              libayatana-appindicator3-1 libxdo3 libxkbcommon-x11-0 librsvg2-common \
              libgl1 libegl1 libgl1-mesa-dri libvulkan1
            # Arch does not publish an official image for these slugs. Inspect
            # the actual package hooks and execute its ELF payload in the
            # matching Debian/Ubuntu target userspace. The host unpacked the
            # package into /payload (see above).
            cp /payload/.PKGINFO pkginfo
            cp /payload/.INSTALL scripts
            root=/payload/usr
        elif [[ "$format" == appimage ]]; then
            # AppImage supplies GTK, ALSA, and the rest of its bundled runtime.
            apt-get install -y --no-install-recommends \
              libx11-6 libx11-xcb1 libxcb-render0 libxcb-shm0 libxcb-xkb1 \
              libwayland-client0 libwayland-cursor0 libwayland-egl1 \
              libgl1 libegl1 libgl1-mesa-dri libvulkan1
            cp "$asset" image
            chmod +x image
            check_elf_header image runtime
            if [[ -n "${APPIMAGE_QEMU:-}" ]]; then
                # The pinned binfmt QEMU build expects a preserved guest argv[0].
                "$APPIMAGE_QEMU" ./image ./image --appimage-extract >/dev/null
            else
                ./image --appimage-extract >/dev/null
            fi
            root="$work/squashfs-root/usr"
        else
            echo "Unknown format: $format" >&2
            exit 2
        fi
        ;;
esac

if [[ "$format" == archlinux ]]; then
    grep -Fx "arch = $pkg_arch" pkginfo
    for dependency in systemd gtk3 libayatana-appindicator xdotool alsa-lib; do
        grep -Fx "depend = $dependency" pkginfo
    done
    grep -Fx "backup = ${config#/}" pkginfo
fi

if [[ "$format" != appimage ]]; then
    grep -F 'systemd-sysusers airscout-planeserver.conf' scripts
    grep -F "systemctl preset $unit" scripts
    grep -F "systemctl disable $unit" scripts
    if [[ "$format" == deb || "$format" == rpm ]]; then
        grep -F "systemctl try-restart $unit" scripts
    fi
    if grep -Eq 'systemctl[[:space:]]+(start|restart|enable)([[:space:]]|$)' scripts; then
        echo 'A lifecycle hook starts or enables the service' >&2
        exit 1
    fi
    grep -Fx "disable $unit" "$root/lib/systemd/system-preset/00-airscout-planeserver.preset"
    unit_path="$root/lib/systemd/system/$unit"
    [[ -f "$unit_path" ]] || unit_path="/lib/systemd/system/$unit"
    grep -Fx 'User=airscout-planeserver' "$unit_path"
    grep -Fx 'StateDirectory=airscout-planeserver' "$unit_path"
    grep -Fx 'WantedBy=multi-user.target' "$unit_path"
    grep -E '^u[[:space:]]+airscout-planeserver([[:space:]]|$)' \
        "$root/lib/sysusers.d/airscout-planeserver.conf"
    grep -F 'ExecStart=/usr/bin/airscout-planeserver --headless' "$unit_path"
fi

desktop-file-validate "$root/share/applications/$app_id.desktop"
appstreamcli validate --no-net "$root/share/metainfo/$app_id.metainfo.xml"
runtime=$(getconf GNU_LIBC_VERSION)
runtime=${runtime#glibc }
limit=$runtime
if [[ "$slug" == linux-x86_64 || "$slug" == linux-arm64 ]]; then limit=2.35; fi
smoke_args=("$root/bin" "$version")
if [[ "$format" == appimage ]]; then
    smoke_args+=(--apprun "$work/squashfs-root/AppRun")
    ldd_path="$root/lib"
else
    ldd_path=
fi

for command in airscout airscout-planeserver airscout-cli catcheck elevation-tile-gen maptile-gen; do
    binary="$root/bin/$command"
    check_elf_header "$binary"
    # Tracing via the target loader checks the resolved transitive DT_NEEDED
    # closure. The command smoke below also checks normal entry points.
    readelf -dW "$binary" | awk '/NEEDED/ {print}'
    libraries=$(LD_LIBRARY_PATH="$ldd_path" ldd "$binary")
    if [[ "$libraries" == *'not found'* ]]; then
        printf '%s\n' "$libraries" >&2
        exit 1
    fi
    floor=$(readelf -VW "$binary" | awk '
        /Version (symbols|definition) section/ { needs=0 }
        /Version needs section/ { needs=1 }
        needs {
            for (i=1; i<NF; i++) {
                if ($i != "Name:") continue
                if ($(i+1) == "GLIBC_ABI_DT_RELR") print "2.36"
                if ($(i+1) ~ /^GLIBC_[0-9]+\.[0-9]+(\.[0-9]+)?$/) print substr($(i+1), 7)
            }
        }' | sort -V | tail -1)
    [[ -n "$floor" ]]
    printf '%s %s requires glibc %s (runtime %s)\n' "$name" "$command" "$floor" "$runtime"
    [[ $(printf '%s\n' "$floor" "$limit" | sort -V | tail -1) == "$limit" ]]
done

runuser -u nobody -- python3 "$hosted/smoke-stage.py" "${smoke_args[@]}"
