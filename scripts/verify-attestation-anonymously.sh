#!/usr/bin/env bash
# Fetch public GitHub attestations without credentials and verify the current run's local bundle.
set -euo pipefail

artifact=${1:?usage: verify-attestation-anonymously.sh ARTIFACT REPO DIGEST PREDICATE SIGNER SOURCE_REF SOURCE_DIGEST RUN_ID RUN_ATTEMPT}
repo=${2:?}
digest=${3:?}
predicate=${4:?}
signer=${5:?}
source_ref=${6:?}
source_digest=${7:?}
run_id=${8:?}
run_attempt=${9:?}
api=${GITHUB_API_URL:-https://api.github.com}

[[ -f "$artifact" ]]
[[ "$repo" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]]
[[ "$digest" =~ ^[0-9a-f]{64}$ ]]
[[ "$source_digest" =~ ^[0-9a-f]{40}$ ]]
[[ "$run_id" =~ ^[0-9]+$ ]]
[[ "$run_attempt" =~ ^[0-9]+$ ]]
for command in base64 curl gh jq mktemp; do
  command -v "$command" >/dev/null 2>&1 || {
    echo "error: $command is required" >&2
    exit 1
  }
done

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
response="$work/attestations.json"
status=
for _attempt in {1..12}; do
  status=$(curl --silent --show-error --location --get \
    --output "$response" --write-out '%{http_code}' \
    --header 'Accept: application/vnd.github+json' \
    --header 'X-GitHub-Api-Version: 2022-11-28' \
    --user-agent 'dekopon-provider-jsonplaceholder-anonymous-verifier/0.1.0' \
    --data-urlencode "predicate_type=$predicate" \
    "$api/repos/$repo/attestations/sha256:$digest")
  if [[ "$status" == 200 ]]; then
    break
  fi
  if [[ "$status" != 404 ]]; then
    echo "error: anonymous attestation fetch returned HTTP $status" >&2
    cat "$response" >&2
    exit 1
  fi
  sleep 5
done
[[ "$status" == 200 ]]
jq -e '.attestations | type == "array" and length > 0' "$response" >/dev/null

# Retries can legitimately leave older immutable attestations for the same digest. Verify each
# bundle under a clean gh configuration, then accept exactly the bundle whose certificate is bound
# to this workflow run attempt. The certificate field is signed GitHub OIDC identity, not predicate
# data controlled by repository code.
expected_invocation="https://github.com/$repo/actions/runs/$run_id/attempts/$run_attempt"
mkdir "$work/gh-config" "$work/candidates"
verified=0
selected=
while IFS= read -r encoded; do
  index=${encoded%%:*}
  bundle=${encoded#*:}
  bundle_path="$work/candidates/$index.bundle.json"
  result_path="$work/candidates/$index.result.json"
  printf '%s' "$bundle" | base64 --decode >"$bundle_path"
  if env -u GH_TOKEN -u GITHUB_TOKEN GH_CONFIG_DIR="$work/gh-config" \
    gh attestation verify "$artifact" \
      --bundle "$bundle_path" \
      --repo "$repo" \
      --predicate-type "$predicate" \
      --signer-workflow "$signer" \
      --source-ref "$source_ref" \
      --source-digest "$source_digest" \
      --deny-self-hosted-runners \
      --format json >"$result_path" 2>/dev/null &&
    jq -e --arg invocation "$expected_invocation" '
      length == 1 and
      .[0].verificationResult.signature.certificate.runInvocationURI == $invocation
    ' "$result_path" >/dev/null; then
    verified=$((verified + 1))
    selected=$bundle_path
  fi
done < <(jq -r '.attestations | to_entries[] | "\(.key):\(.value.bundle | @base64)"' "$response")

if [[ "$verified" -ne 1 ]]; then
  echo "error: expected one attestation for current invocation, verified $verified" >&2
  exit 1
fi
[[ -s "$selected" ]]
printf 'anonymously verified %s attestation for sha256:%s from %s\n' \
  "$predicate" "$digest" "$expected_invocation"
