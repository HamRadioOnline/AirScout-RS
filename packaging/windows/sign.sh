#!/usr/bin/env bash
set -euo pipefail
umask 077
export LC_ALL=C

# Hosted Windows fallback: SimplySign Desktop with the Certum certificate in
# the Windows certificate store, then signtool sign /sha1 <leaf thumbprint>
# /fd SHA256 /tr http://time.certum.pl/ /td SHA256 and signtool verify /pa /all /tw.
# Select that path only after demonstrating noninteractive login, certificate
# selection, signing and timestamp verification on a fresh GitHub-hosted runner.

fail() {
    echo "sign.sh: $*" >&2
    exit 1
}

signing_state() {
    local count=0 name
    for name in CERTUM_ACCOUNT_EMAIL CERTUM_TOTP_SEED CERTUM_CERT_FINGERPRINT; do
        if [[ -n ${!name:-} ]]; then
            count=$((count + 1))
        fi
    done
    case $count in
        0) echo missing ;;
        3) echo configured ;;
        *) fail 'Partial Certum configuration. Set all three Certum secrets in the release environment.' ;;
    esac
}

prepare_tools() {
    local mode=$1 release lock version tool archive
    release="$(cd "$(dirname "${BASH_SOURCE[0]}")/../release" && pwd)"
    lock="$release/../tools.lock.json"
    mkdir -- "$tools"
    for tool in libp11 osslsigncode ssign-pkcs11; do
        archive="$tools/$tool.tar.gz"
        python3 "$release/fetch-tool.py" "$tool" linux-x86_64 "$archive"
        mkdir -- "$tools/$tool"
        if [[ $tool == ssign-pkcs11 ]]; then
            tar -xzf "$archive" -C "$tools/$tool" libssign_pkcs11.so
        else
            tar -xzf "$archive" -C "$tools/$tool" --strip-components=1
        fi
    done

    (
        cd "$tools/libp11"
        ./configure --prefix="$tools" --with-enginesdir="$tools/engines" \
            --with-modulesdir="$tools/providers"
        make -j2
        make install
    )
    cmake -S "$tools/osslsigncode" -B "$tools/osslsigncode/build" \
        -DCMAKE_BUILD_TYPE=Release 2>&1 | tee "$tools/osslsigncode-config.log"
    # osslsigncode 2.16 timestamps with the OpenSSL native HTTP client on
    # OpenSSL 3.x and with libcurl on older OpenSSL. Fail here rather than ship
    # a signer that cannot reach the RFC 3161 timestamp server.
    if grep -q 'cURL support disabled' "$tools/osslsigncode-config.log"; then
        fail 'osslsigncode was built without libcurl; RFC 3161 timestamping requires it.'
    fi
    cmake --build "$tools/osslsigncode/build" --parallel 2

    if [[ $mode == exes ]]; then
        version=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["ssign"]["linux-x86_64"]["version"])' "$lock")
        python3 "$release/fetch-tool.py" ssign linux-x86_64 "$tools/ssign.crate"
        mkdir -- "$tools/ssign"
        tar -xzf "$tools/ssign.crate" -C "$tools/ssign" --strip-components=1
        CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
            cargo install --locked --version "$version" --path "$tools/ssign" --root "$tools" ssign
    else
        python3 "$release/fetch-tool.py" ssign-core linux-x86_64 "$tools/ssign-core.crate"
        mkdir -- "$tools/ssign-core"
        tar -xzf "$tools/ssign-core.crate" -C "$tools/ssign-core" --strip-components=1
    fi
}

require_leaf() {
    # The module and ssign share this isolated session, which selects one card.
    # Store the selected public certificate before sending any digest for signing.
    openssl storeutl -provider-path "$tools/providers" -provider pkcs11prov -provider default \
        -certs 'pkcs11:type=cert' > "$work/card.pem" 2> "$work/tool.log" ||
        fail 'Could not read the Certum card certificate.'
    [[ $(grep -c '^-----BEGIN CERTIFICATE-----$' "$work/card.pem") == 1 ]] ||
        fail 'Expected exactly one Certum card certificate.'
    openssl x509 -in "$work/card.pem" -out "$work/leaf.pem" 2> "$work/tool.log" ||
        fail 'Could not decode the Certum card certificate.'

    local actual
    actual=$(openssl x509 -in "$work/leaf.pem" -noout -fingerprint "-$hash") ||
        fail 'Could not compute the Certum fingerprint.'
    actual=${actual#*=}
    actual=${actual//:/}
    [[ ${actual^^} == "$fingerprint" ]] || fail 'Certum card certificate fingerprint mismatch.'

    # Make both signers use the checked card's cached session only.
    unset CERTUM_OTP
    export CERTUM_TOKEN=''
}

verify_signature() {
    local file=$1
    "$osslsigncode" verify -CAfile "$ca_file" -TSA-CAfile "$ca_file" \
        -require-leaf-hash "$hash:$fingerprint" -in "$file" > "$work/tool.log" 2>&1 ||
        fail "Authenticode verification failed for ${file##*/}."
    grep -Fqx 'Timestamp Server Signature verification: ok' "$work/tool.log" ||
        fail "Missing verified timestamp for ${file##*/}."
}

sign_exes() {
    local stage program file
    local -a exes
    mkdir -- "$work/signed"
    # Architectures share basenames. Sign one stage at a time with the checked
    # cloud session and verify the full stage before replacing any of its EXEs.
    for stage in "$@"; do
        exes=()
        for program in airscout airscout-planeserver airscout-cli catcheck elevation-tile-gen maptile-gen; do
            file="$stage/bin/$program.exe"
            [[ -f $file ]] || fail "Missing EXE: $file"
            exes+=("$file")
        done
        "$tools/bin/ssign" --timestamp-url "$timestamp" --name AirScout-RS \
            -o "$work/signed" "${exes[@]}" > "$work/tool.log" 2>&1 ||
            fail "Certum EXE signing failed for ${stage##*/}."
        for file in "${exes[@]}"; do
            verify_signature "$work/signed/${file##*/}"
        done
        for file in "${exes[@]}"; do
            mv -f -- "$work/signed/${file##*/}" "$file"
        done
    done
}

sign_msi() {
    local candidate intermediate= file
    for candidate in "$tools"/ssign-core/src/certs/*.der; do
        openssl x509 -inform DER -in "$candidate" -out "$work/intermediate.pem"
        if openssl verify -partial_chain -trusted "$work/intermediate.pem" "$work/leaf.pem" \
            > "$work/tool.log" 2>&1; then
            intermediate="$work/intermediate.pem"
            break
        fi
    done
    [[ -n $intermediate ]] || fail 'No pinned Certum intermediate matches the card certificate.'
    for file in "$@"; do
        [[ -f $file ]] || fail "Missing MSI: $file"
        "$osslsigncode" sign -provider pkcs11prov -pkcs11module "$PKCS11_MODULE_PATH" \
            -certs "$work/leaf.pem" -key 'pkcs11:type=private' -ac "$intermediate" \
            -h sha256 -ts "$timestamp" -n AirScout-RS -in "$file" -out "$work/signed.msi" \
            > "$work/tool.log" 2>&1 || fail "Certum MSI signing failed for ${file##*/}."
        verify_signature "$work/signed.msi"
        mv -f -- "$work/signed.msi" "$file"
    done
}

case ${1:-} in
    check)
        signing_state
        exit
        ;;
    prepare)
        [[ $# == 3 && ( $2 == exes || $2 == msi ) ]] || fail 'Usage: sign.sh prepare <exes|msi> <tool-dir>'
        tools=$3
        prepare_tools "$2"
        exit
        ;;
    exes|msi)
        [[ $# -ge 3 ]] || fail 'Usage: sign.sh <exes|msi> <tool-dir> <stage-dirs|msi-files>...'
        mode=$1
        tools=$2
        shift 2
        ;;
    *) fail 'Expected check, prepare, exes or msi.' ;;
esac

state=$(signing_state)
[[ $state == configured ]] || fail 'Certum signing requires all three secrets.'
fingerprint=${CERTUM_CERT_FINGERPRINT//:/}
fingerprint=${fingerprint^^}
if [[ $fingerprint =~ ^[0-9A-F]{64}$ ]]; then
    hash=sha256
elif [[ $fingerprint =~ ^[0-9A-F]{40}$ ]]; then
    hash=sha1
else
    fail 'Expected a SHA-256 fingerprint or SHA-1 thumbprint in hexadecimal.'
fi
export CERTUM_EMAIL=$CERTUM_ACCOUNT_EMAIL CERTUM_OTP=$CERTUM_TOTP_SEED
unset CERTUM_ACCOUNT_EMAIL CERTUM_TOTP_SEED CERTUM_CERT_FINGERPRINT CERTUM_TOKEN
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
export XDG_RUNTIME_DIR="$work" OPENSSL_MODULES="$tools/providers"
export PKCS11_MODULE_PATH="$tools/ssign-pkcs11/libssign_pkcs11.so"
timestamp=http://time.certum.pl/
ca_file=/etc/ssl/certs/ca-certificates.crt
osslsigncode="$tools/osslsigncode/build/osslsigncode"
require_leaf

case $mode in
    exes) sign_exes "$@" ;;
    msi) sign_msi "$@" ;;
esac
