#!/usr/bin/env bash
# Build the V2 supervisor from a frozen lamp-voice commit; publish no OTA intent.
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/ota-config.sh"

# A non-login shell (including make) may not load rustup's PATH setup.
if ! command -v cargo >/dev/null 2>&1; then
  cargo_bin="${CARGO_HOME:-${HOME}/.cargo}/bin"
  if [ -x "$cargo_bin/cargo" ]; then
    export PATH="$cargo_bin:$PATH"
  fi
fi

for tool in git cargo zig gsutil python3 tar; do
  command -v "$tool" >/dev/null || { echo "Missing required tool: $tool" >&2; exit 1; }
done
cargo zigbuild --help >/dev/null

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
remote=$(git -C "$ROOT_DIR" remote get-url origin)
git init --bare -q "$work/source.git"
git -C "$work/source.git" fetch --depth=1 "$remote" refs/heads/lamp-voice
commit=$(git -C "$work/source.git" rev-parse --verify 'FETCH_HEAD^{commit}')
mkdir -p "$work/source" "$work/artifacts"
git -C "$work/source.git" archive "$commit" lampOS | tar -xf - -C "$work/source"
source_dir="$work/source/lampOS"
[ -f "$source_dir/Cargo.lock" ] && [ -f "$source_dir/rust-toolchain.toml" ] || {
  echo 'lamp-voice must contain a locked, pinned lampOS workspace' >&2; exit 1;
}
echo "Building lamp-os supervisor from lamp-voice @ $commit (Linux ARM64)"
(
  cd "$source_dir"
  CARGO_TARGET_DIR="$work/target" cargo zigbuild --locked --release \
    --target aarch64-unknown-linux-gnu -p lamp-keeper
)
cp "$work/target/aarch64-unknown-linux-gnu/release/lamp-keeper" "$work/artifacts/lamp-os"

# Check the artifact itself, not just the requested Cargo target.
python3 - "$work/artifacts" "$commit" "${ROOT_DIR}/system/${VERSION_FILE:-VERSION_LAMP_OS}" <<'PY'
import hashlib, json, pathlib, re, sys
root, commit = pathlib.Path(sys.argv[1]), sys.argv[2]
binary = (root / 'lamp-os').read_bytes()
if len(binary) < 64 or binary[:6] != b'\x7fELF\x02\x01' or int.from_bytes(binary[18:20], 'little') != 183:
    sys.exit('Build did not produce a 64-bit little-endian ARM ELF binary')
version_file = pathlib.Path(sys.argv[3])
previous = version_file.read_text().strip()
if re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', previous) is None:
    sys.exit('VERSION_LAMP_OS must contain a numeric major.minor.patch version')
major, minor, patch = map(int, previous.split('.'))
version = f'{major}.{minor}.{patch + 1}'
version_file.write_text(version + '\n')
(root / 'VERSION').write_text(version + '\n')
print(f'Version bumped: {previous} -> {version}')
digest = hashlib.sha256(binary).hexdigest()
(root / 'SHA256SUMS').write_text(f'{digest}  lamp-os\n')
(root / 'build.json').write_text(json.dumps({
    'source_branch': 'lamp-voice', 'source_commit': commit, 'version': version,
    'crate': 'lamp-keeper', 'artifact': 'lamp-os',
    'target': 'aarch64-unknown-linux-gnu', 'profile': 'release',
    'sha256': digest, 'ota_entrypoint': 'bin/service',
    'complete_ota_package': False,
}, indent=2) + '\n')
PY
sha=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["sha256"])' "$work/artifacts/build.json")
version=$(cat "$work/artifacts/VERSION")
destination="gs://${GCS_BUCKET}/${BUCKET_PREFIX}/services/lamp-os/builds/${version}/${commit}/${sha}"
echo "Uploading binary and provenance to $destination/"
gsutil -h 'Cache-Control:no-cache, no-store, must-revalidate' cp \
  "$work/artifacts/lamp-os" "$work/artifacts/build.json" "$work/artifacts/SHA256SUMS" "$work/artifacts/VERSION" "$destination/"
# Verify the stored artifact before reporting success.
gsutil cp "$destination/lamp-os" "$work/uploaded-lamp-os"
python3 - "$work/uploaded-lamp-os" "$sha" <<'PY'
import hashlib, pathlib, sys
if hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest() != sys.argv[2]:
    sys.exit('Uploaded lamp-os SHA-256 mismatch')
PY
echo "Uploaded and verified: $destination/lamp-os"
echo 'Supervisor binary only; OTA metadata and promotion were not changed.'
