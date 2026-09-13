#!/usr/bin/env bash
# Privileged imports must make every immediate/direct host fail closed.
#
# `dekopon-run` is retired — it stops at 0.11.1, an 0.11-era host that requires the deleted
# `idempotency` manifest field — so an empty Wasmtime linker is the whole check. It is also the
# stronger form: it refuses on the missing import itself, independently of any Dekopon host build.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
component=${1:-"$root/jsonplaceholder-provider.wasm"}
[[ -f "$component" ]]
[[ "$(wasmtime --version)" == "wasmtime 48.0.2" ]]

temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT

if wasmtime --invoke 'describe()' "$component" \
  >"$temporary/wasmtime.out" 2>"$temporary/wasmtime.err"; then
  echo "error: empty Wasmtime linker unexpectedly accepted the HTTP import" >&2
  exit 1
fi
grep -Fq "dekopon:http/client@1.0.0" "$temporary/wasmtime.err"
grep -Fq "imports instance" "$temporary/wasmtime.err"

printf 'verified direct refusal: empty Wasmtime linker rejects the HTTP-importing component\n'
