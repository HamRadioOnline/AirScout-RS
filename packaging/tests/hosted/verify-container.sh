#!/usr/bin/env bash
set -euo pipefail

probe() {
    docker exec -i "$1" /bin/bash -eu -s <<'SMOKE'
test -z "${DISPLAY+x}"
test "$(id -u):$(id -g)" = 9872:9872
touch /config/.smoke-test /data/.smoke-test
libraries=$(ldd /usr/local/bin/airscout-planeserver)
if [[ "$libraries" == *"not found"* ]]; then
    printf '%s\n' "$libraries" >&2
    exit 1
fi
while (( SECONDS < 12 )); do
    if timeout 1 /bin/bash -ec '
        exec 3<>/dev/tcp/127.0.0.1/9872
        printf "GET /planes.json HTTP/1.0\r\nHost: localhost\r\n\r\n" >&3
        read -r protocol status rest <&3
        [[ "$protocol" == HTTP/* && "$status" == 200 ]]
        body=$(tr -d "\r" <&3 | sed "1,/^$/d")
        [[ "$body" == "{\"full_count\":0,\"version\":1}" ]]
    ' 2>/dev/null; then
        exit 0
    fi
    sleep 0.25
done
echo "PlaneServer did not serve the expected JSON on 127.0.0.1:9872" >&2
exit 1
SMOKE
}

if [[ ${1:-} == probe && $# == 2 ]]; then
    probe "$2"
    exit
fi
[[ ${1:-} == published && $# == 2 ]] || {
    echo 'Usage: verify-container.sh probe <container> | published <candidate-dir>' >&2
    exit 2
}
set +x
repo=ghcr.io/hamradioonline/airscout-planeserver
pairs=$(python3 - "$2/result.json" <<'PY'
import json
import re
import sys

images = json.load(open(sys.argv[1], encoding="utf-8"))["images"]
assert {image["platform"] for image in images.values()} == {"linux/amd64", "linux/arm64", "linux/arm/v7"}
for image in images.values():
    assert re.fullmatch(r"sha256:[0-9a-f]{64}", image["digest"])
    print(image["platform"], image["digest"])
PY
)
container=
config_volume=
data_volume=
cleanup() {
    if [[ -n "$container" ]]; then docker rm -f "$container" >/dev/null; fi
    if [[ -n "$config_volume" ]]; then docker volume rm "$config_volume" >/dev/null; fi
    if [[ -n "$data_volume" ]]; then docker volume rm "$data_volume" >/dev/null; fi
    container= config_volume= data_volume=
}
: "${GITHUB_ACTOR:?Set GITHUB_ACTOR for GHCR verification}"
: "${GITHUB_TOKEN:?Set GITHUB_TOKEN for GHCR verification}"
DOCKER_CONFIG=$(mktemp -d)
export DOCKER_CONFIG
trap 'cleanup || true; rm -rf -- "$DOCKER_CONFIG"' EXIT
unset DOCKER_AUTH_CONFIG
# Keep Docker from auto-detecting a credential helper outside this directory.
printf '%s\n' '{"auths":{"ghcr.io":{}}}' > "$DOCKER_CONFIG/config.json"
if ! env -u GITHUB_TOKEN docker login ghcr.io --username "$GITHUB_ACTOR" --password-stdin \
    <<< "$GITHUB_TOKEN" >/dev/null 2>&1; then
    echo 'GHCR login failed' >&2
    exit 1
fi
unset GITHUB_TOKEN

while read -r platform digest; do
    image="$repo@$digest"
    docker pull --platform "$platform" "$image"
    [[ $(docker image inspect --format '{{.Os}}/{{.Architecture}}' "$image") == "${platform%/v7}" ]]
    config_volume=$(docker volume create)
    data_volume=$(docker volume create)
    mounts=(--mount "type=volume,src=$config_volume,dst=/config" --mount "type=volume,src=$data_volume,dst=/data")
    container=$(docker run -d --pull never --network none --platform "$platform" "${mounts[@]}" "$image" --run-for 15)
    if ! probe "$container"; then
        docker logs "$container" >&2
        exit 1
    fi
    [[ $(timeout 25 docker wait "$container") == 0 ]]
    docker rm "$container" >/dev/null
    container=
    container=$(docker run -d --pull never --network none --platform "$platform" "${mounts[@]}" "$image" --run-for 15)
    docker exec "$container" /bin/sh -ec 'test -f /config/.smoke-test; test -f /data/.smoke-test'
    if ! probe "$container"; then
        docker logs "$container" >&2
        exit 1
    fi
    [[ $(timeout 25 docker wait "$container") == 0 ]]
    cleanup
done <<< "$pairs"
