#!/bin/sh
# Download the pinned Smart Turn v3.2 model into lampOS/.cache/endpoint.
# The object is BSD-2 licensed by Daily (pipecat-ai/smart-turn-v3); it is not
# committed. Verification failure leaves no file at the destination.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
dir="${1:-$root/.cache/endpoint}"
name=smart-turn-v3.2-gpu.onnx
sha256=ab8dc64b88713f90b571c15b714bd1330e6c883cad8763dacf65c9376dc539be
url=https://huggingface.co/pipecat-ai/smart-turn-v3/resolve/f766f81d3cfdf7737ac64aad813d91bbfd56bf93/$name
check() {
    if command -v sha256sum >/dev/null 2>&1; then
        echo "$sha256  $1" | sha256sum -c - >/dev/null 2>&1
    else
        echo "$sha256  $1" | shasum -a 256 -c - >/dev/null 2>&1
    fi
}
mkdir -p "$dir"
if [ -f "$dir/$name" ] && check "$dir/$name"; then
    echo "$dir/$name verified"
    exit 0
fi
partial=$(mktemp "$dir/.$name.XXXXXX")
trap 'rm -f "$partial"' EXIT
curl -fsSL --max-filesize 33000000 -o "$partial" "$url"
check "$partial" || { echo "$name failed SHA256 verification" >&2; exit 1; }
chmod 0644 "$partial"
mv "$partial" "$dir/$name"
trap - EXIT
echo "$dir/$name verified"
