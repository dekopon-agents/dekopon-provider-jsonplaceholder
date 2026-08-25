#!/usr/bin/env bash
# Download and verify only the immutable artifacts produced by tagged release run 32810190719.
# This one-off recovery helper never builds or substitutes provider bytes.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
destination=${1:?usage: recover-v0.1.0-artifacts.sh DESTINATION}

repo=dekopon-agents/dekopon-provider-jsonplaceholder
source_run_id=32810190719
source_run_attempt=1
source_workflow_id=341769021
source_sha=dc925dd23240d2dbd3bd9c534347fd33552bbdf6
tag=v0.1.0
tag_object=41aaaa84aee32013c232a4186e4bb717081de256
source_build_job_id=97687940482
source_attest_job_id=97691903267
source_draft_job_id=97691949408
source_ghcr_job_id=97692015109
source_cleanup_job_id=97692045391
component_artifact_id=9549973644
component_archive_sha=c2cfa4e3bde2d1ca2b8acb36a5708d8ffe533516c974556800af0b6381fa4d20
component_archive_size=78676
sbom_artifact_id=9549974073
sbom_archive_sha=f62317641fcae48c30d259516dabb7082724f015eef89cfc170ce6ec1148fa48
sbom_archive_size=6626
component_sha=9562744e6c209a447cafcfe09d11a50ea1926945a4b52099714c7328c2fd5e5d
checksum_sha=3cf61a7b144571d45232cddc8abeebba95c40d81f8d7cbb6cf0172de37144a9d
sbom_sha=22c324de452e9d23b458702adcc4886df2bb4f4bfacc44a1807f09edd0442bc5

: "${GH_TOKEN:?GH_TOKEN is required to read the captured Actions artifacts}"
[[ "${GITHUB_REPOSITORY:-$repo}" == "$repo" ]]
for command in base64 curl gh git jq shasum unzip; do
  command -v "$command" >/dev/null 2>&1 || {
    echo "error: $command is required" >&2
    exit 1
  }
done

run=$(gh api "repos/$repo/actions/runs/$source_run_id")
jq -e \
  --arg repo "$repo" \
  --arg sha "$source_sha" \
  --argjson id "$source_run_id" \
  --argjson attempt "$source_run_attempt" \
  --argjson workflow_id "$source_workflow_id" '
    .id == $id and
    .workflow_id == $workflow_id and
    .name == "Release v0.1.0" and
    .path == ".github/workflows/release.yml" and
    .event == "push" and
    .status == "completed" and
    .conclusion == "failure" and
    .head_branch == "v0.1.0" and
    .head_sha == $sha and
    .run_attempt == $attempt and
    .repository.full_name == $repo
  ' <<<"$run" >/dev/null

jobs=$(gh api "repos/$repo/actions/runs/$source_run_id/jobs?filter=all&per_page=100")
jq -e \
  --argjson build "$source_build_job_id" \
  --argjson attest "$source_attest_job_id" \
  --argjson draft "$source_draft_job_id" \
  --argjson ghcr "$source_ghcr_job_id" \
  --argjson cleanup "$source_cleanup_job_id" '
    ([.jobs[] | {id, name, conclusion}] | sort_by(.id)) as $jobs |
    (.total_count == 7) and
    any($jobs[];
      .id == $build and .name == "Build and verify immutable bytes" and
      .conclusion == "success"
    ) and
    any($jobs[];
      .id == $attest and .name == "Attest provenance and SBOM" and
      .conclusion == "success"
    ) and
    any($jobs[];
      .id == $draft and .name == "Create one owned draft release" and
      .conclusion == "success"
    ) and
    any($jobs[];
      .id == $ghcr and .name == "Create, validate, then expose one OCI layer" and
      .conclusion == "failure"
    ) and
    any($jobs[];
      .id == $cleanup and
      .name == "Roll back only immutable state owned by a failed or cancelled run" and
      .conclusion == "success"
    ) and
    any($jobs[];
      .name == "Reverify immutable publication state, then finalize" and
      .conclusion == "skipped"
    ) and
    any($jobs[];
      .name == "Anonymous public verification" and .conclusion == "skipped"
    )
  ' <<<"$jobs" >/dev/null
jq -e --argjson ghcr "$source_ghcr_job_id" '
  [.jobs[] | select(.id == $ghcr)][0] as $job |
  ([$job.steps[] | select(.name == "Verify bytes and the captured owned draft") |
    .conclusion] == ["failure"]) and
  ([$job.steps[] | select(.name == "Install checksum-pinned ORAS") |
    .conclusion] == ["skipped"])
' <<<"$jobs" >/dev/null

artifacts=$(gh api "repos/$repo/actions/runs/$source_run_id/artifacts?per_page=100")
jq -e \
  --arg component_digest "sha256:$component_archive_sha" \
  --arg sbom_digest "sha256:$sbom_archive_sha" \
  --arg sha "$source_sha" \
  --argjson component_id "$component_artifact_id" \
  --argjson component_size "$component_archive_size" \
  --argjson run_id "$source_run_id" \
  --argjson sbom_id "$sbom_artifact_id" \
  --argjson sbom_size "$sbom_archive_size" '
    .total_count == 2 and
    ([.artifacts[] | {
      id,
      name,
      size_in_bytes,
      expired,
      digest,
      run_id: .workflow_run.id,
      head_branch: .workflow_run.head_branch,
      head_sha: .workflow_run.head_sha
    }] | sort_by(.id)) == ([
      {
        id: $component_id,
        name: "release-component",
        size_in_bytes: $component_size,
        expired: false,
        digest: $component_digest,
        run_id: $run_id,
        head_branch: "v0.1.0",
        head_sha: $sha
      },
      {
        id: $sbom_id,
        name: "release-sbom",
        size_in_bytes: $sbom_size,
        expired: false,
        digest: $sbom_digest,
        run_id: $run_id,
        head_branch: "v0.1.0",
        head_sha: $sha
      }
    ] | sort_by(.id))
  ' <<<"$artifacts" >/dev/null

git -C "$root" fetch --force origin "refs/tags/$tag:refs/tags/$tag"
[[ "$(git -C "$root" cat-file -t "refs/tags/$tag")" == tag ]]
[[ "$(git -C "$root" rev-parse "refs/tags/$tag")" == "$tag_object" ]]
[[ "$(git -C "$root" rev-parse "refs/tags/$tag^{}")" == "$source_sha" ]]
git -C "$root" merge-base --is-ancestor "$source_sha" HEAD

rm -rf "$destination"
mkdir -p "$destination/component" "$destination/sbom" "$destination/archives"
download() {
  local artifact_id=$1
  local archive_sha=$2
  local output=$3
  gh api "repos/$repo/actions/artifacts/$artifact_id/zip" >"$output"
  [[ "$(shasum -a 256 "$output" | awk '{print $1}')" == "$archive_sha" ]]
}
download "$component_artifact_id" "$component_archive_sha" \
  "$destination/archives/release-component.zip"
download "$sbom_artifact_id" "$sbom_archive_sha" \
  "$destination/archives/release-sbom.zip"
unzip -q "$destination/archives/release-component.zip" -d "$destination/component"
unzip -q "$destination/archives/release-sbom.zip" -d "$destination/sbom"

component_files=$(cd "$destination/component" && find . -type f -print | LC_ALL=C sort)
sbom_files=$(cd "$destination/sbom" && find . -type f -print | LC_ALL=C sort)
[[ "$component_files" == $'./jsonplaceholder-provider.wasm\n./jsonplaceholder-provider.wasm.sha256' ]]
[[ "$sbom_files" == './jsonplaceholder-provider.cdx.json' ]]
[[ "$(shasum -a 256 "$destination/component/jsonplaceholder-provider.wasm" |
  awk '{print $1}')" == "$component_sha" ]]
[[ "$(shasum -a 256 "$destination/component/jsonplaceholder-provider.wasm.sha256" |
  awk '{print $1}')" == "$checksum_sha" ]]
[[ "$(shasum -a 256 "$destination/sbom/jsonplaceholder-provider.cdx.json" |
  awk '{print $1}')" == "$sbom_sha" ]]
[[ "$(cat "$destination/component/jsonplaceholder-provider.wasm.sha256")" == \
   "$component_sha  jsonplaceholder-provider.wasm" ]]
(cd "$destination/component" && shasum -a 256 -c jsonplaceholder-provider.wasm.sha256)
[[ "$(wc -c <"$destination/component/jsonplaceholder-provider.wasm" |
  tr -d '[:space:]')" == 277153 ]]
jq -e '
  .bomFormat == "CycloneDX" and
  .specVersion == "1.5" and
  .metadata.component.name == "dekopon-jsonplaceholder-provider" and
  (.components | length) == 42
' "$destination/sbom/jsonplaceholder-provider.cdx.json" >/dev/null

for predicate in 'https://slsa.dev/provenance/v1' 'https://cyclonedx.org/bom'; do
  "$root/scripts/verify-attestation-anonymously.sh" \
    "$destination/component/jsonplaceholder-provider.wasm" \
    "$repo" \
    "$component_sha" \
    "$predicate" \
    "$repo/.github/workflows/release.yml" \
    "refs/tags/$tag" \
    "$source_sha" \
    "$source_run_id" \
    "$source_run_attempt"
done

# The CycloneDX bundle must contain the byte-equivalent JSON document captured by the tagged run.
curl --fail --silent --show-error --location --get \
  --header 'Accept: application/vnd.github+json' \
  --data-urlencode "predicate_type=https://cyclonedx.org/bom" \
  "https://api.github.com/repos/$repo/attestations/sha256:$component_sha" \
  >"$destination/sbom-attestations.json"
jq -e '.attestations | type == "array" and length == 1' \
  "$destination/sbom-attestations.json" >/dev/null
jq -r '.attestations[0].bundle.dsseEnvelope.payload' \
  "$destination/sbom-attestations.json" | base64 --decode \
  >"$destination/sbom-statement.json"
jq -e \
  --arg sha "$component_sha" '
    ._type == "https://in-toto.io/Statement/v1" and
    .predicateType == "https://cyclonedx.org/bom" and
    .subject == [{name: "jsonplaceholder-provider.wasm", digest: {sha256: $sha}}]
  ' "$destination/sbom-statement.json" >/dev/null
jq -S '.predicate' "$destination/sbom-statement.json" >"$destination/attested-sbom.json"
jq -S '.' "$destination/sbom/jsonplaceholder-provider.cdx.json" >"$destination/source-sbom.json"
cmp "$destination/source-sbom.json" "$destination/attested-sbom.json"

printf 'verified immutable tagged artifacts: run=%s component=sha256:%s sbom=sha256:%s\n' \
  "$source_run_id" "$component_sha" "$sbom_sha"
