#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"

python3 - .github/workflows/*.yml <<'PY'
import pathlib
import re
import sys

sha = re.compile(r"^[0-9a-f]{40}(?:\s+#.*)?$")
for name in sys.argv[1:]:
    path = pathlib.Path(name)
    text = path.read_text(encoding="utf-8")
    if "permissions:" not in text:
        raise SystemExit(f"{path}: workflow must declare permissions")
    for number, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        if not stripped.startswith("uses:"):
            continue
        value = stripped.removeprefix("uses:").strip()
        if value.startswith("./"):
            continue
        if "@" not in value:
            raise SystemExit(f"{path}:{number}: Action reference has no revision")
        revision = value.rsplit("@", 1)[1]
        if not sha.fullmatch(revision):
            raise SystemExit(f"{path}:{number}: Action is not pinned to a full commit SHA")

release = pathlib.Path(".github/workflows/release.yml").read_text(encoding="utf-8")
verifier = pathlib.Path("scripts/verify-attestation-anonymously.sh").read_text(encoding="utf-8")
for path, text in [
    (pathlib.Path(".github/workflows/release.yml"), release),
    (pathlib.Path("scripts/verify-attestation-anonymously.sh"), verifier),
]:
    for number, line in enumerate(text.splitlines(), 1):
        command = line.strip()
        if command.startswith("jsonplaceholder ") or "$(jsonplaceholder " in command:
            raise SystemExit(
                f"{path}:{number}: provider name used as a release network command"
            )

network_paths = {
    "draft asset upload": "upload_asset() {\n            local name=$1\n            local output=$2\n            curl ",
    "package absence preflight": "status=$(curl --silent --show-error --location",
    "anonymous component download": "curl --fail --silent --show-error --location \\\n            \"$base/jsonplaceholder-provider.wasm\"",
    "anonymous checksum download": "curl --fail --silent --show-error --location \\\n            \"$base/jsonplaceholder-provider.wasm.sha256\"",
    "anonymous release metadata": "release=$(curl --fail --silent --show-error",
    "immutable-ID cleanup": "if ! status=$(curl \"${args[@]}\" \"$api/$endpoint\")",
}
for name, marker in network_paths.items():
    if marker not in release:
        raise SystemExit(f"release workflow lost curl-backed {name} path")
oras_download = (
    "curl --fail --silent --show-error --location \\\n"
    "            \"https://github.com/oras-project/oras/releases/download/"
)
if release.count(oras_download) != 3:
    raise SystemExit("release workflow must curl all three checksum-pinned ORAS downloads")
visibility_paths = {
    "captured initial visibility": "initial_visibility: ${{ steps.capture.outputs.initial_visibility }}",
    "private or public capture": '.visibility == "private" or .visibility == "public"',
    "immediate anonymous public verification": "initial-anonymous-auth.json",
    "draft-readable GHCR contents access": "contents: write # Read the exact captured draft by ID",
}
for name, marker in visibility_paths.items():
    if marker not in release:
        raise SystemExit(f"release workflow lost {name}")
if "status=$(curl --silent --show-error --location --get" not in verifier:
    raise SystemExit("anonymous attestation verifier lost its curl-backed fetch path")
if "for command in base64 curl gh jq mktemp" not in verifier:
    raise SystemExit("anonymous attestation verifier does not require its verification tools")
retry_markers = {
    "run ID input": "run_id=${8:?}",
    "run attempt input": "run_attempt=${9:?}",
    "current invocation binding": ".verificationResult.signature.certificate.runInvocationURI == $invocation",
    "exactly one current invocation": 'if [[ "$verified" -ne 1 ]]',
}
for name, marker in retry_markers.items():
    if marker not in verifier:
        raise SystemExit(f"anonymous attestation verifier lost {name}")
if release.count('            "$GITHUB_RUN_ID" \\\n            "$GITHUB_RUN_ATTEMPT"') != 4:
    raise SystemExit("all anonymous attestation checks must bind to this run attempt")

print("release, network, and initial-visibility source smoke checks passed")
PY

[[ "$(actionlint -version | head -1)" == *"1.7.12"* ]] || {
  echo "error: actionlint 1.7.12 is required" >&2
  exit 1
}
actionlint -color
printf 'workflow YAML and full-SHA Action references passed actionlint\n'
